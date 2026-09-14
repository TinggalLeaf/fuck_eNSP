use windows_sys::Win32::Networking::WinSock::*;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::*;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn mark(m: &str) {
    use std::io::Write;
    let p = format!("C:\\fuck_ensp_selftest_{}.log", std::process::id());
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(p).unwrap();
    writeln!(f, "{}", m).unwrap();
}

fn main() {
    mark("start");
    unsafe {
        let mut wsa: WSADATA = std::mem::zeroed();
        WSAStartup(0x0202, &mut wsa);

        let s = socket(2, 1, 6); // AF_INET, SOCK_STREAM, IPPROTO_TCP
        let mut sa: SOCKADDR_IN = std::mem::zeroed();
        sa.sin_family = 2;
        sa.sin_port = 65510u16.to_be();
        sa.sin_addr.S_un.S_addr = 0x0100007f_u32.to_be(); // 127.0.0.1
        mark("before connect");
        let rc = connect(s, &sa as *const SOCKADDR_IN as *const SOCKADDR, 16);
        mark(&format!("connect rc={}", rc));
        let msg = b"hello selftest";
        mark("before send");
        let _ = send(s, msg.as_ptr(), msg.len() as i32, 0);
        mark("after send");
        let mut buf = [0u8; 256];
        let _ = recv(s, buf.as_mut_ptr(), 256, 0);
        let _ = closesocket(s);

        let mut si: STARTUPINFOW = std::mem::zeroed();
        si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        let mut cmd = wide("cmd.exe /c exit");
        mark("before CreateProcessW");
        let ok = CreateProcessW(
            std::ptr::null(),
            cmd.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            0,
            std::ptr::null(),
            std::ptr::null(),
            &si,
            &mut pi,
        );
        mark(&format!("CreateProcessW ok={}", ok));
        if ok != 0 {
            WaitForSingleObject(pi.hProcess, 5000);
            CloseHandle(pi.hProcess);
            CloseHandle(pi.hThread);
        }
        WSACleanup();
        mark("done");
        std::thread::sleep(std::time::Duration::from_millis(800));
    }
}
