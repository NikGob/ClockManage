//! Enforcement of the block list on Windows.
//!
//! * Domains -> `hosts` file section (`0.0.0.0` / `::`), works for every browser and app.
//! * Domains and paths (`youtube.com/shorts`) -> browser policies `URLBlocklist`
//!   (Chrome, Edge, Brave, Yandex) and `WebsiteFilter` (Firefox). This is the only
//!   way to block a path without a proxy: hosts can only block whole domains.
//! * Apps -> watchdog kills matching processes while the lock is active.
//!
//! Everything written is remembered in `blocker.json` so cleanup removes only our entries,
//! even after a crash. Other platforms get a no-op implementation (dev builds).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Applied {
    pub active: bool,
    pub sites: Vec<String>,
    /// registry key -> values we appended to its list
    pub policies: Vec<(String, Vec<String>)>,
    /// Path rules the running Firefox already knows (it reads policies only at start).
    #[serde(default)]
    pub firefox_paths: Vec<String>,
}

pub struct Blocker {
    state_file: PathBuf,
    pub applied: Applied,
    pub last_error: Option<String>,
}

pub const HOSTS_BEGIN: &str = "# >>> ClockManage study lock (auto-generated, removed when the lock ends)";
pub const HOSTS_END: &str = "# <<< ClockManage";

/// Host names that go to the hosts file for one block-list entry.
pub fn hosts_names(site: &str) -> Vec<String> {
    if site.contains('/') {
        // Path rule: domain must stay reachable (normal YouTube is allowed).
        return vec![];
    }
    let mut v = vec![site.to_string()];
    if site.split('.').count() == 2 {
        v.push(format!("www.{site}"));
        v.push(format!("m.{site}"));
        v.push(format!("mobile.{site}"));
    }
    v
}

pub fn hosts_section(sites: &[String]) -> String {
    let mut s = String::new();
    s.push_str(HOSTS_BEGIN);
    s.push_str("\r\n");
    for site in sites {
        for h in hosts_names(site) {
            s.push_str(&format!("0.0.0.0 {h}\r\n:: {h}\r\n"));
        }
    }
    s.push_str(HOSTS_END);
    s.push_str("\r\n");
    s
}

/// Remove our section (if any) from hosts content.
pub fn strip_section(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut inside = false;
    for line in content.split_inclusive('\n') {
        let t = line.trim();
        if t == HOSTS_BEGIN {
            inside = true;
            continue;
        }
        if inside {
            if t == HOSTS_END {
                inside = false;
            }
            continue;
        }
        out.push_str(line);
    }
    out
}

pub fn with_section(content: &str, sites: &[String]) -> String {
    let mut base = strip_section(content);
    if !base.is_empty() && !base.ends_with('\n') {
        base.push_str("\r\n");
    }
    base + &hosts_section(sites)
}

/// Chromium URLBlocklist format: `x.com` (domain + subdomains) or `youtube.com/shorts`.
pub fn chromium_rules(sites: &[String]) -> Vec<String> {
    sites.to_vec()
}

/// Firefox WebsiteFilter match patterns.
pub fn firefox_rules(sites: &[String]) -> Vec<String> {
    let mut v = vec![];
    for s in sites {
        let (host, path) = match s.split_once('/') {
            Some((h, p)) => (h, format!("/{p}*")),
            None => (s.as_str(), "/*".to_string()),
        };
        v.push(format!("*://{host}{path}"));
        v.push(format!("*://*.{host}{path}"));
    }
    v
}

