//! fuck_ensp_hook.dll — injected into eNSP processes to dump all internal
//! communication (client <-> vboxserver protocol, process creation) so that
//! eNSP errors like "40/41" can be diagnosed from the real root cause.

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use minhook::MinHook;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::io::Write;
use std::sync::Mutex;
use windows_sys::core::{BOOL, PWSTR};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Networking::WinSock::{
    LPWSAOVERLAPPED_COMPLETION_ROUTINE, SOCKADDR, SOCKADDR_IN, WSABUF,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::IO::OVERLAPPED;
use windows_sys::Win32::System::LibraryLoader::{
    DisableThreadLibraryCalls, GetModuleFileNameW, GetModuleHandleW, GetProcAddress,
};
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows_sys::Win32::System::Threading::{
    CreateThread, GetCurrentProcessId, PROCESS_INFORMATION, STARTUPINFOW,
};

type CHAR = u8;
type WCHAR = u16;

// ---------------------------------------------------------------------------
// config / state
// ---------------------------------------------------------------------------

static LOG_FILE: OnceCell<Mutex<std::fs::File>> = OnceCell::new();
static PROC_NAME: OnceCell<String> = OnceCell::new();
static PID: OnceCell<u32> = OnceCell::new();

/// (socket, overlapped ptr) -> (buffer ptr, buffer len) for pending IOCP recvs
static PENDING: Mutex<Option<HashMap<(usize, usize), (usize, usize)>>> = Mutex::new(None);

// True while inside one of our detours (reentrancy guard).
thread_local! {
    static IN_HOOK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

const MAX_PENDING: usize = 16 * 1024;

fn in_hook() -> bool {
    IN_HOOK.with(|f| f.get())
}

fn guard<R, F: FnOnce() -> R>(f: F) -> Option<R> {
    if in_hook() {
        return None;
    }
    IN_HOOK.with(|c| c.set(true));
    // Never let a panic escape into the host process.
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    IN_HOOK.with(|c| c.set(false));
    r.ok()
}

// ---------------------------------------------------------------------------
// logging
// ---------------------------------------------------------------------------

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn now_ms() -> i64 {
    unsafe {
        let mut ft: i64 = 0;
        GetSystemTimeAsFileTime(&mut ft as *mut i64 as *mut _);
        (ft / 10_000) - 11644473600000
    }
}

fn peer_of(sock: usize) -> String {
    unsafe {
        let mut sa: SOCKADDR_IN = std::mem::zeroed();
        let mut len = std::mem::size_of::<SOCKADDR_IN>() as i32;
        let rc = windows_sys::Win32::Networking::WinSock::getpeername(
            sock,
            &mut sa as *mut SOCKADDR_IN as *mut SOCKADDR,
            &mut len,
        );
        if rc != 0 || sa.sin_family != 2 {
            return String::new();
        }
        let ip = u32::from_be(sa.sin_addr.S_un.S_addr);
        let port = u16::from_be(sa.sin_port);
        format!(
            "{}.{}.{}.{}:{}",
            ip & 0xff,
            (ip >> 8) & 0xff,
            (ip >> 16) & 0xff,
            (ip >> 24) & 0xff,
            port
        )
    }
}

fn fmt_payload(buf: *const u8, len: usize) -> (String, &'static str) {
    if buf.is_null() || len == 0 {
        return (String::new(), "empty");
    }
    let bytes = unsafe { std::slice::from_raw_parts(buf, len.min(64 * 1024)) };
    let printable = bytes
        .iter()
        .filter(|b| matches!(**b, 0x09 | 0x0a | 0x0d | 0x20..=0x7e) || **b >= 0x80)
        .count();
    if printable * 10 >= bytes.len() * 7 {
        // keep raw bytes; backend tries utf8 then gbk
        let s = String::from_utf8_lossy(bytes).into_owned();
        (json_escape(&s), "text")
    } else {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{:02x}", b));
        }
        (s, "hex")
    }
}

fn emit(record: &str) {
    let mut guard = match LOG_FILE.get() {
        Some(m) => m.lock(),
        None => return,
    };
    if let Ok(f) = guard.as_mut() {
        let _ = f.write_all(record.as_bytes());
        let _ = f.write_all(b"\n");
        let _ = f.flush();
    }
}

fn log_net(api: &str, sock: usize, dir: &str, buf: *const u8, len: usize) {
    {
        let peer = peer_of(sock);
        let (data, enc) = fmt_payload(buf, len);
        let rec = format!(
            "{{\"ts\":{},\"pid\":{},\"proc\":\"{}\",\"cat\":\"net\",\"api\":\"{}\",\"sock\":{},\"peer\":\"{}\",\"dir\":\"{}\",\"len\":{},\"enc\":\"{}\",\"data\":\"{}\"}}",
            now_ms(),
            PID.get().copied().unwrap_or(0),
            PROC_NAME.get().map(String::as_str).unwrap_or(""),
            api,
            sock,
            peer,
            dir,
            len,
            enc,
            data
        );
        emit(&rec);
    }
}

fn log_connect(api: &str, sock: usize, name: *const SOCKADDR) {
    {
        let peer = unsafe {
            let sa = &*(name as *const SOCKADDR_IN);
            if sa.sin_family == 2 {
                let ip = u32::from_be(sa.sin_addr.S_un.S_addr);
                let port = u16::from_be(sa.sin_port);
                format!(
                    "{}.{}.{}.{}:{}",
                    ip & 0xff,
                    (ip >> 8) & 0xff,
                    (ip >> 16) & 0xff,
                    (ip >> 24) & 0xff,
                    port
                )
            } else {
                String::new()
            }
        };
        let rec = format!(
            "{{\"ts\":{},\"pid\":{},\"proc\":\"{}\",\"cat\":\"net\",\"api\":\"{}\",\"sock\":{},\"dir\":\"out\",\"peer\":\"{}\",\"len\":0,\"enc\":\"empty\",\"data\":\"\"}}",
            now_ms(),
            PID.get().copied().unwrap_or(0),
            PROC_NAME.get().map(String::as_str).unwrap_or(""),
            api,
            sock,
            peer
        );
        emit(&rec);
    }
}

fn wide_to_string(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe {
        let mut len = 0usize;
        while *p.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(p, len);
        String::from_utf16_lossy(slice)
    }
}

fn log_proc(api: &str, app: *const u16, cmd: *const u16, ok: BOOL, child: u32) {
    {
        let app_s = json_escape(&wide_to_string(app));
        let cmd_s = json_escape(&wide_to_string(cmd));
        let rec = format!(
            "{{\"ts\":{},\"pid\":{},\"proc\":\"{}\",\"cat\":\"proc\",\"api\":\"{}\",\"ok\":{},\"child_pid\":{},\"app\":\"{}\",\"cmd\":\"{}\"}}",
            now_ms(),
            PID.get().copied().unwrap_or(0),
            PROC_NAME.get().map(String::as_str).unwrap_or(""),
            api,
            if ok != 0 { "true" } else { "false" },
            child,
            app_s,
            cmd_s
        );
        emit(&rec);
    }
}

// ---------------------------------------------------------------------------
// trampolines
// ---------------------------------------------------------------------------

type FnWSASend = unsafe extern "system" fn(
    usize,
    *const WSABUF,
    u32,
    *mut u32,
    u32,
    *const OVERLAPPED,
    LPWSAOVERLAPPED_COMPLETION_ROUTINE,
) -> i32;
type FnWSARecv = unsafe extern "system" fn(
    usize,
    *const WSABUF,
    u32,
    *mut u32,
    *mut u32,
    *const OVERLAPPED,
    LPWSAOVERLAPPED_COMPLETION_ROUTINE,
) -> i32;
type FnWSAGetOv =
    unsafe extern "system" fn(usize, *const OVERLAPPED, *mut u32, i32, *mut u32) -> i32;
type Fnsend = unsafe extern "system" fn(usize, *const u8, i32, i32) -> i32;
type Fnrecv = unsafe extern "system" fn(usize, *mut u8, i32, i32) -> i32;
type Fnconnect = unsafe extern "system" fn(usize, *const SOCKADDR, i32) -> i32;
type FnCreateProcessW = unsafe extern "system" fn(
    *const u16,
    PWSTR,
    *const SECURITY_ATTRIBUTES,
    *const SECURITY_ATTRIBUTES,
    BOOL,
    u32,
    *const c_void,
    *const u16,
    *const STARTUPINFOW,
    *mut PROCESS_INFORMATION,
) -> BOOL;
type FnCreateProcessA = unsafe extern "system" fn(
    *const CHAR,
    *mut CHAR,
    *const SECURITY_ATTRIBUTES,
    *const SECURITY_ATTRIBUTES,
    BOOL,
    u32,
    *const c_void,
    *const CHAR,
    *const STARTUPINFOW,
    *mut PROCESS_INFORMATION,
) -> BOOL;
type FnGQCS = unsafe extern "system" fn(
    HANDLE,
    *mut u32,
    *mut usize,
    *mut *mut OVERLAPPED,
    u32,
) -> BOOL;
type FnGQCSEx = unsafe extern "system" fn(
    HANDLE,
    *mut OVERLAPPED_ENTRY,
    u32,
    *mut u32,
    u32,
    *mut u32,
) -> BOOL;

#[repr(C)]
struct OVERLAPPED_ENTRY {
    lpCompletionKey: usize,
    lpOverlapped: *mut OVERLAPPED,
    Internal: usize,
    dwNumberOfBytesTransferred: u32,
}

static O_WSASEND: OnceCell<usize> = OnceCell::new();
static O_WSARECV: OnceCell<usize> = OnceCell::new();
static O_WSAGETOV: OnceCell<usize> = OnceCell::new();
static O_SEND: OnceCell<usize> = OnceCell::new();
static O_RECV: OnceCell<usize> = OnceCell::new();
static O_CONNECT: OnceCell<usize> = OnceCell::new();
static O_CPW: OnceCell<usize> = OnceCell::new();
static O_CPA: OnceCell<usize> = OnceCell::new();
static O_GQCS: OnceCell<usize> = OnceCell::new();
static O_GQCSEX: OnceCell<usize> = OnceCell::new();

unsafe fn o_wsasend() -> FnWSASend {
    std::mem::transmute(*O_WSASEND.get().unwrap_or(&0))
}
unsafe fn o_wsarecv() -> FnWSARecv {
    std::mem::transmute(*O_WSARECV.get().unwrap_or(&0))
}
unsafe fn o_wsagetov() -> FnWSAGetOv {
    std::mem::transmute(*O_WSAGETOV.get().unwrap_or(&0))
}
unsafe fn o_send() -> Fnsend {
    std::mem::transmute(*O_SEND.get().unwrap_or(&0))
}
unsafe fn o_recv() -> Fnrecv {
    std::mem::transmute(*O_RECV.get().unwrap_or(&0))
}
unsafe fn o_connect() -> Fnconnect {
    std::mem::transmute(*O_CONNECT.get().unwrap_or(&0))
}
unsafe fn o_cpw() -> FnCreateProcessW {
    std::mem::transmute(*O_CPW.get().unwrap_or(&0))
}
unsafe fn o_cpa() -> FnCreateProcessA {
    std::mem::transmute(*O_CPA.get().unwrap_or(&0))
}
unsafe fn o_gqcs() -> FnGQCS {
    std::mem::transmute(*O_GQCS.get().unwrap_or(&0))
}
unsafe fn o_gqcsex() -> FnGQCSEx {
    std::mem::transmute(*O_GQCSEX.get().unwrap_or(&0))
}

// ---------------------------------------------------------------------------
// detours
// ---------------------------------------------------------------------------

unsafe extern "system" fn d_WSASend(
    s: usize,
    bufs: *const WSABUF,
    count: u32,
    sent: *mut u32,
    flags: u32,
    ov: *const OVERLAPPED,
    cr: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
) -> i32 {
    if in_hook() {
        return o_wsasend()(s, bufs, count, sent, flags, ov, cr);
    }
    let rc = o_wsasend()(s, bufs, count, sent, flags, ov, cr);
    let _ = guard(|| {
        if !bufs.is_null() && count > 0 {
            let b = &*bufs;
            log_net("WSASend", s, "out", b.buf as *const u8, b.len as usize);
        }
    });
    rc
}

unsafe extern "system" fn d_WSARecv(
    s: usize,
    bufs: *const WSABUF,
    count: u32,
    got: *mut u32,
    flags: *mut u32,
    ov: *const OVERLAPPED,
    cr: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
) -> i32 {
    if in_hook() {
        return o_wsarecv()(s, bufs, count, got, flags, ov, cr);
    }
    let rc = o_wsarecv()(s, bufs, count, got, flags, ov, cr);
    let overlapped = !ov.is_null();
    let _ = guard(|| {
        if bufs.is_null() || count == 0 {
            return;
        }
        let b = &*bufs;
        if !overlapped {
            if rc == 0 && !got.is_null() {
                log_net("WSARecv", s, "in", b.buf as *const u8, *got as usize);
            }
        } else {
            let key = (s, ov as usize);
            let mut m = PENDING.lock().unwrap();
            let map = m.get_or_insert_with(HashMap::new);
            if map.len() > MAX_PENDING {
                map.clear();
            }
            if rc == 0 {
                // completed synchronously
                let n = if got.is_null() { b.len } else { *got };
                drop(m);
                log_net("WSARecv", s, "in", b.buf as *const u8, n as usize);
            } else {
                map.insert(key, (b.buf as usize, b.len as usize));
            }
        }
    });
    rc
}

unsafe extern "system" fn d_WSAGetOverlappedResult(
    s: usize,
    ov: *const OVERLAPPED,
    transferred: *mut u32,
    wait: i32,
    flags: *mut u32,
) -> i32 {
    if in_hook() {
        return o_wsagetov()(s, ov, transferred, wait, flags);
    }
    let rc = o_wsagetov()(s, ov, transferred, wait, flags);
    let _ = guard(|| {
        if rc != 0 && !ov.is_null() && !transferred.is_null() {
            let key = (s, ov as usize);
            let pending = {
                let mut m = PENDING.lock().unwrap();
                m.as_mut().and_then(|map| map.remove(&key))
            };
            if let Some((buf, len)) = pending {
                log_net(
                    "WSARecv",
                    s,
                    "in",
                    buf as *const u8,
                    (*transferred as usize).min(len),
                );
            }
        }
    });
    rc
}

unsafe extern "system" fn d_send(s: usize, buf: *const u8, len: i32, flags: i32) -> i32 {
    if in_hook() {
        return o_send()(s, buf, len, flags);
    }
    let rc = o_send()(s, buf, len, flags);
    let _ = guard(|| {
        if rc > 0 {
            log_net("send", s, "out", buf, rc as usize);
        }
    });
    rc
}

unsafe extern "system" fn d_recv(s: usize, buf: *mut u8, len: i32, flags: i32) -> i32 {
    if in_hook() {
        return o_recv()(s, buf, len, flags);
    }
    let rc = o_recv()(s, buf, len, flags);
    let _ = guard(|| {
        if rc > 0 {
            log_net("recv", s, "in", buf, rc as usize);
        }
    });
    rc
}

unsafe extern "system" fn d_connect(s: usize, name: *const SOCKADDR, namelen: i32) -> i32 {
    if in_hook() {
        return o_connect()(s, name, namelen);
    }
    let rc = o_connect()(s, name, namelen);
    let _ = guard(|| {
        if rc == 0 {
            log_connect("connect", s, name);
        }
    });
    rc
}

unsafe extern "system" fn d_CreateProcessW(
    app: *const WCHAR,
    cmd: PWSTR,
    pa: *const SECURITY_ATTRIBUTES,
    ta: *const SECURITY_ATTRIBUTES,
    inherit: BOOL,
    flags: u32,
    env: *const c_void,
    cwd: *const WCHAR,
    si: *const STARTUPINFOW,
    pi: *mut PROCESS_INFORMATION,
) -> BOOL {
    if in_hook() {
        return o_cpw()(app, cmd, pa, ta, inherit, flags, env, cwd, si, pi);
    }
    let rc = o_cpw()(app, cmd, pa, ta, inherit, flags, env, cwd, si, pi);
    let _ = guard(|| {
        let child = if rc != 0 && !pi.is_null() {
            (*pi).dwProcessId
        } else {
            0
        };
        log_proc("CreateProcessW", app as *const u16, cmd as *const u16, rc, child);
    });
    rc
}

unsafe extern "system" fn d_CreateProcessA(
    app: *const CHAR,
    cmd: *mut CHAR,
    pa: *const SECURITY_ATTRIBUTES,
    ta: *const SECURITY_ATTRIBUTES,
    inherit: BOOL,
    flags: u32,
    env: *const c_void,
    cwd: *const CHAR,
    si: *const STARTUPINFOW,
    pi: *mut PROCESS_INFORMATION,
) -> BOOL {
    if in_hook() {
        return o_cpa()(app, cmd, pa, ta, inherit, flags, env, cwd, si, pi);
    }
    let rc = o_cpa()(app, cmd, pa, ta, inherit, flags, env, cwd, si, pi);
    let _ = guard(|| {
        let narrow = |p: *const CHAR| -> String {
            if p.is_null() {
                return String::new();
            }
            let mut len = 0usize;
            while *p.add(len) != 0 {
                len += 1;
            }
            let b = std::slice::from_raw_parts(p, len);
            String::from_utf8_lossy(b).into_owned()
        };
        let app_s = json_escape(&narrow(app));
        let cmd_s = json_escape(&narrow(cmd as *const CHAR));
        let child = if rc != 0 && !pi.is_null() {
            (*pi).dwProcessId
        } else {
            0
        };
        let rec = format!(
            "{{\"ts\":{},\"pid\":{},\"proc\":\"{}\",\"cat\":\"proc\",\"api\":\"CreateProcessA\",\"ok\":{},\"child_pid\":{},\"app\":\"{}\",\"cmd\":\"{}\"}}",
            now_ms(),
            PID.get().copied().unwrap_or(0),
            PROC_NAME.get().map(String::as_str).unwrap_or(""),
            if rc != 0 { "true" } else { "false" },
            child,
            app_s,
            cmd_s
        );
        emit(&rec);
    });
    rc
}

unsafe extern "system" fn d_GetQueuedCompletionStatus(
    port: HANDLE,
    transferred: *mut u32,
    key: *mut usize,
    ov: *mut *mut OVERLAPPED,
    timeout: u32,
) -> BOOL {
    if in_hook() {
        return o_gqcs()(port, transferred, key, ov, timeout);
    }
    let rc = o_gqcs()(port, transferred, key, ov, timeout);
    let _ = guard(|| {
        if rc != 0 && !ov.is_null() && !(*ov).is_null() && !transferred.is_null() {
            let ovp = *ov as usize;
            let pending = {
                let mut m = PENDING.lock().unwrap();
                match m.as_mut() {
                    Some(map) => map
                        .iter()
                        .find(|((_, o), _)| *o == ovp)
                        .map(|(k, v)| (*k, *v)),
                    None => None,
                }
            };
            if let Some((k, (buf, len))) = pending {
                PENDING
                    .lock()
                    .unwrap()
                    .as_mut()
                    .map(|map| map.remove(&k));
                log_net(
                    "WSARecv",
                    k.0,
                    "in",
                    buf as *const u8,
                    (*transferred as usize).min(len),
                );
            }
        }
    });
    rc
}

unsafe extern "system" fn d_GetQueuedCompletionStatusEx(
    port: HANDLE,
    entries: *mut OVERLAPPED_ENTRY,
    count: u32,
    removed: *mut u32,
    timeout: u32,
    alertable: *mut u32,
) -> BOOL {
    if in_hook() {
        return o_gqcsex()(port, entries, count, removed, timeout, alertable);
    }
    let rc = o_gqcsex()(port, entries, count, removed, timeout, alertable);
    let _ = guard(|| {
        if rc == 0 || removed.is_null() || entries.is_null() {
            return;
        }
        for i in 0..(*removed as isize) {
            let e = &*entries.offset(i);
            let ovp = e.lpOverlapped as usize;
            if ovp == 0 {
                continue;
            }
            let pending = {
                let mut m = PENDING.lock().unwrap();
                match m.as_mut() {
                    Some(map) => map
                        .iter()
                        .find(|((_, o), _)| *o == ovp)
                        .map(|(k, v)| (*k, *v)),
                    None => None,
                }
            };
            if let Some((k, (buf, len))) = pending {
                PENDING
                    .lock()
                    .unwrap()
                    .as_mut()
                    .map(|map| map.remove(&k));
                log_net(
                    "WSARecv",
                    k.0,
                    "in",
                    buf as *const u8,
                    (e.dwNumberOfBytesTransferred as usize).min(len),
                );
            }
        }
    });
    rc
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

fn name_as_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn init_log_file() {
    let dir = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("C:\\"))
        .join("fuck_ensp")
        .join("hooks");
    let _ = std::fs::create_dir_all(&dir);
    let pid = unsafe { GetCurrentProcessId() };
    PID.set(pid).ok();

    let mut exe = [0u16; 520];
    let n = unsafe { GetModuleFileNameW(std::ptr::null_mut(), exe.as_mut_ptr(), 520) };
    let full = String::from_utf16_lossy(&exe[..n as usize]);
    let base = full
        .rsplit('\\')
        .next()
        .unwrap_or("unknown")
        .to_string();
    PROC_NAME.set(base.clone()).ok();

    let path = dir.join(format!(
        "{}_{}.jsonl",
        base.trim_end_matches(".exe"),
        pid
    ));
    if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        LOG_FILE.set(Mutex::new(f)).ok();
    }
}

