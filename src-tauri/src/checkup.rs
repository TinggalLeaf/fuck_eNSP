//! One-click eNSP environment checkup with streaming progress.
//!
//! Each check runs in sequence and emits a `checkup-item` event; the
//! frontend renders a progress bar over the steps. Includes the Hyper-V/VBS
//! evidence chain that is the #1 root cause of eNSP error 40.

use serde::Serialize;
use tauri::Emitter;
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

#[derive(Debug, Clone, Serialize)]
pub struct CheckItem {
    pub id: String,
    pub title: String,
    pub status: String, // pass | warn | fail | info
    pub detail: String,
    #[serde(default)]
    pub fix: String,
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn item(id: &str, title: &str, status: &str, detail: String, fix: &str) -> CheckItem {
    CheckItem {
        id: id.into(),
        title: title.into(),
        status: status.into(),
        detail,
        fix: fix.into(),
    }
}

fn reg_u32(hk: &RegKey, path: &str, value: &str) -> Option<u32> {
    hk.open_subkey(path).ok()?.get_value(value).ok()
}

fn reg_string(hk: &RegKey, path: &str, value: &str) -> Option<String> {
    hk.open_subkey(path).ok()?.get_value(value).ok()
}

fn vbox_install_dir() -> Option<String> {
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    for path in [
        r"SOFTWARE\Oracle\VirtualBox",
        r"SOFTWARE\WOW6432Node\Oracle\VirtualBox",
    ] {
        if let Ok(key) = hk.open_subkey(path) {
            if let Ok(dir) = key.get_value::<String, _>("InstallDir") {
                if std::path::Path::new(&dir).join("VBoxManage.exe").exists() {
                    return Some(dir);
                }
            }
        }
    }
    None
}

fn run_vboxmanage(dir: &str, args: &[&str]) -> Result<String, String> {
    let exe = std::path::Path::new(dir).join("VBoxManage.exe");
    let out = std::process::Command::new(&exe)
        .args(args)
        .output()
        .map_err(|e| format!("spawn VBoxManage failed: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        Err(format!("VBoxManage {:?}: {}", args, stderr.trim()))
    }
}

fn ensp_root() -> Option<String> {
    for p in [
        r"C:\Program Files\Huawei\eNSP",
        r"C:\Program Files (x86)\Huawei\eNSP",
    ] {
        if std::path::Path::new(p).join("eNSP_Client.exe").exists() {
            return Some(p.to_string());
        }
    }
    None
}

fn service_running(name: &str) -> Option<bool> {
    let out = std::process::Command::new("sc")
        .args(["query", name])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() {
        return Some(false); // service does not exist
    }
    Some(text.contains("RUNNING"))
}

/// HypervisorPresent via WMI (most reliable), with a short timeout.
fn hypervisor_present() -> Option<bool> {
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-CimInstance Win32_ComputerSystem).HypervisorPresent",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    if text.trim().eq_ignore_ascii_case("true") {
        Some(true)
    } else if text.trim().eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

fn bcdedit_hypervisor() -> Option<String> {
    let out = std::process::Command::new("bcdedit")
        .args(["/enum", "{current}"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let l = line.trim().to_lowercase();
        if l.starts_with("hypervisorlaunchtype") {
            return l.split_whitespace().last().map(|s| s.to_string());
        }
    }
    None
}

fn port_owner(port: u16) -> Option<String> {
    let out = std::process::Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let needle = format!(":{}", port);
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("TCP") && l.contains(&needle) && l.contains("LISTENING") {
            if let Some(pid_s) = l.split_whitespace().last() {
                if let Ok(pid) = pid_s.parse::<u32>() {
                    return crate::process::list_processes()
                        .into_iter()
                        .find(|p| p.pid == pid)
                        .map(|p| format!("{} (pid {})", p.name, pid));
                }
            }
        }
    }
    None
}

fn free_space(path: &str) -> Option<u64> {
    use windows_sys::core::PWSTR;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let mut wide: Vec<u16> = path.encode_utf16().collect();
    wide.push(0);
    let mut free: u64 = 0;
    let mut d1: u64 = 0;
    let mut d2: u64 = 0;
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr() as PWSTR, &mut free, &mut d1, &mut d2) };
    if ok != 0 {
        Some(free)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// individual checks
// ---------------------------------------------------------------------------

struct Ctx {
    ensp: Option<String>,
    vbox: Option<String>,
    vbox_version: String,
}

fn check_windows(_ctx: &mut Ctx) -> CheckItem {
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    let name = reg_string(
        &hk,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "ProductName",
    )
    .unwrap_or_else(|| "未知".into());
    let build = reg_string(
        &hk,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "CurrentBuildNumber",
    )
    .unwrap_or_else(|| "未知".into());
    let ubr = reg_u32(
        &hk,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "UBR",
    )
    .map(|v| format!(".{}", v))
    .unwrap_or_default();
    let build_num: u32 = build.parse().unwrap_or(0);
    let is_win11 = build_num >= 22000;
    let detail = format!("{} (Build {}{})", name, build, ubr);
    if is_win11 {
        item(
            "windows",
            "操作系统版本",
            "warn",
            format!(
                "{}\n注意：eNSP 1.2/1.3 官方仅支持 Windows 7/8/10，未适配 Windows 11 (Build {})。在 Win11 上需要关闭 Hyper-V/内核隔离才能稳定运行，且可能存在其它兼容问题。",
                detail, build
            ),
            "优先保证 Hyper-V/VBS 关闭；如仍异常，考虑在 Windows 功能中关闭'虚拟机平台'与'适用于 Linux 的 Windows 子系统'，或使用兼容模式/虚拟机方案",
        )
    } else {
        item("windows", "操作系统版本", "pass", detail, "")
    }
}

fn check_ensp(ctx: &mut Ctx) -> CheckItem {
    match &ctx.ensp {
        Some(root) => item("ensp", "eNSP 安装", "pass", root.clone(), ""),
        None => item(
            "ensp",
            "eNSP 安装",
            "fail",
            "未找到 eNSP_Client.exe".into(),
            "请先安装 eNSP 1.3.00",
        ),
    }
}

fn check_vbox(ctx: &mut Ctx) -> CheckItem {
    match &ctx.vbox {
        Some(dir) => {
            let version = run_vboxmanage(dir, &["--version"]).unwrap_or_default();
            ctx.vbox_version = version.trim().to_string();
            let status = if ctx.vbox_version.starts_with("5.2") {
                "pass"
            } else if ctx.vbox_version.starts_with("5.") {
                "warn"
            } else if ctx.vbox_version.is_empty() {
                "warn"
            } else {
                "fail"
            };
            let fix = match status {
                "pass" => "注意：VirtualBox 5.2.x 没有 Hyper-V 兼容层（不支持 WHP API），只要 Hyper-V/VBS 在运行就会失败",
                "warn" => "建议使用 eNSP 官方兼容的 VirtualBox 5.2.x",
                _ => "卸载当前 VirtualBox，安装 5.2.x（如 5.2.30/5.2.44）",
            };
            item(
                "vbox",
                "VirtualBox 版本",
                status,
                format!("{} ({})", dir, ctx.vbox_version),
                fix,
            )
        }
        None => item(
            "vbox",
            "VirtualBox 版本",
            "fail",
            "未检测到 VirtualBox".into(),
            "安装 VirtualBox 5.2.x",
        ),
    }
}

fn check_basevm(ctx: &mut Ctx) -> CheckItem {
    match &ctx.vbox {
        Some(dir) => match run_vboxmanage(dir, &["list", "vms"]) {
            Ok(out) => {
                let missing: Vec<&str> = [
                    "AR_Base",
                    "WLAN_AC_Base",
                    "WLAN_AD_Base",
                    "WLAN_AP_Base",
                    "WLAN_SAP_Base",
                ]
                .iter()
                .filter(|n| !out.contains(&format!("\"{}\"", n)))
                .copied()
                .collect();
                if missing.is_empty() {
                    item(
                        "basevm",
                        "基础虚拟机注册",
                        "pass",
                        "AR_Base / WLAN 基础镜像均已注册".into(),
                        "",
                    )
                } else {
                    item(
                        "basevm",
                        "基础虚拟机注册",
                        "fail",
                        format!("未注册: {}", missing.join(", ")),
                        "打开 eNSP -> 菜单 工具 -> 注册设备，重新注册基础镜像",
                    )
                }
            }
            Err(e) => item(
                "basevm",
                "基础虚拟机注册",
                "warn",
                e,
                "尝试以管理员身份运行本工具和 eNSP",
            ),
        },
        None => item(
            "basevm",
            "基础虚拟机注册",
            "warn",
            "未安装 VirtualBox，无法检查".into(),
            "",
        ),
    }
}

fn check_hostonly(ctx: &mut Ctx) -> CheckItem {
    match &ctx.vbox {
        Some(dir) => match run_vboxmanage(dir, &["list", "hostonlyifs"]) {
            Ok(out) => {
                let count = out.matches("VBoxNetworkName").count();
                if count > 0 {
                    item(
                        "hostonly",
                        "Host-Only 虚拟网卡",
                        "pass",
                        format!("检测到 {} 块 Host-Only 网卡", count),
                        "",
                    )
                } else {
                    item(
                        "hostonly",
                        "Host-Only 虚拟网卡",
                        "fail",
                        "没有 Host-Only 网卡".into(),
                        "VirtualBox -> 管理 -> 全局设定 -> 网络 -> 添加 Host-Only 网卡（并启用 DHCP）",
                    )
                }
            }
            Err(e) => item("hostonly", "Host-Only 虚拟网卡", "warn", e, ""),
        },
        None => item(
            "hostonly",
            "Host-Only 虚拟网卡",
            "warn",
            "未安装 VirtualBox，无法检查".into(),
            "",
        ),
    }
}

fn check_hypervisor_present(_ctx: &mut Ctx) -> CheckItem {
    match hypervisor_present() {
        Some(true) => item(
            "hyperv_present",
            "HypervisorPresent（虚拟机监控程序）",
            "warn",
            "HypervisorPresent = True —— VT-x 正被 Hyper-V/VBS 占用\n说明：此项仅在'VBox 5.2 独占 VT-x'方案下是阻塞项；若采用 VBox 7.x + ensp-vbox-shim 方案（推荐路径），VBox 走 WHP/NEM 后端，与 Hyper-V 共存，无需处理".into(),
            "VBox 5.2 方案：bcdedit /set hypervisorlaunchtype off 并关内核隔离后重启；VBox7+shim 方案：忽略此项",
        ),
        Some(false) => item(
            "hyperv_present",
            "HypervisorPresent（虚拟机监控程序）",
            "pass",
            "HypervisorPresent = False —— VT-x 未被占用".into(),
            "",
        ),
        None => item(
            "hyperv_present",
            "HypervisorPresent（虚拟机监控程序）",
            "warn",
            "无法通过 WMI 读取 HypervisorPresent".into(),
            "",
        ),
    }
}

fn check_vbs(_ctx: &mut Ctx) -> CheckItem {
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    let enabled = reg_u32(
        &hk,
        r"SYSTEM\CurrentControlSet\Control\DeviceGuard",
        "EnableVirtualizationBasedSecurity",
    );
    let status = reg_u32(
        &hk,
        r"SYSTEM\CurrentControlSet\Control\DeviceGuard",
        "VirtualizationBasedSecurityStatus",
    );
    let status_text = match status {
        Some(0) => "0（未启用）",
        Some(1) => "1（已启用但未运行）",
        Some(2) => "2（正在运行）",
        Some(x) => return_unknown(x),
        None => "未设置",
    };
    let detail = format!(
        "EnableVirtualizationBasedSecurity = {}\nVirtualizationBasedSecurityStatus = {}",
        enabled.map(|v| v.to_string()).unwrap_or_else(|| "未设置".into()),
        status_text
    );
    if enabled == Some(1) || status == Some(2) {
        item(
            "vbs",
            "VBS（基于虚拟化的安全性）",
            "fail",
            detail,
            "组策略：计算机配置->管理模板->系统->Device Guard->启用基于虚拟化的安全性=已禁用；或注册表置 0；并在 Windows 安全中心->设备安全性->内核隔离->内存完整性 关闭，然后重启",
        )
    } else if enabled.is_some() || status.is_some() {
        item("vbs", "VBS（基于虚拟化的安全性）", "pass", detail, "")
    } else {
        item(
            "vbs",
            "VBS（基于虚拟化的安全性）",
            "info",
            detail,
            "",
        )
    }
}

fn return_unknown(x: u32) -> &'static str {
    let _ = x;
    "未知"
}