impl Blocker {
    pub fn new(state_file: PathBuf) -> Self {
        let applied = std::fs::read_to_string(&state_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { state_file, applied, last_error: None }
    }

    fn persist(&self) {
        if let Ok(s) = serde_json::to_string_pretty(&self.applied) {
            let _ = std::fs::write(&self.state_file, s);
        }
    }

    /// Make the system match `want` (Some = block these sites, None = no blocking).
    /// Cheap to call repeatedly: it re-checks and repairs the hosts section.
    pub fn sync(&mut self, want: Option<&[String]>, force: bool, restart_firefox: bool) -> Option<String> {
        let result = match want {
            Some(sites) => {
                if force || !self.applied.active || self.applied.sites != sites {
                    platform::apply(&mut self.applied, sites)
                } else {
                    platform::verify(&self.applied)
                }
            }
            None => {
                if self.applied.active || force {
                    platform::clear(&mut self.applied)
                } else {
                    Ok(())
                }
            }
        };
        self.last_error = result.err();
        let note = self.sync_firefox(want, restart_firefox);
        self.persist();
        note
    }

    /// Firefox reads enterprise policies only at start-up, so a running Firefox would still
    /// show path-blocked pages (YouTube Shorts). Domains are covered by hosts immediately.
    /// When the path rules change, restart Firefox; it restores its tabs after the restart.
    fn sync_firefox(&mut self, want: Option<&[String]>, restart: bool) -> Option<String> {
        let paths: Vec<String> = want.unwrap_or(&[]).iter().filter(|s| s.contains('/')).cloned().collect();
        let running = platform::firefox_running();
        if !running {
            self.applied.firefox_paths = paths;
            return None;
        }
        if want.is_none() || paths.is_empty() || paths.iter().all(|p| self.applied.firefox_paths.contains(p)) {
            return None;
        }
        if !restart {
            return Some("Firefox перезапусти вручную — иначе Shorts в нём не заблокирован.".into());
        }
        if platform::restart_firefox() {
            self.applied.firefox_paths = paths;
            Some("Firefox перезапущен, чтобы заблокировать Shorts. Вкладки восстановятся.".into())
        } else {
            Some("Не удалось перезапустить Firefox — перезапусти его вручную.".into())
        }
    }

    /// Kill blocked apps. Returns names of killed processes.
    pub fn kill_apps(&self, apps: &[String]) -> Vec<String> {
        platform::kill_apps(apps)
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::process::CommandExt;
    use winreg::enums::*;
    use winreg::RegKey;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    const CHROMIUM_KEYS: [&str; 4] = [
        r"SOFTWARE\Policies\Google\Chrome\URLBlocklist",
        r"SOFTWARE\Policies\Microsoft\Edge\URLBlocklist",
        r"SOFTWARE\Policies\BraveSoftware\Brave\URLBlocklist",
        r"SOFTWARE\Policies\YandexBrowser\URLBlocklist",
    ];
    const FIREFOX_KEY: &str = r"SOFTWARE\Policies\Mozilla\Firefox\WebsiteFilter\Block";

    fn hosts_path() -> PathBuf {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        PathBuf::from(root).join(r"System32\drivers\etc\hosts")
    }

    fn write_hosts(content: &str) -> Result<(), String> {
        let path = hosts_path();
        // Clear a read-only attribute some "optimizers" set on hosts.
        if let Ok(meta) = std::fs::metadata(&path) {
            let mut p = meta.permissions();
            if p.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                p.set_readonly(false);
                let _ = std::fs::set_permissions(&path, p);
            }
        }
        std::fs::write(&path, content).map_err(|e| format!("Не удалось записать hosts: {e}"))
    }

    fn flush_dns() {
        let _ = std::process::Command::new("ipconfig").arg("/flushdns").creation_flags(CREATE_NO_WINDOW).status();
    }

    fn read_list(key: &RegKey) -> Vec<String> {
        let mut v = vec![];
        for i in 1.. {
            match key.get_value::<String, _>(i.to_string()) {
                Ok(s) => v.push(s),
                Err(_) => break,
            }
        }
        v
    }

    fn write_list(key: &RegKey, old_len: usize, items: &[String]) -> std::io::Result<()> {
        for (i, s) in items.iter().enumerate() {
            key.set_value((i + 1).to_string(), s)?;
        }
        for i in items.len()..old_len {
            let _ = key.delete_value((i + 1).to_string());
        }
        Ok(())
    }

    /// Append our rules to a list policy; returns the rules that were actually added.
    fn add_policy(path: &str, rules: &[String]) -> std::io::Result<Vec<String>> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let (key, _) = hklm.create_subkey(path)?;
        let mut list = read_list(&key);
        let old_len = list.len();
        let mut added = vec![];
        for r in rules {
            if !list.iter().any(|x| x.eq_ignore_ascii_case(r)) {
                list.push(r.clone());
                added.push(r.clone());
            }
        }
        write_list(&key, old_len, &list)?;
        Ok(added)
    }

