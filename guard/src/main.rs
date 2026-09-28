//! ClockManage launch guard.
//!
//! During study time ClockManage registers this exe as the Image File Execution Options
//! "Debugger" of blocked apps (AyuGram.exe, Discord.exe, ...). Windows then starts the guard
//! instead of the app, passing the original command line. The guard:
//! * lock active (fresh heartbeat file) -> drops an "attempt" note for ClockManage, which shows
//!   its "no-no-no" animation, and exits quietly: the app never starts, so there is no error;
//! * lock not active (stale entry after a crash) -> starts the app anyway, bypassing IFEO.
//!
//! It runs with normal user rights (no admin manifest), so no UAC prompt appears.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// ClockManage refreshes the heartbeat every few seconds while blocking.
const HEARTBEAT_MAX_AGE_MS: u128 = 20_000;

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// Split the first (possibly quoted) token off a Windows command line.
fn split_first(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('"') {
        match rest.find('"') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, ""),
        }
    } else {
        match s.find(char::is_whitespace) {
            Some(i) => (&s[..i], &s[i..]),
            None => (s, ""),
        }
    }
}

fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("com.nikgob.clockmanage")
}

fn lock_active(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("guard.beat"))
        .ok()
        .and_then(|s| s.trim().parse::<u128>().ok())
        .is_some_and(|beat| now_ms().saturating_sub(beat) < HEARTBEAT_MAX_AGE_MS)
}

fn report_attempt(dir: &Path, exe: &str) {
    let d = dir.join("attempts");
    let _ = std::fs::create_dir_all(&d);
    let _ = std::fs::write(d.join(format!("{}-{}.txt", now_ms(), std::process::id())), exe);
}

fn main() {
    let cmd = platform::command_line();
    let (_self_exe, rest) = split_first(&cmd);
    let original = rest.trim();
    if original.is_empty() {
        return;
    }
    let (target, _) = split_first(original);
    let exe = target.rsplit(['\\', '/']).next().unwrap_or(target);
    let dir = data_dir();
    if lock_active(&dir) {
        report_attempt(&dir, exe);
    } else {
        platform::launch_bypassing_ifeo(original);
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::Debug::{DebugActiveProcessStop, DebugSetProcessKillOnExit};
    use windows_sys::Win32::System::Environment::GetCommandLineW;
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, DEBUG_ONLY_THIS_PROCESS, PROCESS_INFORMATION, STARTUPINFOW,
    };

    pub fn command_line() -> String {
        unsafe {
            let p = GetCommandLineW();
            let mut n = 0;
            while *p.add(n) != 0 {
                n += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
        }
    }

    /// Creating a process as its debugger skips the IFEO "Debugger" redirect; detach at once.
    pub fn launch_bypassing_ifeo(cmdline: &str) {
        let mut w: Vec<u16> = cmdline.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let mut si: STARTUPINFOW = std::mem::zeroed();
            si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
            let ok = CreateProcessW(
                std::ptr::null(),
                w.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                DEBUG_ONLY_THIS_PROCESS,
                std::ptr::null(),
                std::ptr::null(),
                &si,
                &mut pi,
            );
            if ok != 0 {
                DebugSetProcessKillOnExit(0);
                DebugActiveProcessStop(pi.dwProcessId);
                CloseHandle(pi.hThread);
                CloseHandle(pi.hProcess);
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn command_line() -> String {
        std::env::args().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ")
    }
    pub fn launch_bypassing_ifeo(_cmdline: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quoted_and_plain() {
        let (a, b) = split_first(r#""C:\Program Files\ClockManage\cm-guard.exe" "C:\Apps\AyuGram.exe" -startintray"#);
        assert_eq!(a, r"C:\Program Files\ClockManage\cm-guard.exe");
        let (t, _) = split_first(b);
        assert_eq!(t, r"C:\Apps\AyuGram.exe");
        let (a, b) = split_first("guard.exe Discord.exe --x");
        assert_eq!((a, b.trim()), ("guard.exe", "Discord.exe --x"));
    }

    #[test]
    fn heartbeat_freshness() {
        let dir = std::env::temp_dir().join(format!("cmg-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!lock_active(&dir));
        std::fs::write(dir.join("guard.beat"), now_ms().to_string()).unwrap();
        assert!(lock_active(&dir));
        std::fs::write(dir.join("guard.beat"), "1").unwrap();
        assert!(!lock_active(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
