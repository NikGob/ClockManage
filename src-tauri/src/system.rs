//! Small OS integrations: admin check, autostart + self-heal (Task Scheduler on Windows, a
//! systemd user service on Linux), opening folders.

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::Path;
use clockmanage_core::i18n::Lang;
use clockmanage_core::tr;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
const TASK_LOGON: &str = r"ClockManage\Logon";
#[cfg(windows)]
const TASK_WATCH: &str = r"ClockManage\Watchdog";

#[cfg(windows)]
pub fn is_admin() -> bool {
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

/// Linux: whether the lock can change hosts and browser policies (see `blocker`).
#[cfg(target_os = "linux")]
pub fn is_admin() -> bool {
    crate::blocker::can_block_sites()
}

#[cfg(not(any(windows, target_os = "linux")))]
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
        Err(tr!("system_couldnt_create_the_autostart_tasks_administr"))
    }
}

#[cfg(target_os = "linux")]
const UNIT: &str = "clockmanage.service";

/// A systemd user service started with the graphical session. Like the Windows watchdog it comes
/// back every 5 minutes after the process is gone (a running instance ignores `--background`),
/// and it does nothing once the binary is removed.
#[cfg(target_os = "linux")]
fn autostart_unit(exe: &Path) -> String {
    let exe = exe.display().to_string().replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%").replace('$', "$$");
    format!(
        "[Unit]\nDescription=ClockManage\nConditionPathExists={exe}\nAfter=graphical-session.target\nPartOf=graphical-session.target\n\n\
         [Service]\nExecStart=\"{exe}\" --background\nRestart=always\nRestartSec=300\n\
         # Firefox and players the app starts live on when the app is restarted.\nKillMode=process\n\n\
         [Install]\nWantedBy=graphical-session.target\n"
    )
}

/// What `systemctl --user enable/disable` does, by hand: works without a running user manager.
#[cfg(target_os = "linux")]
fn write_autostart(config: &Path, exe: Option<&Path>) -> std::io::Result<()> {
    let dir = config.join("systemd/user");
    let (unit, link) = (dir.join(UNIT), dir.join("graphical-session.target.wants").join(UNIT));
    let _ = std::fs::remove_file(&link);
    let Some(exe) = exe else {
        return match std::fs::remove_file(&unit) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    };
    std::fs::create_dir_all(link.parent().unwrap_or(&dir))?;
    std::fs::write(&unit, autostart_unit(exe))?;
    std::os::unix::fs::symlink(format!("../{UNIT}"), &link)
}

#[cfg(target_os = "linux")]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let config = std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from).or_else(|| home.map(|h| h.join(".config")));
    let config = config.ok_or_else(|| tr!("system_couldnt_set_up_autostart", e = "HOME"))?;
    // An AppImage runs from a temporary mount: the service must start the file itself.
    let exe = std::env::var_os("APPIMAGE").map(std::path::PathBuf::from).or_else(|| std::env::current_exe().ok());
    let exe = if enabled { Some(exe.ok_or_else(|| tr!("system_couldnt_set_up_autostart", e = "current_exe"))?) } else { None };
    write_autostart(&config, exe.as_deref()).map_err(|e| tr!("system_couldnt_set_up_autostart", e = e))?;
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn set_autostart(_enabled: bool) -> Result<(), String> {
    Ok(())
}

/// Milliseconds since the last keyboard or mouse input in this session.
#[cfg(windows)]
pub fn idle_ms() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    if unsafe { GetLastInputInfo(&mut li) } == 0 {
        return 0;
    }
    unsafe { GetTickCount() }.wrapping_sub(li.dwTime) as u64
}

#[cfg(not(windows))]
pub fn idle_ms() -> u64 {
    u64::MAX
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

#[cfg(windows)]
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
/// The Windows display language: Russian for ru/uk/be, else English.
#[cfg(windows)]
pub fn ui_lang() -> Lang {
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    // Primary language ids: 0x19 Russian, 0x22 Ukrainian, 0x23 Belarusian.
    if matches!(id & 0x3ff, 0x19 | 0x22 | 0x23) {
        Lang::Ru
    } else {
        Lang::En
    }
}

#[cfg(not(windows))]
pub fn ui_lang() -> Lang {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()));
    Lang::from_locale(&locale.unwrap_or_default())
}

/// The interface language from the setting ("auto" = the Windows language), applied process-wide.
pub fn apply_lang(cfg: &clockmanage_core::Config) {
    clockmanage_core::i18n::set(cfg.lang.resolve(ui_lang()));
}

/// The local UTC offset right now, in minutes (the default "study clock" of a new install).
pub fn utc_offset_min() -> i32 {
    clockmanage_core::chrono::Local::now().offset().local_minus_utc() / 60
}

pub fn pc_name() -> String {
    let linux = || std::fs::read_to_string("/proc/sys/kernel/hostname").map(|s| s.trim().to_string()).ok().filter(|s| !s.is_empty());
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(linux)
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| tr!("system_pc"))
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn autostart_service_on_and_off() {
        let config = std::env::temp_dir().join(format!("cm-autostart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&config);
        write_autostart(&config, Some(Path::new("/opt/Clock Manage/clockmanage"))).unwrap();
        let unit = std::fs::read_to_string(config.join("systemd/user/clockmanage.service")).unwrap();
        assert!(unit.contains("ExecStart=\"/opt/Clock Manage/clockmanage\" --background\n"));
        assert!(unit.contains("Restart=always\nRestartSec=300\n") && unit.contains("WantedBy=graphical-session.target"));
        let link = config.join("systemd/user/graphical-session.target.wants/clockmanage.service");
        assert_eq!(std::fs::read_link(&link).unwrap(), Path::new("../clockmanage.service"));
        // Enabling again replaces the entry; turning it off removes both files.
        write_autostart(&config, Some(Path::new("/usr/bin/clockmanage"))).unwrap();
        write_autostart(&config, None).unwrap();
        assert!(!config.join("systemd/user/clockmanage.service").exists() && std::fs::symlink_metadata(&link).is_err());
        assert_eq!(autostart_unit(Path::new("/a/100%$x")).lines().find(|l| l.starts_with("ExecStart")), Some("ExecStart=\"/a/100%%$$x\" --background"));
    }
}