    fn remove_policy(path: &str, ours: &[String]) -> std::io::Result<()> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let Ok(key) = hklm.open_subkey_with_flags(path, KEY_READ | KEY_WRITE) else {
            return Ok(());
        };
        let list = read_list(&key);
        let kept: Vec<String> = list.iter().filter(|x| !ours.iter().any(|o| o.eq_ignore_ascii_case(x))).cloned().collect();
        write_list(&key, list.len(), &kept)
    }

    fn policy_present(path: &str, ours: &[String]) -> bool {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let Ok(key) = hklm.open_subkey(path) else { return ours.is_empty() };
        let list = read_list(&key);
        ours.iter().all(|o| list.iter().any(|x| x.eq_ignore_ascii_case(o)))
    }

    pub fn apply(a: &mut Applied, sites: &[String]) -> Result<(), String> {
        // Start from a clean state so removed entries disappear.
        let _ = clear(a);
        let content = std::fs::read_to_string(hosts_path()).unwrap_or_default();
        write_hosts(&with_section(&content, sites))?;
        let mut errors = vec![];
        let chromium = chromium_rules(sites);
        for key in CHROMIUM_KEYS {
            match add_policy(key, &chromium) {
                Ok(added) => a.policies.push((key.to_string(), added)),
                Err(e) => errors.push(format!("{key}: {e}")),
            }
        }
        match add_policy(FIREFOX_KEY, &firefox_rules(sites)) {
            Ok(added) => a.policies.push((FIREFOX_KEY.to_string(), added)),
            Err(e) => errors.push(format!("Firefox: {e}")),
        }
        a.active = true;
        a.sites = sites.to_vec();
        flush_dns();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("Политики браузеров: {}", errors.join("; ")))
        }
    }

    pub fn verify(a: &Applied) -> Result<(), String> {
        let content = std::fs::read_to_string(hosts_path()).unwrap_or_default();
        let expected = hosts_section(&a.sites);
        let mut repaired = false;
        if !content.replace("\r\n", "\n").contains(&expected.replace("\r\n", "\n")) {
            write_hosts(&with_section(&content, &a.sites))?;
            repaired = true;
        }
        for (key, ours) in &a.policies {
            if !policy_present(key, ours) {
                let _ = add_policy(key, ours);
                repaired = true;
            }
        }
        if repaired {
            flush_dns();
        }
        Ok(())
    }

    pub fn clear(a: &mut Applied) -> Result<(), String> {
        let content = std::fs::read_to_string(hosts_path()).unwrap_or_default();
        let stripped = strip_section(&content);
        if stripped != content {
            write_hosts(&stripped)?;
        }
        for (key, ours) in a.policies.drain(..) {
            let _ = remove_policy(&key, &ours);
        }
        a.active = false;
        a.sites.clear();
        flush_dns();
        Ok(())
    }

    fn processes(name: &str) -> Vec<u32> {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
        };
        let mut out = vec![];
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut entry) != 0;
            while ok {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(name) {
                    out.push(entry.th32ProcessID);
                }
                ok = Process32NextW(snap, &mut entry) != 0;
            }
            CloseHandle(snap);
        }
        out
    }

    pub fn firefox_running() -> bool {
        !processes("firefox.exe").is_empty()
    }

    fn image_path(pid: u32) -> Option<PathBuf> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
            CloseHandle(h);
            ok.then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
        }
    }

    /// Kill Firefox and start it again un-elevated (through explorer), so it re-reads policies.
    /// Firefox treats the kill as a crash and restores the session automatically.
    pub fn restart_firefox() -> bool {
        let pids = processes("firefox.exe");
        let Some(path) = pids.iter().find_map(|&p| image_path(p)) else { return false };
        kill_apps(&["firefox.exe".to_string()]);
        for _ in 0..40 {
            if !firefox_running() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
        std::process::Command::new("explorer.exe").arg(&path).spawn().is_ok()
    }

    pub fn kill_apps(apps: &[String]) -> Vec<String> {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
        };
        use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

        if apps.is_empty() {
            return vec![];
        }
        let wanted: Vec<String> = apps.iter().map(|a| a.to_lowercase()).collect();
        let mut killed = vec![];
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return killed;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut entry) != 0;
            while ok {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if wanted.contains(&name.to_lowercase()) {
                    let h = OpenProcess(PROCESS_TERMINATE, 0, entry.th32ProcessID);
                    if !h.is_null() {
                        if TerminateProcess(h, 1) != 0 {
                            killed.push(name);
                        }
                        CloseHandle(h);
                    }
                }
                ok = Process32NextW(snap, &mut entry) != 0;
            }
            CloseHandle(snap);
        }
        killed
    }
}

#[cfg(not(windows))]
mod platform {
    //! Dev stub: logs instead of touching the system.
    use super::*;

    pub fn apply(a: &mut Applied, sites: &[String]) -> Result<(), String> {
        eprintln!("[blocker] would block: {sites:?}");
        a.active = true;
        a.sites = sites.to_vec();
        Ok(())
    }
    pub fn verify(_: &Applied) -> Result<(), String> {
        Ok(())
    }
    pub fn clear(a: &mut Applied) -> Result<(), String> {
        if a.active {
            eprintln!("[blocker] cleared");
        }
        a.active = false;
        a.sites.clear();
        a.policies.clear();
        Ok(())
    }
    pub fn kill_apps(_: &[String]) -> Vec<String> {
        vec![]
    }
    pub fn firefox_running() -> bool {
        false
    }
    pub fn restart_firefox() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_roundtrip() {
        let base = "127.0.0.1 localhost\r\n";
        let sites = vec!["x.com".to_string(), "youtube.com/shorts".to_string(), "web.telegram.org".to_string()];
        let with = with_section(base, &sites);
        assert!(with.contains("0.0.0.0 www.x.com"));
        assert!(with.contains("0.0.0.0 web.telegram.org"));
        assert!(!with.contains("youtube"));
        assert_eq!(strip_section(&with), base);
        // idempotent
        assert_eq!(with_section(&with, &sites), with);
    }

    #[test]
    fn firefox_patterns() {
        let r = firefox_rules(&["youtube.com/shorts".into()]);
        assert!(r.contains(&"*://*.youtube.com/shorts*".to_string()));
    }
}
