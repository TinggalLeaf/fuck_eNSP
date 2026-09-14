//! Extracts the embedded 32-bit native components (hook DLL + injector) to a
//! stable location under %LOCALAPPDATA so they can be reused across runs.

use std::path::PathBuf;

static HOOK_DLL: &[u8] = include_bytes!(concat!(env!("NATIVE_STAGE_DIR"), "/fuck_ensp_hook.dll"));
static INJECTOR_EXE: &[u8] =
    include_bytes!(concat!(env!("NATIVE_STAGE_DIR"), "/fuck_inject32.exe"));

fn app_data() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir());
    base.join("fuck_ensp")
}

pub fn hook_dir() -> PathBuf {
    app_data().join("hooks")
}

pub fn bin_dir() -> PathBuf {
    app_data().join("bin")
}

fn extract(name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let dir = bin_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(name);
    let need = std::fs::metadata(&path)
        .map(|m| m.len() != bytes.len() as u64)
        .unwrap_or(true);
    if need {
        std::fs::write(&path, bytes)?;
    }
    Ok(path)
}

pub fn hook_dll_path() -> std::io::Result<PathBuf> {
    extract("fuck_ensp_hook.dll", HOOK_DLL)
}

pub fn injector_path() -> std::io::Result<PathBuf> {
    extract("fuck_inject32.exe", INJECTOR_EXE)
}
