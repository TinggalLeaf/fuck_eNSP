//! Injection management: attach to running eNSP processes, launch eNSP with
//! the hook pre-loaded, and watch for new eNSP processes while monitoring.

use crate::native;
use crate::process::{find_ensp_processes, ProcInfo};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// how long to wait before retrying a failed injection
const RETRY_COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
pub struct InjectResult {
    pub pid: u32,
    pub name: String,
    pub ok: bool,
    pub message: String,
}

pub struct InjectState {
    pub hooked: Mutex<HashSet<u32>>,
    /// pid -> last failed attempt; retried only after RETRY_COOLDOWN
    pub failed: Mutex<HashMap<u32, Instant>>,
    pub watching: Arc<AtomicBool>,
    pub watcher_started: bool,
}

impl InjectState {
    pub fn new() -> Self {
        Self {
            hooked: Mutex::new(HashSet::new()),
            failed: Mutex::new(HashMap::new()),
            watching: Arc::new(AtomicBool::new(false)),
            watcher_started: false,
        }
    }
}

fn already_hooked(state: &InjectState, pid: u32) -> bool {
    state.hooked.lock().map(|s| s.contains(&pid)).unwrap_or(false)
}

fn on_cooldown(state: &InjectState, pid: u32) -> bool {
    state
        .failed
        .lock()
        .map(|m| m.get(&pid).map(|t| t.elapsed() < RETRY_COOLDOWN).unwrap_or(false))
        .unwrap_or(false)
}

fn mark_hooked(state: &InjectState, pid: u32) {
    if let Ok(mut s) = state.hooked.lock() {
        s.insert(pid);
    }
    if let Ok(mut m) = state.failed.lock() {
        m.remove(&pid);
    }
}

fn mark_failed(state: &InjectState, pid: u32) {
    if let Ok(mut m) = state.failed.lock() {
        m.insert(pid, Instant::now());
    }
}

fn run_helper(args: &[std::ffi::OsString]) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let injector = native::injector_path().map_err(|e| e.to_string())?;
    let output = std::process::Command::new(&injector)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("spawn injector failed: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() && stdout.starts_with("OK") {
        Ok(stdout)
    } else {
        Err(interpret_helper_error(&format!(
            "{} (code {:?})",
            stdout,
            output.status.code()
        )))
    }
}

/// Turn raw Windows error codes into actionable Chinese hints.
fn interpret_helper_error(msg: &str) -> String {
    if msg.contains("err=5") || msg.contains("err=740") || msg.contains("err=1307") {
        format!(
            "{} —— 权限不足：eNSP 正以管理员身份运行，本工具需要同样以管理员身份运行（重新启动程序并接受 UAC 提权）",
            msg
        )
    } else {
        msg.to_string()
    }
}

/// Inject the hook DLL into one process via the 32-bit helper.
pub fn inject_one(state: &InjectState, proc: &ProcInfo) -> InjectResult {
    if already_hooked(state, proc.pid) {
        return InjectResult {
            pid: proc.pid,
            name: proc.name.clone(),
            ok: true,
            message: "already hooked".into(),
        };
    }
    if on_cooldown(state, proc.pid) {
        return InjectResult {
            pid: proc.pid,
            name: proc.name.clone(),
            ok: false,
            message: "该进程注入刚失败过，冷却中（60 秒后自动重试）".into(),
        };
    }
    let result = (|| -> Result<String, String> {
        let dll = native::hook_dll_path().map_err(|e| e.to_string())?;
        run_helper(&[
            std::ffi::OsString::from("--pid"),
            std::ffi::OsString::from(proc.pid.to_string()),
            dll.into_os_string(),
        ])
    })();
    match result {
        Ok(msg) => {
            mark_hooked(state, proc.pid);
            InjectResult {
                pid: proc.pid,
                name: proc.name.clone(),
                ok: true,
                message: msg,
            }
        }
        Err(e) => {
            mark_failed(state, proc.pid);
            InjectResult {
                pid: proc.pid,
                name: proc.name.clone(),
                ok: false,
                message: e,
            }
        }
    }
}

/// Attach to all currently running eNSP processes.
pub fn attach_all(state: &InjectState) -> Vec<InjectResult> {
    let procs = find_ensp_processes();
    if procs.is_empty() {
        return vec![];
    }
    procs.iter().map(|p| inject_one(state, p)).collect()
}

/// Launch eNSP Client suspended, inject, resume.
pub fn launch_ensp(state: &InjectState) -> Result<InjectResult, String> {
    let client = find_ensp_client().ok_or_else(|| {
        "eNSP_Client.exe not found. Install eNSP or locate it manually.".to_string()
    })?;
    let dll = native::hook_dll_path().map_err(|e| e.to_string())?;
    run_helper(&[
        std::ffi::OsString::from("--launch"),
        std::ffi::OsString::from(&client),
        dll.into_os_string(),
    ])?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    if let Some(p) = find_ensp_processes()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case("eNSP_Client.exe"))
    {
        mark_hooked(state, p.pid);
        return Ok(InjectResult {
            pid: p.pid,
            name: p.name,
            ok: true,
            message: "launched with hook".into(),
        });
    }
    Ok(InjectResult {
        pid: 0,
        name: "eNSP_Client.exe".into(),
        ok: true,
        message: "launched with hook".into(),
    })
}

pub fn find_ensp_client() -> Option<String> {
    for path in [
        "C:\\Program Files\\Huawei\\eNSP\\eNSP_Client.exe",
        "C:\\Program Files (x86)\\Huawei\\eNSP\\eNSP_Client.exe",
    ] {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    None
}

/// Spawn a background thread that injects into every new eNSP process while
/// `watching` is true.
pub fn start_watcher(app: tauri::AppHandle, state: Arc<Mutex<InjectState>>) {
    {
        let mut st = state.lock().unwrap();
        if st.watcher_started {
            st.watching.store(true, Ordering::SeqCst);
            return;
        }
        st.watcher_started = true;
        st.watching.store(true, Ordering::SeqCst);
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(800));
        let watching = state.lock().map(|s| s.watching.load(Ordering::SeqCst)).unwrap_or(false);
        if !watching {
            continue;
        }
        let procs = find_ensp_processes();
        let snapshot = state.lock().unwrap();
        let mut results = Vec::new();
        for p in &procs {
            if !already_hooked(&snapshot, p.pid) && !on_cooldown(&snapshot, p.pid) {
                results.push(inject_one(&snapshot, p));
            }
        }
        drop(snapshot);
        for r in results {
            if r.ok {
                let _ = app.emit("inject-event", &r);
            }
        }
    });
}

use tauri::Emitter;