unsafe fn hook_api(module: &str, name: &str, detour: *mut c_void) -> Option<usize> {
    let wide = name_as_wide(module);
    let h = GetModuleHandleW(wide.as_ptr());
    if h.is_null() {
        diag(&format!("hook {}!{}: module not loaded", module, name));
        return None;
    }
    let addr = GetProcAddress(h, name.as_ptr() as *const u8);
    let addr = addr.map(|f| f as usize).unwrap_or(0);
    if addr == 0 {
        diag(&format!("hook {}!{}: proc not found", module, name));
        return None;
    }
    let created = MinHook::create_hook(addr as *mut c_void, detour);
    match created {
        Ok(tramp) => {
            let en = MinHook::enable_hook(addr as *mut c_void);
            diag(&format!(
                "hook {}!{} @{:x}: created, enable={:?}",
                module,
                name.trim_end_matches('\0'),
                addr,
                en
            ));
            Some(tramp as usize)
        }
        Err(e) => {
            diag(&format!(
                "hook {}!{} @{:x}: create FAILED {:?}",
                module,
                name.trim_end_matches('\0'),
                addr,
                e
            ));
            None
        }
    }
}

fn diag(msg: &str) {
    emit(&format!(
        "{{\"ts\":{},\"pid\":{},\"proc\":\"{}\",\"cat\":\"sys\",\"api\":\"hook_diag\",\"data\":\"{}\"}}",
        now_ms(),
        PID.get().copied().unwrap_or(0),
        PROC_NAME.get().map(String::as_str).unwrap_or(""),
        json_escape(msg)
    ));
}

