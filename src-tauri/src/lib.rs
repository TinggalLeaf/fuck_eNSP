// fuck_ensp — eNSP 万能错误排查工具
// 通过 Hook eNSP 客户端与服务端进程，把内部协议与日志全部吐出来，
// 用来定位 40/41 之类的启动错误码背后的真实原因。

mod checkup;
mod inject;
mod kb;
mod native;
mod process;
mod shim;
mod tail;

use inject::InjectState;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct LogFile {
    pub path: String,
    pub size: u64,
    pub modified: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoverInfo {
    pub hook_dir: String,
    pub ensp_processes: Vec<process::ProcInfo>,
    pub hooked_pids: Vec<u32>,
    pub watching: bool,
}

#[tauri::command]
fn discover(state: tauri::State<Arc<Mutex<InjectState>>>) -> DiscoverInfo {
    let st = state.lock().unwrap();
    let hooked: Vec<u32> = st.hooked.lock().map(|s| s.iter().copied().collect()).unwrap_or_default();
    DiscoverInfo {
        hook_dir: native::hook_dir().display().to_string(),
        ensp_processes: process::find_ensp_processes(),
        hooked_pids: hooked,
        watching: st.watching.load(std::sync::atomic::Ordering::SeqCst),
    }
}

#[tauri::command]
fn attach(state: tauri::State<Arc<Mutex<InjectState>>>) -> Vec<inject::InjectResult> {
    let st = state.lock().unwrap();
    st.watching.store(true, std::sync::atomic::Ordering::SeqCst);
    let results = inject::attach_all(&st);
    if results.is_empty() {
        return vec![inject::InjectResult {
            pid: 0,
            name: String::new(),
            ok: false,
            message: "未发现运行中的 eNSP 进程，请先用“启动 eNSP”或手动打开 eNSP".into(),
        }];
    }
    results
}

#[tauri::command]
fn launch(state: tauri::State<Arc<Mutex<InjectState>>>) -> Result<inject::InjectResult, String> {
    let st = state.lock().unwrap();
    st.watching.store(true, std::sync::atomic::Ordering::SeqCst);
    inject::launch_ensp(&st)
}

#[tauri::command]
fn set_watching(state: tauri::State<Arc<Mutex<InjectState>>>, on: bool) {
    let st = state.lock().unwrap();
    st.watching.store(on, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
fn stream_start(app: tauri::AppHandle) {
    tail::start_stream(app);
}

#[tauri::command]
fn stream_stop() {
    tail::stop_stream();
}

#[tauri::command]
fn hook_logs_read() -> Vec<tail::HookEntry> {
    tail::read_all(2000)
}

#[tauri::command]
fn hook_logs_clear() {
    tail::clear_logs();
}

#[tauri::command]
fn run_checkup(app: tauri::AppHandle) {
    checkup::run_checkup_stream(app);
}

#[tauri::command]
fn error_kb() -> Vec<kb::KbEntry> {
    kb::knowledge_base()
}

#[tauri::command]
fn shim_status() -> shim::ShimStatus {
    shim::shim_status()
}

#[tauri::command]
fn shim_install(app: tauri::AppHandle) -> Result<(), String> {
    shim::shim_install(app)
}

#[tauri::command]
fn shim_uninstall(app: tauri::AppHandle) -> Result<(), String> {
    shim::shim_uninstall(app)
}

#[tauri::command]
fn shim_register_vms(app: tauri::AppHandle) -> Result<(), String> {
    shim::shim_register_vms(app)
}

#[tauri::command]
fn shim_preflight() -> shim::ShimPreflight {
    shim::shim_preflight()
}

#[tauri::command]
fn shim_kill_conflicts() -> Vec<shim::ConflictProc> {
    shim::shim_kill_conflicts()
}

#[tauri::command]
fn ensp_logs() -> Vec<LogFile> {
    let mut out = Vec::new();
    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("eNSP"));
    if let Some(base) = base {
        collect_logs(&base, &mut out);
    }
    for root in [
        r"C:\Program Files\Huawei\eNSP\vboxserver\log",
        r"C:\Program Files (x86)\Huawei\eNSP\vboxserver\log",
    ] {
        collect_logs(std::path::Path::new(root), &mut out);
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

fn collect_logs(dir: &std::path::Path, out: &mut Vec<LogFile>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) != Some("log") {
                continue;
            }
            if let Ok(m) = std::fs::metadata(&p) {
                let modified = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis().to_string())
                    .unwrap_or_default();
                out.push(LogFile {
                    path: p.display().to_string(),
                    size: m.len(),
                    modified,
                });
            }
        }
    }
}

#[tauri::command]
fn ensp_log_read(path: String, tail_lines: Option<usize>) -> Result<String, String> {
    // basic validation: must be a .log under a known-ish location
    let lower = path.to_lowercase();
    if !lower.ends_with(".log") || !(lower.contains("ensp") || lower.contains("virtualbox")) {
        return Err("not an eNSP log file".into());
    }
    let content = std::fs::read(&path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&content).into_owned();
    let lines: Vec<&str> = text.lines().collect();
    let n = tail_lines.unwrap_or(500).min(5000);
    let start = lines.len().saturating_sub(n);
    Ok(lines[start..].join("\n"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = Arc::new(Mutex::new(InjectState::new()));
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            discover,
            attach,
            launch,
            set_watching,
            stream_start,
            stream_stop,
            hook_logs_read,
            hook_logs_clear,
            run_checkup,
            error_kb,
            shim_status,
            shim_install,
            shim_uninstall,
            shim_register_vms,
            shim_preflight,
            shim_kill_conflicts,
            ensp_logs,
            ensp_log_read,
        ])
        .setup(move |app| {
            if let Some(win) = app.get_webview_window("main") {
                if let Ok(icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png")) {
                    let _ = win.set_icon(icon);
                }
            }
            inject::start_watcher(app.handle().clone(), state);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