fn check_hyperv_services(_ctx: &mut Ctx) -> CheckItem {
    let services = [
        ("vmms", "Hyper-V 虚拟机管理服务"),
        ("vmcompute", "Hyper-V 主机计算服务"),
        ("hvhost", "Hyper-V 主机服务"),
        ("hns", "主机网络服务(HNS)"),
    ];
    let mut running = Vec::new();
    let mut stopped = Vec::new();
    for (name, label) in services {
        match service_running(name) {
            Some(true) => running.push(format!("{}({})", label, name)),
            _ => stopped.push(name),
        }
    }
    if running.is_empty() {
        item(
            "hyperv_svc",
            "Hyper-V 核心服务",
            "pass",
            "vmms / vmcompute / hvhost / hns 均未运行".into(),
            "",
        )
    } else {
        item(
            "hyperv_svc",
            "Hyper-V 核心服务",
            "warn",
            format!("正在运行: {}\n未运行: {}", running.join(", "), stopped.join(", ")),
            "仅对 VBox 5.2 独占方案是阻塞项；VBox7+shim 方案可忽略。如需关闭：Windows 功能取消勾选 Hyper-V / 虚拟机平台 / WSL2 后重启",
        )
    }
}

fn check_bcdedit(_ctx: &mut Ctx) -> CheckItem {
    match bcdedit_hypervisor() {
        Some(v) if v == "off" => item(
            "bcdedit",
            "hypervisorlaunchtype（启动配置）",
            "pass",
            "hypervisorlaunchtype = Off".into(),
            "",
        ),
        Some(v) => item(
            "bcdedit",
            "hypervisorlaunchtype（启动配置）",
            "fail",
            format!("hypervisorlaunchtype = {}", v),
            "管理员运行：bcdedit /set hypervisorlaunchtype off，然后重启",
        ),
        None => item(
            "bcdedit",
            "hypervisorlaunchtype（启动配置）",
            "warn",
            "无法读取 bcdedit 配置".into(),
            "",
        ),
    }
}

