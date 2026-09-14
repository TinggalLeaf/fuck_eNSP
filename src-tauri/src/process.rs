//! Process enumeration and eNSP process discovery.

use serde::Serialize;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};

#[derive(Debug, Clone, Serialize)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
}

pub fn list_processes() -> Vec<ProcInfo> {
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len())],
                );
                out.push(ProcInfo {
                    pid: entry.th32ProcessID,
                    name,
                });
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    out
}

/// Host-side eNSP processes worth hooking (exact names — prevents the
/// watcher from chasing short-lived helper exes).
pub fn is_ensp_process(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "ensp_client.exe"
            | "ensp_vboxserver.exe"
            | "ensp_consoleserver.exe"
            | "ensp_consvr.exe"
            | "ensp_wvrpsvr.exe"
    )
}

pub fn find_ensp_processes() -> Vec<ProcInfo> {
    list_processes()
        .into_iter()
        .filter(|p| is_ensp_process(&p.name))
        .collect()
}
