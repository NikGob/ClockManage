//! Small Windows integrations: admin check, autostart + self-heal via Task Scheduler,
//! opening folders.

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::Path;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const TASK_LOGON: &str = r"ClockManage\Logon";
const TASK_WATCH: &str = r"ClockManage\Watchdog";

#[cfg(windows)]
pub fn is_admin() -> bool {
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

#[cfg(not(windows))]
pub fn is_admin() -> bool {
    false
}

#[cfg(windows)]
fn schtasks(args: &[&str]) -> bool {
    std::process::Command::new("schtasks")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Logon task starts the app hidden with admin rights (no UAC prompt).
/// Watchdog task relaunches it every 5 minutes if it was killed; a running
/// instance ignores the `--background` relaunch.
#[cfg(windows)]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    if !enabled {
        schtasks(&["/Delete", "/TN", TASK_LOGON, "/F"]);
        schtasks(&["/Delete", "/TN", TASK_WATCH, "/F"]);
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let tr = format!("\"{}\" --background", exe.display());
    let a = schtasks(&["/Create", "/TN", TASK_LOGON, "/TR", &tr, "/SC", "ONLOGON", "/RL", "HIGHEST", "/F"]);
    let b = schtasks(&["/Create", "/TN", TASK_WATCH, "/TR", &tr, "/SC", "MINUTE", "/MO", "5", "/RL", "HIGHEST", "/F"]);
    if a && b {
        Ok(())
    } else {
        Err("Не удалось создать задачи автозапуска (нужны права администратора).".into())
    }
}

#[cfg(not(windows))]
pub fn set_autostart(_enabled: bool) -> Result<(), String> {
    Ok(())
}

pub fn open_path(path: &Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

pub fn reveal(path: &Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    }
    #[cfg(not(windows))]
    {
        if let Some(p) = path.parent() {
            open_path(p);
        }
    }
}
