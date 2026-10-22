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
#[cfg(windows)]
use clockmanage_core::i18n::tr;
use clockmanage_core::tr;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Applied {
    pub active: bool,
    pub sites: Vec<String>,
    /// registry key -> values we appended to its list
    pub policies: Vec<(String, Vec<String>)>,
    /// Firefox rules currently written and when they changed (unix ms). Firefox reads
    /// policies only at start-up, so any Firefox process older than this lacks them.
    #[serde(default)]
    pub ff_rules: Vec<String>,
    #[serde(default)]
    pub ff_rules_at: i64,
    /// `ff_rules_at` value for which a restart was already attempted (no restart loops).
    #[serde(default)]
    pub ff_handled_at: i64,
    /// Executables redirected to the launch guard via Image File Execution Options.
    #[serde(default)]
    pub ifeo: Vec<String>,
}

pub struct Blocker {
    state_file: PathBuf,
    pub applied: Applied,
    pub last_error: Option<String>,
    firefox_restart_pending: bool,
}

/// What the state shows of the blocker, kept apart from it: `sync` can hold the blocker for
/// seconds (hosts, DNS flush, a polkit prompt) and the window must not wait for that.
#[derive(Clone, Default)]
pub struct Status {
    pub error: Option<String>,
    pub active: bool,
}

/// Remove launch-guard redirects for these exes (only entries pointing at our guard).
pub fn clear_guard_redirects(exes: &[String]) {
    for e in exes {
        platform::ifeo_clear_any_guard(&e.to_ascii_lowercase());
    }
}

/// Restart Firefox so it loads the current rules. Slow (waits for Firefox): call it off the
/// timer thread and without holding the blocker lock.
pub fn restart_firefox() -> Result<(), String> {
    platform::restart_firefox()
}

pub const HOSTS_BEGIN: &str = "# >>> ClockManage study lock (auto-generated, removed when the lock ends)";
pub const HOSTS_END: &str = "# <<< ClockManage";

/// Host names that go to the hosts file for one block-list entry.
fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

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

const BROWSERS: [&str; 7] = ["chrome.exe", "msedge.exe", "brave.exe", "firefox.exe", "browser.exe", "opera.exe", "vivaldi.exe"];
const BLOCKED_PAGE_TITLES: [&str; 3] = ["blocked page", "страница заблокирована", "заблокированная страница"];

