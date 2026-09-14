//! ensp-vbox-shim (https://github.com/LBXaaa/ensp-vbox-shim) 集成：
//! - 内嵌官方 v0.1.4-beta 整合包（最新 Release）
//! - 原生实现整合包 install.ps1 -Check 的全部检测项
//! - 一键安装 / 注册设备 / 卸载（流式输出到前端）

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

use crate::process::list_processes;

const SHIM_ZIP: &[u8] = include_bytes!("../assets/shim.zip");

const SHIM_DLL_SHA256: &str = "6d2aadce202a740e128add181dcac1b81ae060c1b508f1a6d8cfdbb2fef69efe";
const VARP_PATCHED_SHA256: &str = "f0107975ba1b04325af2d31189ee92833233c1163f4553600207789977f94451";
const VARP_PRISTINE_SHA256: &str = "5ae6817a9f2f05cfbb5f1f89af910007c22988c22bc02fdf2c44a67a9ff26eb5";
const VCRT_HASHES: [(&str, &str); 2] = [
    ("VCRUNTIME140.dll", "87fc734e0f2884985514edace58cf649a8ad67cb058dc7b7a4068f77af86810a"),
    ("MSVCP140.dll", "546ee2af2ffff02a34dbc1139bc6eb0eb5d67d83b3be782cfead374d29c8e01e"),
];
const CLSID_VBOX: &str = "{B1A7A4F2-47B9-4A1E-82B2-07CCD5323C3F}";
const SPOOF_VER: &str = "5.2.44";