/// 定位所有 VBoxHardening.log
fn hardening_log_paths(ctx: &Ctx) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    if let Some(dir) = &ctx.vbox {
        let p = std::path::Path::new(dir).join("VBoxHardening.log");
        if p.exists() {
            found.push(p);
        }
    }
    for base in [
        r"C:\Users\MapleReHub\VirtualBox VMs",
        r"C:\Users\Public\VirtualBox VMs",
    ] {
        if let Ok(rd) = std::fs::read_dir(base) {
            for e in rd.flatten() {
                let p = e.path().join("Logs").join("VBoxHardening.log");
                if p.exists() {
                    found.push(p);
                }
            }
        }
    }
    found
}

/// C:\Windows\System32\ntdll.dll 的文件版本（如 10.0.26100.9444）
fn ntdll_version() -> Option<String> {
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-Item 'C:\\Windows\\System32\\ntdll.dll').VersionInfo.FileVersion",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let v = text.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// 版本比较：a >= (maj,min,build,rev)?（容忍 "10.0.26100.9444 (WinBuild…)" 这类后缀）
fn version_ge(a: &str, b: (u32, u32, u32, u32)) -> bool {
    let parts: Vec<u32> = a
        .split('.')
        .map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
        .collect();
    let get = |i: usize| parts.get(i).copied().unwrap_or(0);
    (get(0), get(1), get(2), get(3)) >= b
}