pub fn match_blocked_tab(exe: &str, title: &str, sites: &[String]) -> Option<String> {
    let exe = exe.rsplit(['\\', '/']).next().unwrap_or(exe).to_ascii_lowercase();
    if !BROWSERS.contains(&exe.as_str()) {
        return None;
    }
    // "x.com - Google Chrome", "x.com — Mozilla Firefox"
    let mut page = title.trim().to_lowercase();
    for sep in [" - ", " — ", " – "] {
        if let Some((head, _)) = page.split_once(sep) {
            page = head.trim().to_string();
        }
    }
    let page = page.as_str();
    if BLOCKED_PAGE_TITLES.contains(&page) {
        return Some(page.to_string());
    }
    let host = page.strip_prefix("www.").unwrap_or(page);
    sites.iter().find_map(|s| {
        let site_host = s.split('/').next().unwrap_or(s);
        (host == site_host || host.ends_with(&format!(".{site_host}"))).then(|| site_host.to_string())
    })
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
        // What we changed in the system: losing it would leave policies nobody removes.
        let applied = crate::store::read_json(&state_file).unwrap_or_default();
        Self { state_file, applied, last_error: None, firefox_restart_pending: false }
    }

    pub fn status(&self) -> Status {
        Status { error: self.last_error.clone(), active: self.applied.active }
    }

    fn persist(&self) {
        if let Ok(s) = serde_json::to_string_pretty(&self.applied) {
            let _ = crate::store::write_atomic(&self.state_file, &s);
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
                    platform::verify(&mut self.applied)
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

    /// Firefox reads enterprise policies only at start-up, and keeps already open connections
    /// alive, so a Firefox that was started before the current rules were written still opens
    /// blocked pages. Detect that by process start time and restart it (tabs are restored).
    fn sync_firefox(&mut self, want: Option<&[String]>, restart: bool) -> Option<String> {
        want?;
        if self.applied.ff_rules.is_empty() || self.applied.ff_handled_at == self.applied.ff_rules_at {
            return None;
        }
        let started = platform::firefox_started_at()?;
        if started >= self.applied.ff_rules_at {
            return None;
        }
        self.applied.ff_handled_at = self.applied.ff_rules_at;
        if !restart {
            return Some(tr!("blocker_firefox_was_started_before_the_lock"));
        }
        // The restart itself takes seconds: the caller runs it off the timer thread.
        self.firefox_restart_pending = true;
        None
    }

    /// True once when a Firefox restart was requested by `sync`.
    pub fn take_firefox_restart(&mut self) -> bool {
        std::mem::take(&mut self.firefox_restart_pending)
    }

    /// Mark the current rules as delivered to Firefox (after a manual restart).
    pub fn mark_firefox_restarted(&mut self) {
        self.applied.ff_handled_at = self.applied.ff_rules_at;
        self.persist();
    }

    /// If the focused browser tab shows a blocked site (its error/blocked page is titled with the
    /// bare host, e.g. "x.com - Google Chrome"), return that host. A heuristic: browsers do not
    /// report policy/DNS blocks to other programs.
    pub fn blocked_tab(&self, sites: &[String]) -> Option<String> {
        let (exe, title) = platform::foreground_window()?;
        match_blocked_tab(&exe, &title, sites)
    }

    /// Redirect launches of blocked apps to the guard (`cm-guard.exe` next to our exe) while
    /// `apps` is Some; remove every redirect we made when None. The guard stops the launch
    /// before the app starts, so Windows shows no "cannot access" error as it does when a
    /// freshly started process is killed. `verify` re-checks entries someone may have removed.
    pub fn sync_launch_guard(&mut self, apps: Option<&[String]>, verify: bool) {
        let guard = std::env::current_exe().ok().map(|p| p.with_file_name("cm-guard.exe")).filter(|p| p.exists());
        let want: Vec<String> = match (apps, &guard) {
            (Some(a), Some(_)) => a.iter().map(|s| s.to_ascii_lowercase()).collect(),
            _ => vec![],
        };
        let mut changed = false;
        let current = std::mem::take(&mut self.applied.ifeo);
        for exe in current {
            if want.contains(&exe) && (!verify || guard.as_ref().is_some_and(|g| platform::ifeo_present(&exe, g))) {
                self.applied.ifeo.push(exe);
            } else {
                if let Some(g) = &guard {
                    platform::ifeo_clear(&exe, g);
                } else {
                    platform::ifeo_clear_any_guard(&exe);
                }
                changed = true;
            }
        }
        if let Some(g) = &guard {
            for exe in &want {
                if !self.applied.ifeo.contains(exe) && platform::ifeo_set(exe, g) {
                    self.applied.ifeo.push(exe.clone());
                    changed = true;
                }
            }
        }
        if changed {
            self.persist();
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
        std::fs::write(&path, content).map_err(|e| tr!("blocker_couldnt_write_hosts", e = e))
    }

    fn flush_dns() {
        let _ = std::process::Command::new("ipconfig").arg("/flushdns").creation_flags(CREATE_NO_WINDOW).status();
    }

    /// Chromium rereads policies when Group Policy signals a refresh, otherwise only every ~15 min,
    /// so without the signal the lock reaches the browser up to 15 min late (behind a VPN extension
    /// or on a kept-alive connection hosts does nothing). A plain refresh finds no changed GPO and
    /// signals nothing: only `RP_FORCE` wakes the browsers.
    pub fn refresh_policies() {
        use windows_sys::Win32::System::GroupPolicy::{RefreshPolicyEx, RP_FORCE};
        unsafe { RefreshPolicyEx(1, RP_FORCE) };
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
        let (ff_rules, ff_at) = (std::mem::take(&mut a.ff_rules), a.ff_rules_at);
        let _ = take_out(a);
        a.ff_rules = ff_rules;
        a.ff_rules_at = ff_at;
        let content = std::fs::read_to_string(hosts_path()).unwrap_or_default();
        // Policies go in even if hosts is locked: behind a proxy they are the only block.
        let hosts = write_hosts(&with_section(&content, sites));
        let mut errors = vec![];
        let chromium = chromium_rules(sites);
        for key in CHROMIUM_KEYS {
            match add_policy(key, &chromium) {
                Ok(added) => a.policies.push((key.to_string(), added)),
                Err(e) => errors.push(format!("{key}: {e}")),
            }
        }
        let ff = firefox_rules(sites);
        match add_policy(FIREFOX_KEY, &ff) {
            Ok(added) => {
                a.policies.push((FIREFOX_KEY.to_string(), added));
                if a.ff_rules != ff {
                    a.ff_rules = ff;
                    a.ff_rules_at = now_ms();
                }
            }
            Err(e) => errors.push(format!("Firefox: {e}")),
        }
        a.active = true;
        a.sites = sites.to_vec();
        flush_dns();
        refresh_policies();
        let mut problems: Vec<String> = hosts.err().into_iter().collect();
        if !errors.is_empty() {
            problems.push(tr!("blocker_browser_policies", v = errors.join("; ")));
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("; "))
        }
    }

    pub fn verify(a: &mut Applied) -> Result<(), String> {
        let content = std::fs::read_to_string(hosts_path()).unwrap_or_default();
        let expected = hosts_section(&a.sites);
        let mut hosts = Ok(());
        if !content.replace("\r\n", "\n").contains(&expected.replace("\r\n", "\n")) {
            hosts = write_hosts(&with_section(&content, &a.sites));
            if hosts.is_ok() {
                flush_dns();
            }
        }
        // Put back rules someone removed, without a forced refresh: if a GPO owns the key, forcing
        // would wipe them again every 30 s. Browsers pick the repair up on their own ~15 min reload.
        let mut ff_repaired = false;
        for (key, ours) in &a.policies {
            if !policy_present(key, ours) {
                let _ = add_policy(key, ours);
                ff_repaired |= key == FIREFOX_KEY;
            }
        }
        if ff_repaired {
            a.ff_rules_at = now_ms();
        }
        hosts
    }

    pub fn clear(a: &mut Applied) -> Result<(), String> {
        take_out(a)?;
        flush_dns();
        refresh_policies();
        Ok(())
    }

    /// Remove our hosts section and policy rules without telling DNS and browsers yet: `apply`
    /// does that once, after the new rules are in (a forced refresh reapplies all of Group Policy).
    fn take_out(a: &mut Applied) -> Result<(), String> {
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
        a.ff_rules.clear();
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

    /// (exe path, title) of the foreground window.
    const IFEO: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options";

    fn guard_value(guard: &std::path::Path) -> String {
        format!("\"{}\"", guard.display())
    }

    /// Both registry views, so 32-bit apps are covered as well.
    fn ifeo_views() -> [u32; 2] {
        [KEY_WOW64_64KEY, KEY_WOW64_32KEY]
    }

    pub fn ifeo_set(exe: &str, guard: &std::path::Path) -> bool {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let ours = guard_value(guard);
        let mut ok = false;
        for view in ifeo_views() {
            let Ok((key, _)) = hklm.create_subkey_with_flags(format!(r"{IFEO}\{exe}"), KEY_READ | KEY_WRITE | view) else {
                continue;
            };
            match key.get_value::<String, _>("Debugger") {
                // Somebody else's debugger (a real one): leave it alone.
                Ok(v) if v != ours && !v.to_ascii_lowercase().contains("cm-guard.exe") => continue,
                _ => {}
            }
            ok |= key.set_value("Debugger", &ours).is_ok();
        }
        ok
    }

    pub fn ifeo_present(exe: &str, guard: &std::path::Path) -> bool {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        hklm.open_subkey_with_flags(format!(r"{IFEO}\{exe}"), KEY_READ | KEY_WOW64_64KEY)
            .and_then(|k| k.get_value::<String, _>("Debugger"))
            .is_ok_and(|v| v == guard_value(guard))
    }

    pub fn ifeo_clear(exe: &str, _guard: &std::path::Path) {
        ifeo_clear_any_guard(exe);
    }

    /// Remove a Debugger value only if it points at our guard.
    pub fn ifeo_clear_any_guard(exe: &str) {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        for view in ifeo_views() {
            if let Ok(key) = hklm.open_subkey_with_flags(format!(r"{IFEO}\{exe}"), KEY_READ | KEY_WRITE | view) {
                if key.get_value::<String, _>("Debugger").is_ok_and(|v| v.to_ascii_lowercase().contains("cm-guard.exe")) {
                    let _ = key.delete_value("Debugger");
                }
            }
        }
    }

    pub fn foreground_window() -> Option<(String, String)> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut buf = [0u16; 512];
            let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if n <= 0 {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let exe = image_path(pid)?;
            Some((exe.display().to_string(), String::from_utf16_lossy(&buf[..n as usize])))
        }
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

    /// Start time (unix ms) of the oldest running firefox.exe.
    pub fn firefox_started_at() -> Option<i64> {
        use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
        use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        let mut oldest: Option<i64> = None;
        for pid in processes("firefox.exe") {
            unsafe {
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if h.is_null() {
                    continue;
                }
                let z = || FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
                let (mut c, mut e, mut k, mut u) = (z(), z(), z(), z());
                if GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) != 0 {
                    let ft = ((c.dwHighDateTime as i64) << 32) | c.dwLowDateTime as i64;
                    let ms = (ft - 116_444_736_000_000_000) / 10_000;
                    oldest = Some(oldest.map_or(ms, |o| o.min(ms)));
                }
                CloseHandle(h);
            }
        }
        oldest
    }

    /// Kill Firefox and start it again un-elevated (through explorer), so it re-reads policies.
    /// Firefox treats the kill as a crash and restores the session automatically.
    pub fn restart_firefox() -> Result<(), String> {
        let pids = processes("firefox.exe");
        if pids.is_empty() {
            return Err(tr!("blocker_firefox_isnt_running"));
        }
        let path = pids.iter().find_map(|&p| image_path(p)).ok_or(tr("blocker_firefox_exe_path_not_found"))?;
        let wait = |running: bool, ms: u64| {
            for _ in 0..ms / 100 {
                if firefox_running() == running {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            firefox_running() == running
        };
        kill_apps(&["firefox.exe".to_string()]);
        if !wait(false, 5_000) {
            kill_apps(&["firefox.exe".to_string()]);
            if !wait(false, 3_000) {
                return Err(tr!("blocker_firefox_didnt_close"));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        // explorer.exe starts it with the user's normal (non-admin) rights.
        let _ = std::process::Command::new("explorer.exe").arg(&path).spawn();
        if wait(true, 6_000) {
            return Ok(());
        }
        // Fallback: start directly (inherits admin rights, but Firefox is back with the rules).
        std::process::Command::new(&path).spawn().map_err(|e| e.to_string())?;
        Ok(())
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

/// Whether the lock can change hosts and browser policies: root, or the .deb's polkit rule
/// for the root step. The AppImage has no install step, so it closes apps only (also on a
/// machine that has the .deb: pkexec runs only the installed binary).
#[cfg(target_os = "linux")]
pub fn can_block_sites() -> bool {
    platform::is_root() || platform::helper_installed()
}

/// `clockmanage --block-sites [site…]`, the root step (through pkexec, or the package's remove
/// script with no sites): make hosts and browser policies block exactly these sites, none = take
/// ours out. Under pkexec the binary does nothing else, whatever the arguments.
#[cfg(target_os = "linux")]
pub fn helper_main() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some(platform::HELPER_ARG) {
        return std::env::var_os("PKEXEC_UID").is_some().then_some(2);
    }
    if !platform::is_root() {
        return Some(2);
    }
    // pkexec keeps the caller's umask: folders root creates must stay readable for browsers.
    unsafe { libc::umask(0o022) };
    let mut sites = vec![];
    for a in &args[1..] {
        match clockmanage_core::config::normalize_site(a) {
            Some(s) if &s == a => sites.push(s),
            _ => return Some(2),
        }
    }
    // The app hands its language over in LANGUAGE (a C locale must not cancel it here).
    let lang = std::env::var("LANGUAGE").ok().filter(|l| !l.is_empty());
    let lang = lang.map(|l| clockmanage_core::i18n::Lang::from_locale(l.split(':').next().unwrap_or_default()));
    clockmanage_core::i18n::set(lang.unwrap_or_else(crate::system::ui_lang));
    match platform::write_system(std::path::Path::new("/"), &sites) {
        Ok(()) => Some(0),
        Err(e) => {
            eprintln!("{e}");
            Some(1)
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    //! The app runs as the user. Hosts and browser policies are root's files, so the root step
    //! (`helper_main`, through pkexec) writes them; the .deb ships the polkit rule that lets the
    //! active user run it without a password. Blocked apps are the user's own processes: closing
    //! them needs no rights. Nothing stops an app before it starts (no IFEO on Linux).
    use super::*;
    use serde_json::{json, Value};
    use std::path::Path;

    const POLKIT_POLICY: &str = "/usr/share/polkit-1/actions/com.nikgob.clockmanage.policy";
    /// The binary the polkit rule names (`exec.path`): pkexec runs only this one.
    const HELPER: &str = "/usr/bin/clockmanage";
    pub const HELPER_ARG: &str = "--block-sites";
    /// Chrome, Chromium (Debian and Ubuntu), Edge, Brave, Yandex: our own file in each, read live.
    const CHROMIUM_DIRS: [&str; 6] = [
        "etc/opt/chrome/policies/managed",
        "etc/chromium/policies/managed",
        "etc/chromium-browser/policies/managed",
        "etc/opt/edge/policies/managed",
        "etc/brave/policies/managed",
        "etc/opt/yandex/browser/policies/managed",
    ];
    const OURS: &str = "clockmanage.json";
    const FIREFOX_DIR: &str = "etc/firefox/policies";
    const FIREFOX_NAMES: [&str; 3] = ["firefox", "firefox-bin", "firefox-esr"];

    pub fn is_root() -> bool {
        unsafe { libc::geteuid() == 0 }
    }

    pub fn helper_installed() -> bool {
        crate::system::own_appimage().is_none() && Path::new(POLKIT_POLICY).exists() && Path::new(HELPER).exists()
    }

    fn read_json(p: &Path) -> Option<Value> {
        std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok())
    }

    /// Browsers run as the user and must read what root wrote.
    fn write_file(p: &Path, s: &str) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(p, s)?;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o644))
    }

    fn remove(p: &Path) -> std::io::Result<()> {
        match std::fs::remove_file(p) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// The system blocks exactly `sites` (none: nothing of ours is left). Reads only, as the user.
    pub fn in_place(root: &Path, sites: &[String]) -> bool {
        let hosts = std::fs::read_to_string(root.join("etc/hosts")).unwrap_or_default().replace("\r\n", "\n");
        let hosts_ok = if sites.is_empty() {
            !hosts.lines().any(|l| l.trim() == HOSTS_BEGIN)
        } else {
            hosts.contains(&hosts_section(sites).replace("\r\n", "\n"))
        };
        let chromium_ok = CHROMIUM_DIRS.iter().all(|d| {
            let p = root.join(d).join(OURS);
            if sites.is_empty() {
                !p.exists()
            } else {
                read_json(&p).is_some_and(|v| v["URLBlocklist"] == json!(chromium_rules(sites)))
            }
        });
        hosts_ok && chromium_ok && firefox_in_place(root, &firefox_rules(sites))
    }

    fn firefox_in_place(root: &Path, rules: &[String]) -> bool {
        let dir = root.join(FIREFOX_DIR);
        if rules.is_empty() {
            return !dir.join(OURS).exists();
        }
        let doc = read_json(&dir.join("policies.json")).unwrap_or_default();
        let block = doc.pointer("/policies/WebsiteFilter/Block").and_then(Value::as_array).cloned().unwrap_or_default();
        rules.iter().all(|r| block.iter().any(|x| x.as_str() == Some(r)))
    }

    fn block_list(doc: &mut Value) -> Option<&mut Vec<Value>> {
        let mut v = doc;
        for (key, empty) in [("policies", json!({})), ("WebsiteFilter", json!({})), ("Block", json!([]))] {
            v = v.as_object_mut()?.entry(key).or_insert(empty);
        }
        v.as_array_mut()
    }

    /// Our rules go into Firefox's one policies.json next to whatever an admin put there. What we
    /// added is remembered beside it (Firefox reads nothing else there), so taking our rules out
    /// leaves the admin's alone, and a file we created goes away with them.
    fn write_firefox(root: &Path, rules: &[String]) -> std::io::Result<()> {
        let dir = root.join(FIREFOX_DIR);
        let (path, mark) = (dir.join("policies.json"), dir.join(OURS));
        let prev = read_json(&mark);
        if rules.is_empty() && prev.is_none() {
            return Ok(());
        }
        let prev = prev.unwrap_or_default();
        let prev_added: Vec<&str> = prev["added"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let created = prev["created"].as_bool().unwrap_or(!path.exists());
        let mut doc = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).map_err(std::io::Error::other)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
            Err(e) => return Err(e),
        };
        let block = block_list(&mut doc).ok_or_else(|| std::io::Error::other("policies.json: no WebsiteFilter.Block list"))?;
        block.retain(|r| !r.as_str().is_some_and(|r| prev_added.contains(&r)));
        let mut added = vec![];
        for r in rules {
            if !block.iter().any(|x| x.as_str() == Some(r)) {
                block.push(json!(r));
                added.push(r.clone());
            }
        }
        if rules.is_empty() {
            if created && doc == json!({ "policies": { "WebsiteFilter": { "Block": [] } } }) {
                remove(&path)?;
            } else {
                write_file(&path, &serde_json::to_string_pretty(&doc).unwrap_or_default())?;
            }
            return remove(&mark);
        }
        std::fs::create_dir_all(&dir)?;
        write_file(&path, &serde_json::to_string_pretty(&doc).unwrap_or_default())?;
        write_file(&mark, &json!({ "added": added, "created": created }).to_string())
    }

    /// The root step itself: hosts, every Chromium policy file and Firefox's rules match `sites`.
    pub fn write_system(root: &Path, sites: &[String]) -> Result<(), String> {
        let mut problems = vec![];
        let hosts = root.join("etc/hosts");
        // Only a missing file counts as empty: an unreadable one must never be replaced.
        let content = match std::fs::read_to_string(&hosts) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            r => r,
        };
        let written = content.and_then(|content| {
            let new = if sites.is_empty() { strip_section(&content) } else { with_section(&content, sites).replace("\r\n", "\n") };
            if new == content { Ok(()) } else { write_file(&hosts, &new) }
        });
        if let Err(e) = written {
            problems.push(tr!("blocker_couldnt_write_hosts", e = e));
        }
        let mut errors = vec![];
        let chromium = serde_json::to_string_pretty(&json!({ "URLBlocklist": chromium_rules(sites) })).unwrap_or_default();
        for d in CHROMIUM_DIRS {
            let p = root.join(d).join(OURS);
            let r = if sites.is_empty() { remove(&p) } else { std::fs::create_dir_all(root.join(d)).and_then(|_| write_file(&p, &chromium)) };
            if let Err(e) = r {
                errors.push(format!("{}: {e}", p.display()));
            }
        }
        if let Err(e) = write_firefox(root, &firefox_rules(sites)) {
            errors.push(format!("Firefox: {e}"));
        }
        if !errors.is_empty() {
            problems.push(tr!("blocker_browser_policies", v = errors.join("; ")));
        }
        if root == Path::new("/") {
            // systemd-resolved rereads hosts by itself; its cache may still hold the real address.
            let _ = std::process::Command::new("resolvectl").arg("flush-caches").stderr(std::process::Stdio::null()).status();
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("; "))
        }
    }

    /// Make the system match `sites`: nothing to do when it already does, else the root step.
    /// Without the .deb's polkit rule there is no root step: sites stay open (the UI says so).
    fn privileged(sites: &[String]) -> Result<(), String> {
        let root = Path::new("/");
        if in_place(root, sites) {
            return Ok(());
        }
        if is_root() {
            return write_system(root, sites);
        }
        if !helper_installed() {
            return Ok(());
        }
        // pkexec keeps LANGUAGE: the root step's errors come in the app's language.
        let out = std::process::Command::new("pkexec")
            .arg(HELPER)
            .arg(HELPER_ARG)
            .args(sites)
            .env("LANGUAGE", clockmanage_core::i18n::get().key())
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| tr!("blocker_pkexec_didnt_run_the_site", e = e))?;
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        match out.status.code() {
            Some(0) => Ok(()),
            // pkexec's own: not authorized, dismissed, or no such program.
            Some(126 | 127) | None => Err(tr!("blocker_pkexec_didnt_run_the_site", e = if err.is_empty() { out.status.to_string() } else { err })),
            Some(_) => Err(err),
        }
    }

    pub fn apply(a: &mut Applied, sites: &[String]) -> Result<(), String> {
        let res = privileged(sites);
        let ff = firefox_rules(sites);
        // Only rules really in place count: without the root step nothing was written (no Firefox
        // restart for rules it doesn't have), and a failing hosts still leaves the policies written.
        if a.ff_rules != ff && firefox_in_place(Path::new("/"), &ff) {
            a.ff_rules = ff;
            a.ff_rules_at = now_ms();
        }
        a.active = true;
        a.sites = sites.to_vec();
        res
    }

    pub fn verify(a: &mut Applied) -> Result<(), String> {
        let ff = firefox_rules(&a.sites);
        let ff_lost = !firefox_in_place(Path::new("/"), &ff);
        let res = privileged(&a.sites);
        if ff_lost && firefox_in_place(Path::new("/"), &ff) {
            a.ff_rules = ff;
            a.ff_rules_at = now_ms();
        }
        res
    }

    pub fn clear(a: &mut Applied) -> Result<(), String> {
        privileged(&[])?;
        a.active = false;
        a.sites.clear();
        a.ff_rules.clear();
        Ok(())
    }

    /// This user's processes with the names they go by: the exe's file name and the first word of
    /// the command line (`Game.exe` under Wine, a script's interpreter otherwise). Other users'
    /// processes (fast user switching) are neither killed nor counted as this user's Firefox.
    fn processes() -> Vec<(i32, Vec<String>)> {
        use std::os::unix::fs::MetadataExt;
        let (me, uid) = (std::process::id() as i32, unsafe { libc::getuid() });
        let base = |s: &str| s.rsplit(['/', '\\']).next().unwrap_or(s).trim_end_matches(" (deleted)").to_string();
        std::fs::read_dir("/proc")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let pid: i32 = e.file_name().to_str()?.parse().ok()?;
                if e.metadata().ok()?.uid() != uid {
                    return None;
                }
                let mut names = vec![];
                if let Ok(exe) = std::fs::read_link(e.path().join("exe")) {
                    names.push(base(&exe.to_string_lossy()));
                }
                if let Ok(cmd) = std::fs::read(e.path().join("cmdline")) {
                    let argv0 = String::from_utf8_lossy(cmd.split(|&b| b == 0).next().unwrap_or_default()).to_string();
                    if !argv0.is_empty() {
                        names.push(base(&argv0));
                    }
                }
                (pid != me && !names.is_empty()).then_some((pid, names))
            })
            .collect()
    }

    /// `Telegram.exe` matches the process `Telegram` (and `Telegram.exe` under Wine).
    pub fn kill_apps(apps: &[String]) -> Vec<String> {
        if apps.is_empty() {
            return vec![];
        }
        let wanted: Vec<String> = apps
            .iter()
            .flat_map(|a| {
                let a = a.to_ascii_lowercase();
                let stem = a.strip_suffix(".exe").unwrap_or(&a).to_string();
                [a, stem]
            })
            .collect();
        let mut killed = vec![];
        for (pid, names) in processes() {
            if let Some(n) = names.iter().find(|n| wanted.contains(&n.to_ascii_lowercase())) {
                if unsafe { libc::kill(pid, libc::SIGKILL) } == 0 {
                    killed.push(n.clone());
                }
            }
        }
        killed
    }

    fn firefox_pids() -> Vec<i32> {
        processes().into_iter().filter(|(_, n)| n.iter().any(|n| FIREFOX_NAMES.contains(&n.as_str()))).map(|(p, _)| p).collect()
    }

    /// Start time (unix ms) of the oldest running Firefox: boot time + ticks since boot.
    pub fn firefox_started_at() -> Option<i64> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        let boot: i64 = stat.lines().find_map(|l| l.strip_prefix("btime "))?.trim().parse().ok()?;
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as i64;
        firefox_pids()
            .into_iter()
            .filter_map(|pid| {
                let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
                // Field 22, counted from the state that follows "(comm)".
                let ticks: i64 = s.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()?;
                Some(boot * 1000 + ticks * 1000 / hz)
            })
            .min()
    }

    /// Kill Firefox and start it again from PATH (a snap Firefox must go through its launcher).
    /// Firefox treats the kill as a crash and restores the session.
    pub fn restart_firefox() -> Result<(), String> {
        if firefox_pids().is_empty() {
            return Err(tr!("blocker_firefox_isnt_running"));
        }
        for _ in 0..50 {
            for pid in firefox_pids() {
                unsafe { libc::kill(pid, libc::SIGKILL) };
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            if firefox_pids().is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let mut child = std::process::Command::new("firefox").spawn().map_err(|e| e.to_string())?;
                std::thread::spawn(move || child.wait());
                return Ok(());
            }
        }
        Err(tr!("blocker_firefox_didnt_close"))
    }

    pub fn foreground_window() -> Option<(String, String)> {
        None
    }
    pub fn ifeo_set(_: &str, _: &std::path::Path) -> bool {
        false
    }
    pub fn ifeo_present(_: &str, _: &std::path::Path) -> bool {
        false
    }
    pub fn ifeo_clear(_: &str, _: &std::path::Path) {}
    pub fn ifeo_clear_any_guard(_: &str) {}
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    //! Dev stub: logs instead of touching the system.
    use super::*;

    pub fn apply(a: &mut Applied, sites: &[String]) -> Result<(), String> {
        eprintln!("[blocker] would block: {sites:?}");
        a.active = true;
        a.sites = sites.to_vec();
        Ok(())
    }
    pub fn verify(_: &mut Applied) -> Result<(), String> {
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
    pub fn foreground_window() -> Option<(String, String)> {
        None
    }
    pub fn ifeo_set(_: &str, _: &std::path::Path) -> bool {
        false
    }
    pub fn ifeo_present(_: &str, _: &std::path::Path) -> bool {
        false
    }
    pub fn ifeo_clear(_: &str, _: &std::path::Path) {}
    pub fn ifeo_clear_any_guard(_: &str) {}
    pub fn restart_firefox() -> Result<(), String> {
        Err(tr!("blocker_windows_only"))
    }
    pub fn firefox_started_at() -> Option<i64> {
        None
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
    fn blocked_tab_detection() {
        let sites = vec!["x.com".to_string(), "youtube.com/shorts".to_string()];
        let chrome = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
        assert_eq!(match_blocked_tab(chrome, "x.com - Google Chrome", &sites).as_deref(), Some("x.com"));
        assert_eq!(match_blocked_tab(chrome, "www.youtube.com - Google Chrome", &sites).as_deref(), Some("youtube.com"));
        assert_eq!(match_blocked_tab(chrome, "Лекция по матану - YouTube - Google Chrome", &sites), None);
        assert_eq!(match_blocked_tab(r"C:\x\Code.exe", "x.com - Visual Studio Code", &sites), None);
        assert!(match_blocked_tab(r"C:\ff\firefox.exe", "Страница заблокирована — Mozilla Firefox", &sites).is_some());
    }

    #[test]
    fn firefox_patterns() {
        let r = firefox_rules(&["youtube.com/shorts".into()]);
        assert!(r.contains(&"*://*.youtube.com/shorts*".to_string()));
    }

    #[cfg(target_os = "linux")]
    fn temp_root(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cm-root-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("etc")).unwrap();
        d
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_root_step_roundtrip() {
        let root = temp_root("roundtrip");
        let hosts = "127.0.0.1 localhost\n::1 localhost\n";
        std::fs::write(root.join("etc/hosts"), hosts).unwrap();
        let sites = vec!["x.com".to_string(), "youtube.com/shorts".to_string()];
        assert!(!platform::in_place(&root, &sites));
        platform::write_system(&root, &sites).unwrap();
        assert!(platform::in_place(&root, &sites));
        let written = std::fs::read_to_string(root.join("etc/hosts")).unwrap();
        assert!(written.starts_with(hosts) && written.contains("0.0.0.0 www.x.com\n") && !written.contains('\r'));
        let chrome: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("etc/opt/chrome/policies/managed/clockmanage.json")).unwrap()).unwrap();
        assert_eq!(chrome["URLBlocklist"], serde_json::json!(["x.com", "youtube.com/shorts"]));
        let ff = std::fs::read_to_string(root.join("etc/firefox/policies/policies.json")).unwrap();
        assert!(ff.contains("*://*.youtube.com/shorts*"));
        // A grown list replaces the old one.
        let more = vec!["x.com".to_string(), "youtube.com/shorts".to_string(), "reddit.com".to_string()];
        assert!(!platform::in_place(&root, &more));
        platform::write_system(&root, &more).unwrap();
        assert!(platform::in_place(&root, &more));
        // Taken out: hosts as it was, no policy file of ours, Firefox's file gone (we created it).
        platform::write_system(&root, &[]).unwrap();
        assert!(platform::in_place(&root, &[]));
        assert_eq!(std::fs::read_to_string(root.join("etc/hosts")).unwrap(), hosts);
        assert!(!root.join("etc/opt/chrome/policies/managed/clockmanage.json").exists());
        assert!(!root.join("etc/firefox/policies/policies.json").exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_keeps_the_admins_firefox_policies() {
        let root = temp_root("firefox");
        let dir = root.join("etc/firefox/policies");
        std::fs::create_dir_all(&dir).unwrap();
        let admin = serde_json::json!({ "policies": { "DisableTelemetry": true, "WebsiteFilter": { "Block": ["*://casino.example/*"] } } });
        std::fs::write(dir.join("policies.json"), admin.to_string()).unwrap();
        platform::write_system(&root, &["casino.example".to_string(), "x.com".to_string()]).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("policies.json")).unwrap()).unwrap();
        assert_eq!(doc["policies"]["DisableTelemetry"], true);
        assert!(doc["policies"]["WebsiteFilter"]["Block"].as_array().unwrap().contains(&serde_json::json!("*://x.com/*")));
        platform::write_system(&root, &[]).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("policies.json")).unwrap()).unwrap();
        assert_eq!(doc, admin);
        // A broken file is left alone, not replaced.
        std::fs::write(dir.join("policies.json"), "{ not json").unwrap();
        assert!(platform::write_system(&root, &["x.com".to_string()]).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("policies.json")).unwrap(), "{ not json");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_never_replaces_an_unreadable_hosts() {
        let root = temp_root("badhosts");
        let hosts = b"127.0.0.1 localhost # caf\xe9 in Latin-1\n".to_vec();
        std::fs::write(root.join("etc/hosts"), &hosts).unwrap();
        assert!(platform::write_system(&root, &["x.com".to_string()]).is_err());
        assert!(platform::write_system(&root, &[]).is_err());
        assert_eq!(std::fs::read(root.join("etc/hosts")).unwrap(), hosts);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_closes_a_blocked_app() {
        let exe = std::env::temp_dir().join(format!("cm-target-{}", std::process::id()));
        std::fs::copy("/bin/sleep", &exe).unwrap();
        let mut child = std::process::Command::new(&exe).arg("30").spawn().unwrap();
        let name = exe.file_name().unwrap().to_string_lossy().to_string();
        // Right after the spawn /proc may still show the test binary: the app closes blocked apps
        // on every pass, so give the new process a moment to show up as itself.
        let mut killed = vec![];
        for _ in 0..100 {
            killed = platform::kill_apps(&[format!("{name}.exe")]);
            if !killed.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(killed, vec![name]);
        assert!(!child.wait().unwrap().success());
        let _ = std::fs::remove_file(&exe);
    }

    /// Chromium browsers reread URLBlocklist only on this signal (or every ~15 min).
    #[cfg(windows)]
    #[test]
    #[ignore = "runs a real Group Policy refresh: cargo test -p clockmanage -- --ignored"]
    fn policy_refresh_wakes_browsers() {
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows_sys::Win32::System::GroupPolicy::{RegisterGPNotification, UnregisterGPNotification};
        use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
        unsafe {
            let event = CreateEventW(std::ptr::null(), 0, 0, std::ptr::null());
            assert!(RegisterGPNotification(event, 1) != 0);
            platform::refresh_policies();
            let signaled = WaitForSingleObject(event, 30_000) == WAIT_OBJECT_0;
            UnregisterGPNotification(event);
            CloseHandle(event);
            assert!(signaled, "no Group Policy signal: browsers keep the old block list for up to 15 min");
        }
    }
}