fn install_hooks() {
    unsafe {
        if let Some(t) = hook_api("ws2_32.dll", "WSASend\0", d_WSASend as *mut c_void) {
            O_WSASEND.set(t).ok();
        }
        if let Some(t) = hook_api("ws2_32.dll", "WSARecv\0", d_WSARecv as *mut c_void) {
            O_WSARECV.set(t).ok();
        }
        if let Some(t) = hook_api(
            "ws2_32.dll",
            "WSAGetOverlappedResult\0",
            d_WSAGetOverlappedResult as *mut c_void,
        ) {
            O_WSAGETOV.set(t).ok();
        }
        if let Some(t) = hook_api("ws2_32.dll", "send\0", d_send as *mut c_void) {
            O_SEND.set(t).ok();
        }
        if let Some(t) = hook_api("ws2_32.dll", "recv\0", d_recv as *mut c_void) {
            O_RECV.set(t).ok();
        }
        if let Some(t) = hook_api("ws2_32.dll", "connect\0", d_connect as *mut c_void) {
            O_CONNECT.set(t).ok();
        }
        if let Some(t) = hook_api(
            "kernel32.dll",
            "CreateProcessW\0",
            d_CreateProcessW as *mut c_void,
        ) {
            O_CPW.set(t).ok();
        }
        if let Some(t) = hook_api(
            "kernel32.dll",
            "CreateProcessA\0",
            d_CreateProcessA as *mut c_void,
        ) {
            O_CPA.set(t).ok();
        }
        if let Some(t) = hook_api(
            "kernel32.dll",
            "GetQueuedCompletionStatus\0",
            d_GetQueuedCompletionStatus as *mut c_void,
        ) {
            O_GQCS.set(t).ok();
        }
        if let Some(t) = hook_api(
            "kernel32.dll",
            "GetQueuedCompletionStatusEx\0",
            d_GetQueuedCompletionStatusEx as *mut c_void,
        ) {
            O_GQCSEX.set(t).ok();
        }
    }
}