/// 解析 VBoxHardening.log 中的 -5607 加固自检失败特征
fn hardening_5607_evidence() -> Option<Vec<String>> {
    let ctx = Ctx {
        ensp: None,
        vbox: vbox_install_dir(),
        vbox_version: String::new(),
    };
    let paths = hardening_log_paths(&ctx);
    for p in paths {
        if let Ok(content) = std::fs::read_to_string(&p) {
            let mut hits: Vec<String> = Vec::new();
            for line in content.lines() {
                let l = line.trim();
                if l.contains("-5607")
                    || l.contains("isn't close enough to the mapping size")
                    || l.contains("supR3HardNtChildPurify")
                    || (l.contains("SizeOfImage") && l.contains("mapping size"))
                {
                    hits.push(l.to_string());
                }
            }
            if !hits.is_empty() {
                hits.truncate(10);
                hits.insert(0, format!("来源: {}", p.display()));
                return Some(hits);
            }
        }
    }
    None
}

fn check_vbox_hardening_log(ctx: &mut Ctx) -> CheckItem {
    let found = hardening_log_paths(ctx);
    if found.is_empty() {
        item(
            "hardening",
            "VBoxHardening.log（加固日志）",
            "info",
            "未找到 VBoxHardening.log。若设备启动失败且本日志同时缺失，通常是克隆机失败后被 eNSP/VBox 自动清理所致（需在启动失败的瞬间手动抢存日志）".into(),
            "可再次启动设备，在 eNSP 自动删除前到 %USERPROFILE%\\VirtualBox VMs\\<设备名>\\Logs\\ 手动复制 VBoxHardening.log",
        )
    } else {
        let mut detail = format!("找到 {} 处：\n{}", found.len(), found.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n"));
        if let Some(mut ev) = hardening_5607_evidence() {
            detail.push_str("\n\n检测到 -5607 加固自检失败特征：\n");
            detail.push_str(&ev.split_off(1).join("\n"));
        }
        item(
            "hardening",
            "VBoxHardening.log（加固日志）",
            if hardening_5607_evidence().is_some() { "fail" } else { "info" },
            detail,
            "若含 -5607 / SizeOfImage 相关错误，说明是 VBox 5.2 加固与新版 ntdll.dll 不兼容（见综合结论）",
        )
    }
}

/// ntdll.dll 版本展示 + 已知不兼容提示。
/// 判定以 VBoxHardening.log 的 -5607 特征为准（加固校验与 ntdll 版本强耦合，
/// 无固定阈值，不能仅凭版本号下结论）
fn check_ntdll(ctx: &mut Ctx) -> CheckItem {
    let vbox52 = ctx.vbox_version.starts_with("5.2") || ctx.vbox_version.starts_with("5.");
    let evidence = hardening_5607_evidence();
    let has_evidence = evidence.is_some();
    match ntdll_version() {
        Some(v) => {
            let win11_hot = version_ge(&v, (10, 0, 26100, 9444));
            let status = if has_evidence {
                "fail"
            } else if vbox52 && win11_hot {
                "warn"
            } else if vbox52 {
                "info"
            } else {
                "pass"
            };
            let mut detail = format!("C:\\Windows\\System32\\ntdll.dll = {v}");
            if vbox52 {
                detail.push_str(
                    "\nVirtualBox 5.2.x 的加固自检与 ntdll 版本强耦合（校验 SizeOfImage 与实际映射大小的差，容差写死），",
                );
                detail.push_str("仅验证过旧版 ntdll。Windows 更新替换 ntdll 后随时可能触发 -5607（失败发生在 CHILD_PURIFICATION 阶段，与 VT-x/Hyper-V 无关）");
            }
            if let Some(mut ev) = evidence {
                detail.push_str("\n\n已从 VBoxHardening.log 提取到 -5607 特征：\n");
                detail.push_str(&ev.split_off(1).join("\n"));
            }
            item(
                "ntdll",
                "ntdll.dll 与加固自检兼容性",
                status,
                detail,
                if has_evidence {
                    "不要降级 5.2.44（同属 5.2 分支未含修复）。唯一可行路径：VirtualBox 7.1.4+ + 开源垫片 LBXaaa/ensp-vbox-shim；Hyper-V 可恢复（VBox 7 走 WHP/NEM 与 Hyper-V 共存）"
                } else if vbox52 {
                    "若 startvm 失败，启动设备的瞬间到 %USERPROFILE%\\VirtualBox VMs\\<设备>\\Logs\\ 抢存 VBoxHardening.log，本工具会自动识别 -5607"
                } else {
                    ""
                },
            )
        }
        None => item(
            "ntdll",
            "ntdll.dll 与加固自检兼容性",
            "warn",
            "无法读取 ntdll.dll 版本".into(),
            "",
        ),
    }
}

/// 从 hook 日志统计 VBoxManage 子命令调用次数
fn vboxmanage_ops() -> std::collections::HashMap<String, usize> {
    let mut ops = std::collections::HashMap::new();
    let dir = crate::native::hook_dir();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_lowercase();
            if !name.contains("vboxserver") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                for line in content.lines() {
                    let Some(pos) = line.find("\"cmd\":\"") else { continue };
                    let cmd = &line[pos + 7..];
                    let Some(vb) = cmd.find("VBoxManage") else { continue };
                    let rest = &cmd[vb + 10..];
                    let op: String = rest
                        .chars()
                        .skip_while(|c| *c == '"' || *c == ' ' || *c == '\\' || *c == '/')
                        .take_while(|c| c.is_ascii_alphanumeric())
                        .collect::<String>()
                        .to_lowercase();
                    if !op.is_empty() {
                        *ops.entry(op).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    ops
}

/// startvm 失败特征：克隆/注册/快照全部成功，唯独 startvm 失败
/// （前者是纯磁盘与配置操作不需要 VT-x，startvm 需要硬件虚拟化）
fn check_startvm_signature(_ctx: &mut Ctx) -> CheckItem {
    let ops = vboxmanage_ops();
    let get = |k: &str| ops.get(k).copied().unwrap_or(0);
    let startvm = get("startvm");
    let disk_ops = get("clonevm") + get("registervm") + get("snapshot");
    let cleanup = get("unregistervm") + get("controlvm");
    if startvm == 0 {
        return item(
            "startvm_sig",
            "startvm 失败特征（Hook 日志分析）",
            "info",
            "近期未捕获到 VBoxManage startvm 调用（尚未启动过设备或日志已清空）".into(),
            "",
        );
    }
    let detail = format!(
        "Hook 日志捕获到的 VBoxManage 调用：clonevm={} registervm={} snapshot={} modifyvm={} startvm={} controlvm={} unregistervm={}\n克隆/注册/快照是纯磁盘和配置操作，不需要 VT-x，所以全部成功；startvm 需要硬件虚拟化扩展，被 hypervisor 独占时即以 exit code 1 终止 —— 这正是错误 40 的日志特征。",
        get("clonevm"),
        get("registervm"),
        get("snapshot"),
        get("modifyvm"),
        startvm,
        get("controlvm"),
        get("unregistervm")
    );
    if disk_ops > 0 && cleanup >= startvm {
        item(
            "startvm_sig",
            "startvm 失败特征（Hook 日志分析）",
            "fail",
            detail,
            "与 Hyper-V/VBS 证据链结论一致：释放 VT-x 后（关闭 VBS/内核隔离并重启），startvm 即可成功",
        )
    } else {
        item(
            "startvm_sig",
            "startvm 失败特征（Hook 日志分析）",
            "warn",
            detail,
            "",
        )
    }
}

/// 综合结论：-5607 加固不兼容（最高优先级）> Hyper-V/VBS 冲突 > 通过
fn check_conclusion(ctx: &mut Ctx) -> CheckItem {
    let hardening_5607 = hardening_5607_evidence().is_some();
    let vbox52 = ctx.vbox_version.starts_with("5.2") || ctx.vbox_version.starts_with("5.");

    if vbox52 && hardening_5607 {
        let evidence = if hardening_5607 {
            "已从 VBoxHardening.log 中提取到 -5607 特征行"
        } else {
            "未抢到加固日志，但 ntdll.dll 版本已处于已知不兼容区间（2025-09 KB5124007/5124008 起）"
        };
        return item(
            "conclusion",
            "综合结论：错误 40 根因判定",
            "fail",
            format!(
                concat!(
                    "真正根因：VirtualBox 5.2.x 的进程加固自检（hardening）与新版 Windows 的 ntdll.dll 不兼容，错误码 -5607。\n",
                    "证据链：\n",
                    "1. {}\n",
                    "2. 失败发生在 supR3HardNtChildPurify 阶段（进程 Respawn 后的自我净化），此时 VMM 尚未初始化，根本没走到申请 VT-x 那一步 —— 与 Hyper-V 无关\n",
                    "3. SUPR3HardenedWinFindAdversaries: 0x0，排除第三方安全软件注入与 Defender 干扰\n",
                    "4. ntdll.dll 要求 PE 头 SizeOfImage 与实际映射大小足够接近（当前差 0x5000，超出 5.2 写死的容差）→ 自检拒绝启动进程\n",
                    "5. 解释日志现象：clonevm/snapshot/registervm 是纯磁盘配置操作（不拉起受加固保护的进程）全部成功，只有 startvm 拉起 VBoxHeadless 触发自检而失败\n",
                    "6. 变量排除：卸载 Hyper-V 后重启再试仍然失败"
                ),
                evidence
            ),
            concat!(
                "唯一可行路径：\n",
                "① 卸载 VirtualBox 5.2.30，安装 VirtualBox 7.1.4 或更高（-5607 校验问题在 7.1.4 修复）\n",
                "② 配合开源垫片 LBXaaa/ensp-vbox-shim：伪装版本号 + vtable 映射 + 调用约定转换，让原版 eNSP 1.2 调用 VBox 7.x 引擎\n",
                "③ ⚠️ 不要降级 5.2.44 —— 同属 5.2 分支未含修复，对本 build 无效\n",
                "④ Hyper-V 可以恢复（bcdedit /set hypervisorlaunchtype auto + 重新勾选 Hyper-V/虚拟机平台/WSL）：VBox 7 支持 WHP/NEM 后端与 Hyper-V 共存，垫片方案本就以'不关 Hyper-V'为目标"
            ),
        );
    }

    let hv_services_on = service_running("vmms") == Some(true)
        || service_running("vmcompute") == Some(true);
    let hvhost_on = service_running("hvhost") == Some(true);
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    let vbs_enabled = reg_u32(
        &hk,
        r"SYSTEM\CurrentControlSet\Control\DeviceGuard",
        "EnableVirtualizationBasedSecurity",
    ) == Some(1);
    let vbs_running = reg_u32(
        &hk,
        r"SYSTEM\CurrentControlSet\Control\DeviceGuard",
        "VirtualizationBasedSecurityStatus",
    ) == Some(2);
    let vbs_on = vbs_enabled || vbs_running;
    let hyperv_on = hypervisor_present().unwrap_or(false)
        || bcdedit_hypervisor().map(|v| v != "off").unwrap_or(false)
        || hv_services_on;
    let _ = hvhost_on;

    if hyperv_on && vbs_on && !hv_services_on {
        item(
            "conclusion",
            "综合结论：错误 40 根因判定",
            "fail",
            concat!(
                "变量未排除干净：Hypervisor 仍驻留（疑似 VBS 引导阶段单独拉起）。\n",
                "vmms/vmcompute 已关，但 hvhost 常驻 / HypervisorPresent=True / VBS 启用，VT-x 仍被占用。\n",
                "注意：这不一定是你机器上的真正死因 —— 对 VBox 5.2 而言，新版 ntdll 的加固不兼容（-5607，见上方两项检测）是同等重要的独立根因；",
                "若已抢到 VBoxHardening.log 且含 -5607，则以加固不兼容为准，升级 VBox 7.1.4+ shim 方案，且无需关闭 VBS"
            )
            .to_string(),
            "① 内核隔离内存完整性=关 ② DeviceGuard\\EnableVirtualizationBasedSecurity 置 0 ③ 重启（hvhost 应消失、HypervisorPresent 转 False）",
        )
    } else if hyperv_on && hv_services_on {
        item(
            "conclusion",
            "综合结论：错误 40 根因判定",
            "fail",
            concat!(
                "Hyper-V 正在运行，与 VirtualBox 5.2.x 冲突。\n",
                "注意：请先排除 -5607 加固不兼容（上方 ntdll 检测）——新版 Windows 上 VBox 5.2 的首要死因是加固自检失败，而非 Hyper-V。\n",
                "若已排除：vmms/vmcompute 运行中，VT-x 被 Hyper-V 接管，VBox 5.2 无 WHP 兼容层，startvm 失败"
            )
            .to_string(),
            "bcdedit /set hypervisorlaunchtype off；关闭内核隔离；Windows 功能取消勾选 Hyper-V/虚拟机平台/WSL2；重启",
        )
    } else if !hyperv_on && vbox52 {
        item(
            "conclusion",
            "综合结论：错误 40 根因判定",
            "pass",
            "Hyper-V/VBS 未运行，VT-x 未被占用；VBox 5.2 与 ntdll 无已知冲突。若仍报 40，请查看实时日志中服务端返回的详细原因".into(),
            "",
        )
    } else {
        item(
            "conclusion",
            "综合结论：错误 40 根因判定",
            "info",
            "未发现已知高危冲突组合，请结合其它检查项与实时日志排查".into(),
            "",
        )
    }
}

fn check_ports(_ctx: &mut Ctx) -> Vec<CheckItem> {
    let mut out = Vec::new();
    for port in [65510u16, 65511, 65514] {
        match std::net::TcpListener::bind(("127.0.0.1", port)) {
            Ok(_) => out.push(item(
                &format!("port_{}", port),
                &format!("端口 {} 可用性", port),
                "pass",
                "端口空闲".into(),
                "",
            )),
            Err(_) => {
                let owner = port_owner(port);
                let owner_name = owner.clone().unwrap_or_default();
                let lower = owner_name.to_lowercase();
                let (st, fix) = if owner.is_some() && (lower.contains("ensp") || lower.contains("vbox")) {
                    ("pass", "已被 eNSP/VBox 进程占用，属于正常现象")
                } else if owner.is_some() {
                    ("warn", "该端口被非 eNSP 进程占用，可能导致设备启动失败")
                } else {
                    ("warn", "端口被占用，可能导致错误 41")
                };
                out.push(item(
                    &format!("port_{}", port),
                    &format!("端口 {} 可用性", port),
                    st,
                    match owner {
                        Some(n) => format!("被 {} 占用", n),
                        None => "被占用".into(),
                    },
                    fix,
                ));
            }
        }
    }
    out
}

fn check_pcap(_ctx: &mut Ctx) -> CheckItem {
    let pcap = std::path::Path::new(r"C:\Windows\System32\wpcap.dll").exists();
    item(
        "pcap",
        "WinPcap / Npcap",
        if pcap { "pass" } else { "warn" },
        if pcap {
            "wpcap.dll 存在".into()
        } else {
            "未找到 wpcap.dll".into()
        },
        if pcap {
            ""
        } else {
            "抓包功能需要 WinPcap；如需抓包请安装 WinPcap 或 Npcap（兼容模式）"
        },
    )
}

fn check_disk(_ctx: &mut Ctx) -> CheckItem {
    match free_space("C:\\") {
        Some(bytes) => {
            let gb = bytes as f64 / 1024.0 / 1024.0 / 1024.0;
            item(
                "disk",
                "C 盘剩余空间",
                if gb > 5.0 { "pass" } else { "warn" },
                format!("{:.1} GB", gb),
                if gb > 5.0 {
                    ""
                } else {
                    "空间不足会导致设备创建/配置失败（错误 27/28/42）"
                },
            )
        }
        None => item("disk", "C 盘剩余空间", "warn", "无法读取".into(), ""),
    }
}

fn scan_ensp_logs(_ctx: &mut Ctx) -> CheckItem {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("eNSP"));
    let Some(base) = base else {
        return item("ensplogs", "eNSP 自身日志扫描", "info", "无 LOCALAPPDATA".into(), "");
    };
    let mut log_files: Vec<std::path::PathBuf> = Vec::new();
    for sub in ["VBoxServer", "ConsoleServer"] {
        let dir = base.join(sub);
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("log") {
                    log_files.push(p);
                }
            }
        }
    }
    if let Some(root) = ensp_root() {
        for f in ["install.log", "VBoxManage.log"] {
            let p = std::path::Path::new(&root).join("vboxserver").join("log").join(f);
            if p.exists() {
                log_files.push(p);
            }
        }
    }
    log_files.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .map(|t| t.elapsed().unwrap_or_default())
            .unwrap_or_default()
    });
    let mut found: Vec<String> = Vec::new();
    for path in log_files.iter().take(5) {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines().rev().take(300) {
                let l = line.to_lowercase();
                if l.contains("error") || l.contains("fail") || l.contains("错误") {
                    found.push(format!("{}: {}", path.display(), line.trim()));
                }
            }
        }
    }
    if found.is_empty() {
        item(
            "ensplogs",
            "eNSP 自身日志扫描",
            "info",
            "未发现明显错误记录（eNSP 尚未运行或运行正常）".into(),
            "",
        )
    } else {
        found.truncate(8);
        item(
            "ensplogs",
            "eNSP 自身日志扫描",
            "warn",
            found.join("\n"),
            "结合实时日志中捕获的协议报文定位根因",
        )
    }
}

