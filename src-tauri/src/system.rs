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

fn parse_hex(c: &str) -> Option<u32> {
    let h = c.trim().trim_start_matches('#');
    (h.len() == 6).then(|| u32::from_str_radix(h, 16).ok()).flatten()
}

/// Paint the native title bar (Windows 11) in the app's surface colour so it blends in,
/// keeping native caption buttons, snap layouts and resizing. Ignored on older Windows.
#[cfg(windows)]
pub fn style_titlebar(window: &tauri::WebviewWindow, bg: &str, fg: &str, dark: bool) {
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_TEXT_COLOR: u32 = 36;
    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;
    // COLORREF is 0x00BBGGRR.
    let bgr = |rgb: u32| ((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff);
    let set = |attr: u32, v: u32| unsafe {
        DwmSetWindowAttribute(hwnd, attr, &v as *const u32 as *const _, 4);
    };
    set(DWMWA_USE_IMMERSIVE_DARK_MODE, dark as u32);
    if let Some(c) = parse_hex(bg) {
        set(DWMWA_CAPTION_COLOR, bgr(c));
        set(DWMWA_BORDER_COLOR, bgr(c));
    }
    if let Some(c) = parse_hex(fg) {
        set(DWMWA_TEXT_COLOR, bgr(c));
    }
}

#[cfg(not(windows))]
pub fn style_titlebar(_window: &tauri::WebviewWindow, _bg: &str, _fg: &str, _dark: bool) {
    let _ = parse_hex;
}

const FIREWALL_RULE: &str = "ClockManage phone sync";

/// Inbound rule for the phone API and discovery (TCP + UDP of this exe). Every network profile:
/// home Wi-Fi is often marked "public" by Windows, and the API answers paired phones only.
#[cfg(windows)]
pub fn allow_phone_firewall() {
    let run = |args: &[&str]| {
        std::process::Command::new("netsh")
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    let name = format!("name={FIREWALL_RULE}");
    if run(&["advfirewall", "firewall", "show", "rule", &name]) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let program = format!("program={}", exe.display());
    run(&["advfirewall", "firewall", "add", "rule", &name, "dir=in", "action=allow", &program, "enable=yes", "profile=any"]);
}

#[cfg(not(windows))]
pub fn allow_phone_firewall() {}

#[cfg(windows)]
pub fn remove_phone_firewall() {
    let _ = std::process::Command::new("netsh")
        .args(["advfirewall", "firewall", "delete", "rule", &format!("name={FIREWALL_RULE}")])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(not(windows))]
pub fn remove_phone_firewall() {}

/// Name of this PC, shown on the phone when it finds it.
pub fn pc_name() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "ПК".into())
}

/// `lan_ip`, looked up at most every 30 s (the state goes to the UI every second).
pub fn lan_ip_cached() -> Option<String> {
    use std::sync::Mutex;
    static CACHE: Mutex<Option<(std::time::Instant, Option<String>)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, ip)) = c.as_ref() {
        if at.elapsed() < std::time::Duration::from_secs(30) {
            return ip.clone();
        }
    }
    let ip = lan_ip();
    *c = Some((std::time::Instant::now(), ip.clone()));
    ip
}

/// The LAN address of the default interface (no packet is sent: UDP connect only picks a route).
pub fn lan_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.168.0.1:9").or_else(|_| s.connect("10.0.0.1:9")).ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then(|| ip.to_string())
}