unsafe extern "system" fn init_thread(_: *mut c_void) -> u32 {
    std::panic::set_hook(Box::new(|_| {}));
    init_log_file();
    install_hooks();
    emit(&format!(
        "{{\"ts\":{},\"pid\":{},\"proc\":\"{}\",\"cat\":\"sys\",\"api\":\"hook_init\",\"data\":\"hooks installed\"}}",
        now_ms(),
        PID.get().copied().unwrap_or(0),
        PROC_NAME.get().map(String::as_str).unwrap_or("")
    ));
    // signal readiness so the injector resumes the (suspended) host only
    // after hooks are active
    let name: Vec<u16> = format!("Local\\fuck_ensp_hook_ready_{}", PID.get().copied().unwrap_or(0))
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let ev = windows_sys::Win32::System::Threading::CreateEventW(
        std::ptr::null(),
        1,
        0,
        name.as_ptr(),
    );
    if !ev.is_null() {
        windows_sys::Win32::System::Threading::SetEvent(ev);
    }
    0
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllMain(hinst: *mut c_void, reason: u32, _: *mut c_void) -> BOOL {
    if reason == 1 {
        // DLL_PROCESS_ATTACH
        DisableThreadLibraryCalls(hinst);
        let mut tid = 0u32;
        let t = CreateThread(
            std::ptr::null(),
            0,
            Some(init_thread),
            std::ptr::null_mut(),
            0,
            &mut tid,
        );
        if !t.is_null() {
            CloseHandle(t);
        }
    }
    1
}
