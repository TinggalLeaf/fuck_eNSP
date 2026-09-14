//! fuck_inject32.exe — 32-bit injection helper (must match target bitness).
//!
//! Usage:
//!   fuck_inject32.exe --pid <PID> <dll_path>
//!   fuck_inject32.exe --launch <exe_path> <dll_path> [args...]

#![windows_subsystem = "windows"]

use std::ffi::c_void;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, CreateRemoteThread, GetExitCodeThread, ResumeThread, WaitForSingleObject,
    CREATE_SUSPENDED, PROCESS_INFORMATION, STARTUPINFOW,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn inject(pid: u32, dll_path: &str) -> Result<(), String> {
    let dll_wide = wide(dll_path);
    let bytes = dll_wide.len() * 2 + 2;

    let proc = windows_sys::Win32::System::Threading::OpenProcess(
        windows_sys::Win32::System::Threading::PROCESS_ALL_ACCESS,
        0,
        pid,
    );
    if proc.is_null() {
        return Err(format!("OpenProcess failed, err={}", GetLastError()));
    }

    let remote = VirtualAllocEx(
        proc,
        std::ptr::null_mut(),
        bytes,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE,
    );
    if remote.is_null() {
        let e = GetLastError();
        CloseHandle(proc);
        return Err(format!("VirtualAllocEx failed, err={}", e));
    }

    let mut written = 0usize;
    let ok = WriteProcessMemory(
        proc,
        remote,
        dll_wide.as_ptr() as *const c_void,
        bytes,
        &mut written,
    );
    if ok == 0 {
        let e = GetLastError();
        VirtualFreeEx(proc, remote, 0, MEM_RELEASE);
        CloseHandle(proc);
        return Err(format!("WriteProcessMemory failed, err={}", e));
    }

    let k32 = GetModuleHandleW(wide("kernel32.dll").as_ptr());
    if k32.is_null() {
        return Err("GetModuleHandleW(kernel32) failed".into());
    }
    let loadlib = GetProcAddress(k32, b"LoadLibraryW\0".as_ptr());
    let loadlib = match loadlib {
        Some(f) => f as *const c_void,
        None => {
            return Err("GetProcAddress(LoadLibraryW) failed".into());
        }
    };

    let tid = 0u32;
    let thread = CreateRemoteThread(
        proc,
        std::ptr::null(),
        0,
        Some(std::mem::transmute::<*const c_void, unsafe extern "system" fn(*mut c_void) -> u32>(
            loadlib,
        )),
        remote,
        0,
        &tid as *const u32 as *mut u32,
    );
    if thread.is_null() {
        let e = GetLastError();
        VirtualFreeEx(proc, remote, 0, MEM_RELEASE);
        CloseHandle(proc);
        return Err(format!("CreateRemoteThread failed, err={}", e));
    }

    // wait until the loader thread actually finishes (up to 15s)
    let mut code: u32 = 0;
    let mut waited = 0u32;
    loop {
        GetExitCodeThread(thread, &mut code);
        if code != 259 {
            break;
        }
        if waited >= 15_000 {
            break;
        }
        WaitForSingleObject(thread, 100);
        waited += 100;
    }
    CloseHandle(thread);
    VirtualFreeEx(proc, remote, 0, MEM_RELEASE);
    CloseHandle(proc);

    if code == 0 {
        return Err(format!(
            "LoadLibraryW returned NULL in target, err={}",
            GetLastError()
        ));
    }

    // wait until the hook DLL finished installing hooks (it signals an event)
    wait_hook_ready(pid, 10_000);
    Ok(())
}

fn wait_hook_ready(pid: u32, timeout_ms: u32) {
    use windows_sys::Win32::System::Threading::{
        CreateEventW, OpenEventW, SetEvent, WaitForSingleObject,
    };
    let name: Vec<u16> = format!("Local\\fuck_ensp_hook_ready_{}", pid)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let ev = OpenEventW(0x001F0003, 0, name.as_ptr());
        let ev = if ev.is_null() {
            // the hook may signal before we open; create pre-signaled as fallback
            let e = CreateEventW(std::ptr::null(), 1, 0, name.as_ptr());
            if !e.is_null() {
                SetEvent(e);
            }
            e
        } else {
            ev
        };
        if !ev.is_null() {
            WaitForSingleObject(ev, timeout_ms);
            CloseHandle(ev);
        }
    }
}

unsafe fn launch_and_inject(exe: &str, dll: &str, args: &[String]) -> Result<(), String> {
    let mut cmd = String::from(exe);
    for a in args {
        cmd.push(' ');
        cmd.push_str(a);
    }
    let mut cmd_wide: Vec<u16> = cmd.encode_utf16().collect();
    cmd_wide.push(0);

    let mut si: STARTUPINFOW = std::mem::zeroed();
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut pi: PROCESS_INFORMATION = std::mem::zeroed();

    let ok = CreateProcessW(
        std::ptr::null(),
        cmd_wide.as_mut_ptr(),
        std::ptr::null(),
        std::ptr::null(),
        0,
        CREATE_SUSPENDED,
        std::ptr::null(),
        std::ptr::null(),
        &si,
        &mut pi,
    );
    if ok == 0 {
        return Err(format!("CreateProcessW({}) failed, err={}", exe, GetLastError()));
    }

    match inject(pi.dwProcessId, dll) {
        Ok(()) => {
            ResumeThread(pi.hThread);
            CloseHandle(pi.hThread);
            CloseHandle(pi.hProcess);
            println!("OK launched pid={} injected", pi.dwProcessId);
            Ok(())
        }
        Err(e) => {
            let _ = windows_sys::Win32::System::Threading::TerminateProcess(pi.hProcess, 1);
            CloseHandle(pi.hThread);
            CloseHandle(pi.hProcess);
            Err(format!("inject into child failed: {}", e))
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: fuck_inject32 --pid <PID> <dll> | --launch <exe> <dll> [args]");
        std::process::exit(2);
    }
    let result = unsafe {
        if args[1] == "--pid" {
            let pid: u32 = args[2].parse().expect("invalid pid");
            inject(pid, &args[3])
        } else if args[1] == "--launch" {
            launch_and_inject(&args[2], &args[3], &args[4..])
        } else {
            Err(format!("unknown mode {}", args[1]))
        }
    };
    match result {
        Ok(()) => println!("OK"),
        Err(e) => {
            println!("FAIL {}", e);
            std::process::exit(1);
        }
    }
}