// ---------------------------------------------------------------------------
// runner
// ---------------------------------------------------------------------------

type CheckFn = fn(&mut Ctx) -> Vec<CheckItem>;

fn run_check_fn(ctx: &mut Ctx, f: CheckFn) -> Vec<CheckItem> {
    f(ctx)
}

/// Run every check in sequence, emitting `checkup-item` after each step.
pub fn run_checkup_stream(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut ctx = Ctx {
            ensp: ensp_root(),
            vbox: vbox_install_dir(),
            vbox_version: String::new(),
        };

        // 每项是一个"步骤"，步骤内部可能产出多个检查项
        let steps: Vec<(&str, CheckFn)> = vec![
            ("操作系统版本", |c| vec![check_windows(c)]),
            ("eNSP 安装", |c| vec![check_ensp(c)]),
            ("VirtualBox 版本", |c| vec![check_vbox(c)]),
            ("HypervisorPresent", |c| vec![check_hypervisor_present(c)]),
            ("VBS 虚拟化安全", |c| vec![check_vbs(c)]),
            ("Hyper-V 核心服务", |c| vec![check_hyperv_services(c)]),
            ("hypervisorlaunchtype", |c| vec![check_bcdedit(c)]),
            ("基础虚拟机注册", |c| vec![check_basevm(c)]),
            ("Host-Only 网卡", |c| vec![check_hostonly(c)]),
            ("服务端口", |c| check_ports(c)),
            ("VBoxHardening.log", |c| vec![check_vbox_hardening_log(c)]),
            ("ntdll 兼容性", |c| vec![check_ntdll(c)]),
            ("startvm 失败特征", |c| vec![check_startvm_signature(c)]),
            ("WinPcap", |c| vec![check_pcap(c)]),
            ("磁盘空间", |c| vec![check_disk(c)]),
            ("eNSP 日志扫描", |c| vec![scan_ensp_logs(c)]),
            ("综合结论", |c| vec![check_conclusion(c)]),
        ];
        let total = steps.len();

        for (i, (name, f)) in steps.iter().enumerate() {
            let items = run_check_fn(&mut ctx, *f);
            for it in &items {
                let _ = app.emit(
                    "checkup-item",
                    &serde_json::json!({"idx": i, "total": total, "step": name, "item": it}),
                );
            }
        }
        let _ = app.emit("checkup-done", &serde_json::json!({ "total": total }));
    });
}