#[derive(Debug, Clone, Serialize)]
pub struct ShimItem {
    pub id: String,
    pub title: String,
    pub status: String, // pass | warn | fail | info
    pub detail: String,
    #[serde(default)]
    pub fix: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShimStatus {
    pub items: Vec<ShimItem>,
    pub installed: bool,
    pub vbox_version: String,
    pub ensp_dir: String,
    pub vbox_dir: String,
    pub shim_version: String,
}

fn sha256_file(path: &std::path::Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let mut h = Sha256::new();
    h.update(&data);
    Some(format!("{:x}", h.finalize()))
}

/// 进程加固需关闭的冲突进程（安装时它们持有待替换文件的锁）。
/// eNSP 全家 + VirtualBox 全家，小写匹配。
const CONFLICT_PROCS: [&str; 10] = [
    "ensp_client.exe",
    "ensp_vboxserver.exe",
    "ensp_consoleserver.exe",
    "ensp_consvr.exe",
    "ensp_wvrpsvr.exe",
    "vboxsvc.exe",
    "vboxheadless.exe",
    "virtualbox.exe",
    "vboxnetdhcp.exe",
    "vboxnetnat.exe",
];

fn pf_roots() -> Vec<String> {
    let mut roots = vec![
        r"C:\Program Files".to_string(),
        r"C:\Program Files (x86)".to_string(),
    ];
    for c in (b'D'..=b'Z').map(char::from) {
        let d = format!("{c}:\\");
        if !Path::new(&d).is_dir() {
            continue;
        }
        for pf in ["Program Files", "Program Files (x86)"] {
            let p = format!("{d}{pf}");
            if Path::new(&p).is_dir() {
                roots.push(p);
            }
        }
    }
    roots
}

struct UninstallEntry {
    name: String,
    location: String,
    version: String,
}

/// 扫描 HKLM 卸载注册表（含 64/32 两个视图），InstallLocation 为空时
/// 从 UninstallString 反推安装目录，覆盖自定义安装路径的情况。
fn uninstall_entries() -> Vec<UninstallEntry> {
    let mut out = Vec::new();
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    for root in [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        if let Ok(k) = hk.open_subkey(root) {
            for sub in k.enum_keys().flatten() {
                if let Ok(sk) = k.open_subkey(&sub) {
                    let get = |v: &str| sk.get_value::<String, _>(v).unwrap_or_default();
                    let mut location = get("InstallLocation");
                    if location.is_empty() {
                        let un = get("UninstallString").trim().trim_matches('"').to_string();
                        if un.to_ascii_lowercase().ends_with(".exe") {
                            if let Some(parent) = Path::new(&un).parent() {
                                location = parent.to_string_lossy().into_owned();
                            }
                        }
                    }
                    out.push(UninstallEntry {
                        name: get("DisplayName"),
                        location,
                        version: get("DisplayVersion"),
                    });
                }
            }
        }
    }
    out
}

fn find_ensp() -> SoftInfo {
    // 1) 卸载注册表（支持自定义安装路径）
    for e in uninstall_entries() {
        if e.name.to_ascii_lowercase().contains("ensp")
            && Path::new(&e.location).join("eNSP_Client.exe").exists()
        {
            return SoftInfo {
                found: true,
                path: e.location,
                version: e.version,
                source: "卸载注册表".into(),
            };
        }
    }
    // 2) 各盘 Program Files 智能搜索
    for root in pf_roots() {
        let p = Path::new(&root).join(r"Huawei\eNSP");
        if p.join("eNSP_Client.exe").exists() {
            return SoftInfo {
                found: true,
                path: p.to_string_lossy().into_owned(),
                version: String::new(),
                source: "智能搜索".into(),
            };
        }
    }
    SoftInfo::default()
}

fn vbox_info_with_version(path: String, source: &str) -> SoftInfo {
    let version = vbox_version(&path);
    SoftInfo {
        found: true,
        path,
        version,
        source: source.into(),
    }
}

fn find_vbox() -> SoftInfo {
    // 1) HKLM\SOFTWARE\Oracle\VirtualBox InstallDir
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    for p in [
        r"SOFTWARE\Oracle\VirtualBox",
        r"SOFTWARE\WOW6432Node\Oracle\VirtualBox",
    ] {
        if let Ok(k) = hk.open_subkey(p) {
            if let Ok(d) = k.get_value::<String, _>("InstallDir") {
                if Path::new(&d).join("VBoxManage.exe").exists() {
                    return vbox_info_with_version(d, "VirtualBox 注册表");
                }
            }
        }
    }
    // 2) 卸载注册表（覆盖自定义安装路径）
    for e in uninstall_entries() {
        if e.name.to_ascii_lowercase().contains("virtualbox")
            && Path::new(&e.location).join("VBoxManage.exe").exists()
        {
            return vbox_info_with_version(e.location, "卸载注册表");
        }
    }
    // 3) 各盘 Program Files 智能搜索
    for root in pf_roots() {
        let p = Path::new(&root).join(r"Oracle\VirtualBox");
        if p.join("VBoxManage.exe").exists() {
            return vbox_info_with_version(p.to_string_lossy().into_owned(), "智能搜索");
        }
    }
    SoftInfo::default()
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SoftInfo {
    pub found: bool,
    pub path: String,
    pub version: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConflictProc {
    pub name: String,
    pub pid: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShimPreflight {
    pub ensp: SoftInfo,
    pub vbox: SoftInfo,
    pub conflicts: Vec<ConflictProc>,
    pub blockers: Vec<String>,
    pub ready: bool,
}

fn live_conflicts() -> Vec<ConflictProc> {
    list_processes()
        .into_iter()
        .filter(|p| CONFLICT_PROCS.contains(&p.name.to_ascii_lowercase().as_str()))
        .map(|p| ConflictProc { name: p.name, pid: p.pid })
        .collect()
}

pub fn shim_preflight() -> ShimPreflight {
    let ensp = find_ensp();
    let vbox = find_vbox();
    let conflicts = live_conflicts();

    let mut blockers: Vec<String> = Vec::new();
    if !ensp.found {
        blockers.push("未检测到 eNSP — 请先安装华为 eNSP".into());
    }
    if !vbox.found {
        blockers.push("未检测到 VirtualBox — shim 需要官方 VirtualBox 7.2.x".into());
    } else if !vbox.version.starts_with("7.2") {
        blockers.push(format!(
            "VirtualBox {} 不满足要求 — shim 需要 7.2.x 引擎",
            if vbox.version.is_empty() {
                "（版本未知）".to_string()
            } else {
                vbox.version.clone()
            }
        ));
    }
    if !conflicts.is_empty() {
        blockers.push(format!(
            "{} 个冲突进程正在运行，安装时会锁定待替换的文件",
            conflicts.len()
        ));
    }

    let ready = ensp.found
        && vbox.found
        && vbox.version.starts_with("7.2")
        && conflicts.is_empty();

    ShimPreflight {
        ensp,
        vbox,
        conflicts,
        blockers,
        ready,
    }
}

/// 结束所有冲突进程，返回两轮重试后仍存活的。调用方应为管理员。
pub fn shim_kill_conflicts() -> Vec<ConflictProc> {
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess};
    const PROCESS_TERMINATE: u32 = 0x0001;

    for _ in 0..2 {
        for p in list_processes() {
            if CONFLICT_PROCS.contains(&p.name.to_ascii_lowercase().as_str()) {
                unsafe {
                    let h = OpenProcess(PROCESS_TERMINATE, 0, p.pid);
                    if !h.is_null() {
                        let _ = TerminateProcess(h, 1);
                        windows_sys::Win32::Foundation::CloseHandle(h);
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(600));
        if live_conflicts().is_empty() {
            return Vec::new();
        }
    }
    live_conflicts()
}

fn vbox_dir() -> Option<String> {
    let i = find_vbox();
    i.found.then_some(i.path)
}

fn ensp_dir() -> Option<String> {
    let i = find_ensp();
    i.found.then_some(i.path)
}

fn vbox_version(dir: &str) -> String {
    let exe = std::path::Path::new(dir).join("VBoxManage.exe");
    match std::process::Command::new(&exe)
        .args(["--version"])
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

fn reg_str(path: &str, value: &str) -> Option<String> {
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    hk.open_subkey(path).ok()?.get_value(value).ok()
}

pub fn shim_status() -> ShimStatus {
    let mut items = Vec::new();
    let ensp = ensp_dir();
    let vbox = vbox_dir();
    let version = vbox.as_deref().map(vbox_version).unwrap_or_default();

    // 1. VirtualBox 7.2.x（shim 的硬性前提）
    let vbox72 = version.starts_with("7.2");
    items.push(ShimItem {
        id: "vbox72".into(),
        title: "VirtualBox 7.2.x 引擎".into(),
        status: if vbox72 {
            "pass".into()
        } else if version.is_empty() {
            "fail".into()
        } else {
            "fail".into()
        },
        detail: if version.is_empty() {
            "未检测到 VirtualBox".into()
        } else {
            format!(
                "当前版本 {} —— 垫片要求官方 VirtualBox 7.2.x（二进制实为 7.2.8 引擎）",
                version
            )
        },
        fix: if vbox72 {
            String::new()
        } else {
            "卸载 VirtualBox 5.2.x，安装官方 VirtualBox 7.2.x 后再运行一键安装。直链：download.virtualbox.org/virtualbox/7.2.8/VirtualBox-7.2.8-173730-Win.exe".into()
        },
    });

    // 2. VBox52.dll 垫片 × 4 加载位置
    let mut dll_ok = 0;
    if let Some(e) = &ensp {
        for (i, rel) in ["tools", "vboxserver", "", r"plugin\ngfw\tools\ngfw"]
            .iter()
            .enumerate()
        {
            let dir = if rel.is_empty() {
                std::path::PathBuf::from(e)
            } else {
                std::path::Path::new(e).join(rel)
            };
            let dll = dir.join("VBox52.dll");
            let (status, detail) = match sha256_file(&dll) {
                Some(h) if h == SHIM_DLL_SHA256 => {
                    dll_ok += 1;
                    ("pass", "已部署 ✓（哈希匹配 v0.1.4）".to_string())
                }
                Some(_) => ("warn", "存在但哈希不同（版本不符或已被改动）".to_string()),
                None => ("fail", "未部署".to_string()),
            };
            items.push(ShimItem {
                id: format!("dll_{i}"),
                title: format!("垫片 VBox52.dll [{}]", if rel.is_empty() { "根目录" } else { rel }),
                status: status.into(),
                detail,
                fix: if status == "pass" {
                    String::new()
                } else {
                    "运行下方「一键安装 shim」".into()
                },
            });
        }
    } else {
        items.push(ShimItem {
            id: "dll_0".into(),
            title: "垫片 VBox52.dll（4 个加载位置）".into(),
            status: "fail".into(),
            detail: "未找到 eNSP 安装目录".into(),
            fix: "请先安装华为 eNSP 1.3.00".into(),
        });
    }

    // 3. 注册表版本伪装
    let spoof = reg_str(r"SOFTWARE\WOW6432Node\Oracle\VirtualBox", "Version")
        .map(|v| v == SPOOF_VER)
        .unwrap_or(false);
    items.push(ShimItem {
        id: "spoof".into(),
        title: "注册表版本伪装".into(),
        status: if spoof { "pass" } else { "fail" }.into(),
        detail: format!(
            "HKLM\\SOFTWARE\\WOW6432Node\\Oracle\\VirtualBox\\Version = {}（目标 {}）",
            reg_str(r"SOFTWARE\WOW6432Node\Oracle\VirtualBox", "Version")
                .unwrap_or_else(|| "未设置".into()),
            SPOOF_VER
        ),
        fix: if spoof { String::new() } else { "运行下方「一键安装 shim」".into() },
    });

    // 4. CLSID 劫持
    let expected = ensp
        .as_ref()
        .map(|e| format!("{}\\tools\\VBox52.dll", e.trim_end_matches('\\')));
    let clsid_path = format!(
        r"SOFTWARE\Classes\WOW6432Node\CLSID\{}\InprocServer32",
        CLSID_VBOX
    );
    let clsid = reg_str(&clsid_path, "");
    let clsid_ok = matches!(&clsid, Some(v) if Some(v.as_str()) == expected.as_deref());
    items.push(ShimItem {
        id: "clsid".into(),
        title: "CLSID InprocServer32 劫持".into(),
        status: if clsid_ok { "pass" } else { "fail" }.into(),
        detail: format!(
            "{}\\(默认) = {}",
            clsid_path,
            clsid.clone().unwrap_or_else(|| "未设置".into())
        ),
        fix: if clsid_ok { String::new() } else { "运行下方「一键安装 shim」".into() },
    });

    // 5. VAR_Plugin.dll 补丁
    let varp = ensp
        .as_ref()
        .map(|e| std::path::Path::new(e).join(r"plugin\ar1000v\VAR_Plugin.dll"));
    let varp_status = match varp.as_ref().and_then(|p| sha256_file(p)) {
        Some(h) if h == VARP_PATCHED_SHA256 => ("pass", "已补丁 ✓（v0.1.4 预构建版）"),
        Some(h) if h == VARP_PRISTINE_SHA256 => ("fail", "出厂原版（28 站点偏移需重映射，必须补丁）"),
        Some(_) => ("warn", "非标准版本（哈希不同）"),
        None => ("info", "未找到（未安装 AR 设备包）"),
    };
    items.push(ShimItem {
        id: "varp".into(),
        title: "AR 插件 VAR_Plugin.dll".into(),
        status: varp_status.0.into(),
        detail: varp_status.1.into(),
        fix: if varp_status.0 == "pass" || varp_status.0 == "info" {
            String::new()
        } else {
            "运行下方「一键安装 shim」（会先写 .bak 备份，可逆）".into()
        },
    });

    // 6. x86 VC++ 运行时
    if let Some(v) = &vbox {
        for (name, expect) in VCRT_HASHES {
            let p = std::path::Path::new(v).join("x86").join(name);
            let (status, detail) = match sha256_file(&p) {
                Some(h) if h == expect => ("pass", "已部署 ✓".to_string()),
                Some(_) => ("pass", "存在（版本不同，亦可）".to_string()),
                None => ("fail", "缺失 ★（干净机会 error 40 / 0x800700C1）".to_string()),
            };
            items.push(ShimItem {
                id: format!("vcrt_{name}"),
                title: format!("x86 运行时 VBox\\x86\\{name}"),
                status: status.into(),
                detail,
                fix: if status == "pass" { String::new() } else { "运行下方「一键安装 shim」".into() },
            });
        }
    }

    // 7. Host-Only 网卡（VBox 7 同样需要）
    if let Some(v) = &vbox {
        let out = std::process::Command::new(std::path::Path::new(v).join("VBoxManage.exe"))
            .args(["list", "hostonlyifs"])
            .output();
        let count = out
            .map(|o| String::from_utf8_lossy(&o.stdout).matches("VBoxNetworkName").count())
            .unwrap_or(0);
        items.push(ShimItem {
            id: "hostonly".into(),
            title: "Host-Only 虚拟网卡".into(),
            status: if count > 0 { "pass" } else { "fail" }.into(),
            detail: if count > 0 {
                format!("检测到 {} 块", count)
            } else {
                "没有 Host-Only 网卡，设备间/宿主机互通需要它".into()
            },
            fix: if count > 0 {
                String::new()
            } else {
                "VirtualBox -> 工具 -> 网络 -> Host-Only 网络 添加（并启用 DHCP）".into()
            },
        });
    }

    let installed = vbox72
        && dll_ok == 4
        && spoof
        && clsid_ok
        && varp_status.0 == "pass";

    ShimStatus {
        items,
        installed,
        vbox_version: version,
        ensp_dir: ensp.unwrap_or_default(),
        vbox_dir: vbox.clone().unwrap_or_default(),
        shim_version: "v0.1.4-beta".into(),
    }
}

// ---------------------------------------------------------------------------
// one-click actions
// ---------------------------------------------------------------------------

static SHIM_RUNNING: AtomicBool = AtomicBool::new(false);

fn extract_shim() -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join("fuck_ensp_shim");
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let zip = dir.join("shim.zip");
    std::fs::write(&zip, SHIM_ZIP).map_err(|e| e.to_string())?;
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "Expand-Archive -Force -Path '{}' -DestinationPath '{}'",
                zip.display(),
                dir.display()
            ),
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "解压失败: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(dir)
}

fn run_shim_ps1(app: tauri::AppHandle, script: &str, extra_args: &[&str]) -> Result<(), String> {
    if SHIM_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("已有 shim 任务在运行".into());
    }
    let dir = extract_shim()?;
    let script_owned = script.to_string();
    let args_owned: Vec<String> = extra_args.iter().map(|s| s.to_string()).collect();
    let script_path = dir.join(&script_owned);
    if !script_path.exists() {
        SHIM_RUNNING.store(false, Ordering::SeqCst);
        return Err(format!("整合包内缺少 {script}"));
    }

    std::thread::spawn(move || {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut args: Vec<String> = vec![
            "-NoProfile".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-File".into(),
            script_path.to_string_lossy().into_owned(),
        ];
        args.extend(args_owned.iter().cloned());
        let mut child = match std::process::Command::new("powershell")
            .args(&args)
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = app.emit("shim-exit", serde_json::json!({"code": -1, "error": e.to_string()}));
                SHIM_RUNNING.store(false, Ordering::SeqCst);
                return;
            }
        };

        let emit_lines = |pipe: std::process::ChildStdout, app: &tauri::AppHandle| {
            let reader = BufReader::new(pipe);
            for line in reader.lines().map_while(Result::ok) {
                let _ = app.emit("shim-log", line);
            }
        };
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let app2 = app.clone();
        let t1 = stdout.map(|s| std::thread::spawn(move || emit_lines(s, &app2)));
        let app3 = app.clone();
        let t2 = stderr.map(|s| {
            std::thread::spawn(move || {
                let reader = BufReader::new(s);
                for line in reader.lines().map_while(Result::ok) {
                    let _ = app3.emit("shim-log", format!("[stderr] {line}"));
                }
            })
        });

        let code = child.wait().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
        if let Some(t) = t1 {
            let _ = t.join();
        }
        if let Some(t) = t2 {
            let _ = t.join();
        }
        let _ = app.emit("shim-exit", serde_json::json!({"code": code}));
        SHIM_RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(())
}

pub fn shim_install(app: tauri::AppHandle) -> Result<(), String> {
    run_shim_ps1(app, "install_all.ps1", &[])
}

pub fn shim_uninstall(app: tauri::AppHandle) -> Result<(), String> {
    run_shim_ps1(app, "install.ps1", &["-Uninstall"])
}

pub fn shim_register_vms(app: tauri::AppHandle) -> Result<(), String> {
    run_shim_ps1(app, "register_vms.ps1", &[])
}
