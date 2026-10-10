//! Persistent user configuration (config.json).

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::day::PlanBlock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Timing {
    /// Length of one work segment inside a block.
    pub work_segment_min: u32,
    /// Break between segments of one block.
    pub short_break_min: u32,
    /// Break between two blocks.
    pub between_blocks_min: u32,
    /// Lunch with timer.
    pub lunch_min: u32,
}

impl Default for Timing {
    fn default() -> Self {
        Self { work_segment_min: 45, short_break_min: 10, between_blocks_min: 20, lunch_min: 45 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BlockList {
    /// Domains (`x.com`) or domain + path (`youtube.com/shorts`).
    pub sites: Vec<String>,
    /// Executable names (`Telegram.exe`).
    pub apps: Vec<String>,
}

impl Default for BlockList {
    fn default() -> Self {
        Self {
            sites: [
                "web.telegram.org",
                "x.com",
                "twitter.com",
                "twitch.tv",
                "discord.com",
                "discordapp.com",
                "discord.gg",
                "youtube.com/shorts",
                "2ch.su/b",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            apps: ["Telegram.exe", "AyuGram.exe", "Discord.exe", "DiscordPTB.exe", "DiscordCanary.exe"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchemeVariant {
    /// Seed colour kept as is — strongest accent.
    Fidelity,
    /// Calm tonal spot.
    TonalSpot,
    /// Maximum colourfulness.
    Vibrant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Appearance {
    pub seed: String,
    pub mode: ThemeMode,
    pub variant: SchemeVariant,
    /// Mini timer in inverse colours: a dark pill on a light theme and vice versa.
    pub mini_contrast: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self { seed: "#2E7D32".into(), mode: ThemeMode::System, variant: SchemeVariant::Fidelity, mini_contrast: false }
    }
}

/// Kind of day. Ordered by strictness: during the lock a day may only move up this order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum DayKind {
    Off,
    Light,
    #[default]
    Full,
}

impl DayKind {
    pub const ALL: [DayKind; 3] = [DayKind::Full, DayKind::Light, DayKind::Off];

    /// "full" | "light" | "off" — the same names as in config.json.
    pub fn key(self) -> &'static str {
        match self {
            DayKind::Full => "full",
            DayKind::Light => "light",
            DayKind::Off => "off",
        }
    }

    pub fn from_key(s: &str) -> Option<DayKind> {
        DayKind::ALL.into_iter().find(|k| k.key() == s.trim())
    }

    pub fn label(self) -> &'static str {
        match self {
            DayKind::Full => "Полный",
            DayKind::Light => "Лёгкий",
            DayKind::Off => "Выходной",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct DayProfile {
    /// Plan a new day of this kind starts with.
    pub plan: Vec<PlanBlock>,
    /// "Начать день" turns on blocking.
    pub block: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Profiles {
    pub full: DayProfile,
    pub light: DayProfile,
    pub off: DayProfile,
}

impl Default for Profiles {
    fn default() -> Self {
        Self {
            full: DayProfile {
                plan: vec![
                    PlanBlock::new("Математика", 90),
                    PlanBlock::new("Словацкий", 90),
                    PlanBlock::new("Экстернат", 150),
                ],
                block: true,
            },
            light: DayProfile {
                plan: vec![PlanBlock::new("Математика", 60), PlanBlock::new("Словацкий", 60)],
                block: true,
            },
            off: DayProfile { plan: vec![], block: false },
        }
    }
}

impl Profiles {
    pub fn get(&self, k: DayKind) -> &DayProfile {
        match k {
            DayKind::Full => &self.full,
            DayKind::Light => &self.light,
            DayKind::Off => &self.off,
        }
    }

    pub fn get_mut(&mut self, k: DayKind) -> &mut DayProfile {
        match k {
            DayKind::Full => &mut self.full,
            DayKind::Light => &mut self.light,
            DayKind::Off => &mut self.off,
        }
    }
}

/// A non-study stretch with its own countdown: lunch, a nap, a walk…
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SegmentType {
    pub name: String,
    pub minutes: u32,
    /// The end is a loud alarm that rings until "Встал" (a nap). No 5-minute warning.
    pub alarm: bool,
    /// Blocked sites/apps open while it runs (until its planned end), like the old "ем за ПК".
    pub open_access: bool,
    /// Time to get ready before the countdown (coffee, getting to bed): the segment waits for
    /// "Лёг", only reminds when this is up. 0 = the countdown starts at once. Configs from
    /// before 0.4.7 have none: an alarm segment (a nap) gets [`DEFAULT_PREP_MIN`].
    pub prep_min: Option<u32>,
}

impl SegmentType {
    pub fn prep(&self) -> u32 {
        self.prep_min.unwrap_or(if self.alarm { DEFAULT_PREP_MIN } else { 0 })
    }
}

impl Default for SegmentType {
    fn default() -> Self {
        Self { name: "Отрезок".into(), minutes: 15, alarm: false, open_access: false, prep_min: None }
    }
}

pub fn default_segments(lunch_min: u32) -> Vec<SegmentType> {
    vec![
        SegmentType { name: "Обед".into(), minutes: lunch_min, ..Default::default() },
        SegmentType { name: "Сон".into(), minutes: 20, alarm: true, prep_min: Some(DEFAULT_PREP_MIN), ..Default::default() },
        SegmentType { name: "Прогулка".into(), minutes: 15, ..Default::default() },
    ]
}

pub const MAX_SEGMENT_MIN: u32 = 240;
/// Preparation before a nap unless the type says otherwise.
pub const DEFAULT_PREP_MIN: u32 = 10;
pub const MAX_PREP_MIN: u32 = 60;

/// Weekday keys, Monday..Sunday (the order of `Config::week`).
pub const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// How many days ahead a plan may be set for one date.
pub const MAX_PLAN_AHEAD_DAYS: i64 = 60;

/// A phone paired over the local network.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct PhoneDevice {
    pub id: String,
    pub name: String,
    /// Bearer token the phone sends with every request.
    pub token: String,
    pub paired_at: i64,
}

/// Phone sync over Wi-Fi: the PC serves the timer to the Android app in the same network.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Phone {
    pub enabled: bool,
    /// TCP port of the phone API (the discovery reply tells it to the phone).
    pub port: u16,
    /// Android package names blocked on the phone during the lock (sites come from `blocklist`).
    pub apps: Vec<String>,
    pub devices: Vec<PhoneDevice>,
}

pub const PHONE_PORT: u16 = 47811;
/// UDP port the PC answers discovery broadcasts on.
pub const PHONE_DISCOVERY_PORT: u16 = 47810;

impl Default for Phone {
    fn default() -> Self {
        Self {
            enabled: false,
            port: PHONE_PORT,
            apps: [
                "org.telegram.messenger",
                "com.discord",
                "com.twitter.android",
                "tv.twitch.android.app",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            devices: vec![],
        }
    }
}

pub const DEFAULT_PHRASE: &str =
    "Я осознанно прерываю учебный день, понимаю что это попадёт в лог, и через десять минут вернусь к работе";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Kind of each weekday, Monday..Sunday.
    pub week: [DayKind; 7],
    pub profiles: Profiles,
    /// Minutes after local midnight when blocking ends (22:00 = 1320).
    pub day_end_min: u32,
    /// Offset of the "study clock" from UTC. Moscow = +180, no DST.
    pub tz_offset_min: i32,
    pub timing: Timing,
    pub blocklist: BlockList,
    /// Grant access to blocked things while the timer is paused.
    pub pause_access: bool,
    pub pause_access_min: u32,
    pub emergency_phrase: String,
    pub emergency_min: u32,
    /// Loud reminder period while the timer waits for "start next part".
    pub reminder_sec: u32,
    pub sound: bool,
    pub overlay: bool,
    /// Restart a running Firefox when path rules (Shorts) start to apply.
    pub restart_firefox: bool,
    pub autostart: bool,
    pub mcp_enabled: bool,
    pub mcp_port: u16,
    pub appearance: Appearance,
    /// Types of non-study segments ("Отрезок" button and `type: "break"` plan items).
    /// Missing in an old config.json -> empty, and `normalize` fills it keeping the old lunch length.
    #[serde(default = "Vec::new")]
    pub segments: Vec<SegmentType>,
    pub phone: Phone,
    /// Plans set ahead for particular dates (MCP `set_plan` with `date`): that day starts with
    /// it instead of its profile's template. Dropped once that date's day exists.
    pub plan_overrides: BTreeMap<NaiveDate, Vec<PlanBlock>>,
    /// Before profiles (0.1.x): study day flags, migrated into `week` by `normalize`.
    #[serde(skip_serializing)]
    pub study_days: Option<[bool; 7]>,
    /// Before profiles (0.1.x): the single plan template, migrated into the full profile.
    #[serde(skip_serializing)]
    pub plan_template: Option<Vec<PlanBlock>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            week: [
                DayKind::Full,
                DayKind::Full,
                DayKind::Full,
                DayKind::Full,
                DayKind::Full,
                DayKind::Off,
                DayKind::Off,
            ],
            profiles: Profiles::default(),
            day_end_min: 22 * 60,
            tz_offset_min: 180,
            timing: Timing::default(),
            blocklist: BlockList::default(),
            pause_access: false,
            pause_access_min: 10,
            emergency_phrase: DEFAULT_PHRASE.into(),
            emergency_min: 10,
            reminder_sec: 60,
            sound: true,
            overlay: true,
            restart_firefox: true,
            autostart: true,
            mcp_enabled: true,
            mcp_port: 0,
            appearance: Appearance::default(),
            segments: default_segments(45),
            phone: Phone::default(),
            plan_overrides: BTreeMap::new(),
            study_days: None,
            plan_template: None,
        }
    }
}

/// Which parts of the config are protected while the lock is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditContext {
    /// Blocking is currently in force (study day started and not finished, or a blocking
    /// single timer runs). Outside of it everything may be changed.
    pub locked: bool,
}

impl Config {
    pub fn profile(&self, k: DayKind) -> &DayProfile {
        self.profiles.get(k)
    }

    /// Kind of a day on `date` by the week schedule.
    pub fn kind_on(&self, date: NaiveDate) -> DayKind {
        self.week[date.weekday().num_days_from_monday() as usize]
    }

    /// The plan a day on `date` starts with: the one set ahead for that date, else the template.
    pub fn plan_on(&self, date: NaiveDate) -> &Vec<PlanBlock> {
        self.plan_overrides.get(&date).unwrap_or(&self.profile(self.kind_on(date)).plan)
    }

    /// Drop plans set ahead for `date` and earlier: those days have their own plan now.
    pub fn drop_overrides_through(&mut self, date: NaiveDate) -> bool {
        let n = self.plan_overrides.len();
        self.plan_overrides.retain(|d, _| *d > date);
        n != self.plan_overrides.len()
    }

    /// The segment type called `name` (case-insensitive).
    pub fn segment_type(&self, name: &str) -> Option<&SegmentType> {
        let n = name.trim();
        self.segments.iter().find(|t| t.name.eq_ignore_ascii_case(n) || t.name.to_lowercase() == n.to_lowercase())
    }

    pub fn normalize(&mut self) {
        if let Some(days) = self.study_days.take() {
            self.week = days.map(|d| if d { DayKind::Full } else { DayKind::Off });
        }
        if let Some(plan) = self.plan_template.take() {
            self.profiles.full.plan = plan;
        }
        self.timing.work_segment_min = self.timing.work_segment_min.clamp(5, 240);
        self.timing.short_break_min = self.timing.short_break_min.clamp(1, 120);
        self.timing.between_blocks_min = self.timing.between_blocks_min.clamp(1, 180);
        self.timing.lunch_min = self.timing.lunch_min.clamp(5, 180);
        self.pause_access_min = self.pause_access_min.clamp(1, 60);
        self.emergency_min = self.emergency_min.clamp(1, 60);
        self.reminder_sec = self.reminder_sec.clamp(15, 600);
        self.day_end_min = self.day_end_min.min(24 * 60 - 1);
        self.blocklist.sites = normalize_list(&self.blocklist.sites, normalize_site);
        self.blocklist.apps = normalize_list(&self.blocklist.apps, normalize_app);
        // Configs from before 0.3 have no segment types: start from the defaults, lunch keeps
        // its old length.
        if self.segments.is_empty() {
            self.segments = default_segments(self.timing.lunch_min);
        }
        let mut segs: Vec<SegmentType> = vec![];
        for t in &self.segments {
            let name: String = t.name.trim().chars().take(24).collect();
            if name.is_empty() || segs.iter().any(|s| s.name.to_lowercase() == name.to_lowercase()) {
                continue;
            }
            let prep_min = Some(t.prep().min(MAX_PREP_MIN));
            segs.push(SegmentType { name, minutes: t.minutes.clamp(1, MAX_SEGMENT_MIN), prep_min, ..t.clone() });
        }
        segs.truncate(10);
        self.segments = segs;
        if self.phone.port < 1024 {
            self.phone.port = PHONE_PORT;
        }
        self.phone.apps = normalize_list(&self.phone.apps, normalize_package);
        self.phone.devices.retain(|d| d.token.len() >= 16);
        let p = &mut self.profiles;
        let plans = [&mut p.full.plan, &mut p.light.plan, &mut p.off.plan].into_iter().chain(self.plan_overrides.values_mut());
        for plan in plans {
            for b in plan.iter_mut() {
                b.normalize();
            }
            plan.retain(|b| b.minutes > 0);
            plan.truncate(16);
        }
        self.plan_overrides.retain(|_, p| !p.is_empty());
        while self.plan_overrides.len() > MAX_PLAN_AHEAD_DAYS as usize {
            self.plan_overrides.pop_last();
        }
        if self.emergency_phrase.trim().chars().count() < 30 {
            self.emergency_phrase = DEFAULT_PHRASE.into();
        }
    }

    /// Validate a config change against the lock rules. Returns a human readable refusal.
    pub fn check_update(&self, new: &Config, ctx: EditContext) -> Result<(), String> {
        if !ctx.locked {
            return Ok(());
        }
        if new.pause_access != self.pause_access {
            return Err(format!(
                "Доступ на паузе меняется до начала учебного дня или после {}.",
                fmt_hm(self.day_end_min)
            ));
        }
        let lower = |v: &[String]| v.iter().map(|s| s.to_ascii_lowercase()).collect::<Vec<_>>();
        let new_sites = lower(&new.blocklist.sites);
        if lower(&self.blocklist.sites).iter().any(|s| !new_sites.contains(s)) {
            return Err("Во время блокировки сайты можно только добавлять.".into());
        }
        let new_apps = lower(&new.blocklist.apps);
        if lower(&self.blocklist.apps).iter().any(|s| !new_apps.contains(s)) {
            return Err("Во время блокировки приложения можно только добавлять.".into());
        }
        let new_phone = lower(&new.phone.apps);
        if lower(&self.phone.apps).iter().any(|s| !new_phone.contains(s)) {
            return Err("Во время блокировки приложения телефона можно только добавлять.".into());
        }
        // Today is pinned in the day state, so the week and the profiles only shape future days.
        if new.day_end_min < self.day_end_min {
            return Err("Во время учёбы конец дня можно только сдвинуть позже.".into());
        }
        if new.tz_offset_min != self.tz_offset_min {
            return Err("Часовой пояс меняется только вне блокировки.".into());
        }
        if new.timing.work_segment_min != self.timing.work_segment_min {
            return Err("Длина отрезка меняется только вне блокировки.".into());
        }
        let t = (&new.timing, &self.timing);
        if t.0.short_break_min > t.1.short_break_min
            || t.0.between_blocks_min > t.1.between_blocks_min
            || t.0.lunch_min > t.1.lunch_min
        {
            return Err("Во время учёбы перерывы и обед можно только сократить.".into());
        }
        for t in &new.segments {
            match self.segment_type(&t.name) {
                Some(old) if t.minutes > old.minutes || t.prep() > old.prep() => {
                    return Err(format!("Во время учёбы отрезок «{}» можно только сократить (и его подготовку тоже).", old.name));
                }
                Some(old) if t.open_access && !old.open_access => {
                    return Err("Доступ на время отрезка включается только вне блокировки.".into());
                }
                None if t.open_access => return Err("Доступ на время отрезка включается только вне блокировки.".into()),
                _ => {}
            }
        }
        if new.emergency_min > self.emergency_min || new.emergency_phrase != self.emergency_phrase {
            return Err("Аварийный доступ настраивается только вне блокировки.".into());
        }
        if new.pause_access_min > self.pause_access_min {
            return Err("Лимит доступа на паузе нельзя увеличить во время блокировки.".into());
        }
        if !new.autostart && self.autostart {
            return Err("Автозапуск нельзя выключить во время блокировки.".into());
        }
        Ok(())
    }

    /// Add `sites` / `apps` to the block list, or take them out. Entries are normalized like the
    /// settings screen does; the lock rules are checked by the caller (`check_update`).
    pub fn edit_blocklist(&mut self, remove: bool, sites: &[String], apps: &[String]) -> BlocklistChange {
        let mut ch = BlocklistChange::default();
        ch.sites = edit_list(&mut self.blocklist.sites, sites, normalize_site, remove, &mut ch);
        ch.apps = edit_list(&mut self.blocklist.apps, apps, normalize_app, remove, &mut ch);
        ch
    }
}

/// One list of `edit_blocklist`; returns the entries added (or removed).
fn edit_list(list: &mut Vec<String>, input: &[String], normalize: fn(&str) -> Option<String>, remove: bool, ch: &mut BlocklistChange) -> Vec<String> {
    let mut changed: Vec<String> = vec![];
    for raw in input {
        let Some(n) = normalize(raw) else {
            ch.invalid.push(raw.trim().to_string());
            continue;
        };
        let at = list.iter().position(|s| s.eq_ignore_ascii_case(&n));
        match (remove, at) {
            (false, None) => {
                list.push(n.clone());
                changed.push(n);
            }
            (true, Some(at)) => changed.push(list.remove(at)),
            _ if !changed.iter().chain(&ch.unchanged).any(|s| s.eq_ignore_ascii_case(&n)) => ch.unchanged.push(n),
            _ => {}
        }
    }
    changed
}

/// What `edit_blocklist` did.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct BlocklistChange {
    /// Sites added (or removed), as stored.
    pub sites: Vec<String>,
    pub apps: Vec<String>,
    /// Already in the list (adding) or not in it (removing).
    pub unchanged: Vec<String>,
    /// Not a site / an exe name, or a system app that is never blocked.
    pub invalid: Vec<String>,
}

impl BlocklistChange {
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty() && self.apps.is_empty()
    }
}

fn normalize_list(v: &[String], f: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in v {
        if let Some(n) = f(s) {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(&n)) {
                out.push(n);
            }
        }
    }
    out
}

/// Processes that must never be blocked: Windows itself, the WebView runtime, ClockManage.
/// Blocking any of them would break the system (or the app) for the whole study day.
const PROTECTED_APPS: [&str; 22] = [
    "explorer.exe", "svchost.exe", "csrss.exe", "wininit.exe", "winlogon.exe", "lsass.exe",
    "services.exe", "smss.exe", "dwm.exe", "fontdrvhost.exe", "sihost.exe", "ctfmon.exe",
    "taskhostw.exe", "runtimebroker.exe", "searchhost.exe", "startmenuexperiencehost.exe",
    "shellexperiencehost.exe", "conhost.exe", "msedgewebview2.exe", "clockmanage.exe",
    "cm-guard.exe", "system",
];

/// Only a bare file name: "C:\x\Steam" -> "Steam.exe". System apps are dropped.
pub fn normalize_app(s: &str) -> Option<String> {
    let s = s.trim().rsplit(['\\', '/']).next().unwrap_or("").trim();
    let exe = if s.is_empty() {
        return None;
    } else if s.to_ascii_lowercase().ends_with(".exe") {
        s.to_string()
    } else {
        format!("{s}.exe")
    };
    (!is_protected_app(&exe)).then_some(exe)
}

pub fn is_protected_app(exe: &str) -> bool {
    let e = exe.to_ascii_lowercase();
    PROTECTED_APPS.contains(&e.as_str())
}

/// Phone apps that must stay usable: calls (emergency!), system UI, settings, launchers, ClockManage.
pub fn is_protected_package(p: &str) -> bool {
    let p = p.to_ascii_lowercase();
    ["com.android.systemui", "com.android.settings", "com.nikgob.clockmanage", "android"].contains(&p.as_str())
        || ["dialer", "launcher", "emergency", "incallui", ".phone", "telecom"].iter().any(|k| p.contains(k))
}

/// `org.telegram.messenger` (trimmed); anything that is not a package name is dropped.
pub fn normalize_package(s: &str) -> Option<String> {
    let s = s.trim();
    let valid = s.contains('.')
        && !s.starts_with('.')
        && !s.ends_with('.')
        && s.len() <= 120
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
    (valid && !is_protected_package(s)).then(|| s.to_string())
}

/// `https://www.YouTube.com/shorts/` -> `youtube.com/shorts`.
pub fn normalize_site(s: &str) -> Option<String> {
    let mut s = s.trim().to_ascii_lowercase();
    for p in ["https://", "http://", "*://", "*."] {
        if let Some(r) = s.strip_prefix(p) {
            s = r.to_string();
        }
    }
    if let Some(r) = s.strip_prefix("www.") {
        s = r.to_string();
    }
    while s.ends_with('/') || s.ends_with('*') {
        s.pop();
    }
    let host = s.split('/').next().unwrap_or("");
    let valid = !host.is_empty()
        && host.contains('.')
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    valid.then_some(s)
}

pub fn fmt_hm(min: u32) -> String {
    format!("{:02}:{:02}", min / 60, min % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_normalization() {
        assert_eq!(normalize_site("https://www.YouTube.com/shorts/").as_deref(), Some("youtube.com/shorts"));
        assert_eq!(normalize_site("x.com").as_deref(), Some("x.com"));
        assert_eq!(normalize_site("nonsense"), None);
        assert_eq!(normalize_site("  "), None);
    }

    #[test]
    fn system_apps_cannot_be_blocked() {
        let mut c = Config::default();
        c.blocklist.apps = vec!["explorer.exe".into(), "Steam".into(), r"C:\Games\Epic.exe".into(), "ClockManage.exe".into()];
        c.normalize();
        assert_eq!(c.blocklist.apps, vec!["Steam.exe".to_string(), "Epic.exe".to_string()]);
    }

    #[test]
    fn blocklist_edits() {
        let mut c = Config::default();
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let ch = c.edit_blocklist(false, &s(&["https://www.YouTube.com/", "X.com", "nonsense", "youtube.com"]), &s(&["Steam", r"C:\x\telegram.exe", "explorer.exe", " "]));
        assert_eq!(ch.sites, s(&["youtube.com"]));
        assert_eq!(ch.apps, s(&["Steam.exe"]));
        assert_eq!(ch.unchanged, s(&["x.com", "telegram.exe"]));
        assert_eq!(ch.invalid, s(&["nonsense", "explorer.exe", ""]));
        assert!(c.blocklist.sites.contains(&"youtube.com".to_string()) && c.blocklist.apps.contains(&"Steam.exe".to_string()));

        let before = c.clone();
        let ch = c.edit_blocklist(true, &s(&["YOUTUBE.com", "reddit.com"]), &s(&["steam.exe"]));
        assert_eq!((ch.sites, ch.apps, ch.unchanged), (s(&["youtube.com"]), s(&["Steam.exe"]), s(&["reddit.com"])));
        assert!(!c.blocklist.sites.contains(&"youtube.com".to_string()));
        // Removing is what the lock forbids.
        assert!(before.check_update(&c, EditContext { locked: true }).is_err());
        assert!(before.check_update(&c, EditContext { locked: false }).is_ok());
        assert!(c.edit_blocklist(true, &[], &[]).is_empty());
    }

    #[test]
    fn locked_blocklist_only_grows() {
        let cfg = Config::default();
        let ctx = EditContext { locked: true };
        let mut n = cfg.clone();
        n.blocklist.sites.push("reddit.com".into());
        assert!(cfg.check_update(&n, ctx).is_ok());
        n.blocklist.sites.retain(|s| s != "x.com");
        assert!(cfg.check_update(&n, ctx).is_err());
    }

    #[test]
    fn locked_rules_only_tighten() {
        let cfg = Config::default();
        let ctx = EditContext { locked: true };
        let mut n = cfg.clone();
        n.day_end_min = 23 * 60;
        n.timing.short_break_min = 5;
        n.week[2] = DayKind::Light;
        assert!(cfg.check_update(&n, ctx).is_ok());
        let mut n = cfg.clone();
        n.day_end_min = 21 * 60;
        assert!(cfg.check_update(&n, ctx).is_err());
        let mut n = cfg.clone();
        n.timing.between_blocks_min = 30;
        assert!(cfg.check_update(&n, ctx).is_err());
        let mut n = cfg.clone();
        n.timing.work_segment_min = 30;
        assert!(cfg.check_update(&n, ctx).is_err());
    }

    #[test]
    fn legacy_config_migrates() {
        let json = r#"{"study_days":[true,true,false,true,true,false,true],"plan_template":[{"name":"X","minutes":60}]}"#;
        let mut c: Config = serde_json::from_str(json).unwrap();
        c.normalize();
        assert_eq!(c.week[2], DayKind::Off);
        assert_eq!(c.week[6], DayKind::Full);
        assert_eq!(c.profiles.full.plan, vec![PlanBlock::new("X", 60)]);
        let out = serde_json::to_string(&c).unwrap();
        assert!(!out.contains("study_days") && !out.contains("plan_template"));
    }

    #[test]
    fn phone_apps_normalize_and_only_grow() {
        let mut c = Config::default();
        c.phone.apps = vec![" com.discord ".into(), "com.google.android.dialer".into(), "nonsense".into(), "com.discord".into()];
        c.normalize();
        assert_eq!(c.phone.apps, vec!["com.discord".to_string()]);
        let ctx = EditContext { locked: true };
        let mut n = c.clone();
        n.phone.apps.push("com.zhiliaoapp.musically".into());
        assert!(c.check_update(&n, ctx).is_ok());
        n.phone.apps.clear();
        assert!(c.check_update(&n, ctx).is_err());
        assert!(c.check_update(&n, EditContext { locked: false }).is_ok());
    }

    #[test]
    fn segment_types_migrate_and_only_tighten() {
        let mut c: Config = serde_json::from_str(r#"{"timing":{"lunch_min":60}}"#).unwrap();
        c.normalize();
        assert_eq!(c.segments.len(), 3);
        assert_eq!(c.segment_type("обед").unwrap().minutes, 60);
        assert!(c.segment_type("Сон").unwrap().alarm);
        // A nap gets 10 minutes to get ready, other segments start their countdown at once.
        assert_eq!(c.segment_type("Сон").unwrap().prep_min, Some(10));
        assert_eq!(c.segment_type("Обед").unwrap().prep_min, Some(0));
        let ctx = EditContext { locked: true };
        let mut n = c.clone();
        n.segments[0].minutes = 30;
        n.segments.push(SegmentType { name: "Душ".into(), minutes: 10, ..Default::default() });
        assert!(c.check_update(&n, ctx).is_ok());
        let mut n = c.clone();
        n.segments[1].minutes = 90;
        assert!(c.check_update(&n, ctx).is_err());
        let mut n = c.clone();
        n.segments[0].open_access = true;
        assert!(c.check_update(&n, ctx).is_err());
        assert!(c.check_update(&n, EditContext { locked: false }).is_ok());
        let mut n = c.clone();
        n.segments[1].prep_min = Some(20);
        assert!(c.check_update(&n, ctx).is_err());
        n.segments[1].prep_min = Some(5);
        assert!(c.check_update(&n, ctx).is_ok());
    }

    #[test]
    fn nap_from_an_old_config_gets_its_preparation() {
        // 0.4.6 configs have no prep_min: a nap gets the default, a walk none.
        let mut c: Config = serde_json::from_str(
            r#"{"segments":[{"name":"Сон","minutes":20,"alarm":true},{"name":"Прогулка","minutes":15}]}"#,
        )
        .unwrap();
        c.normalize();
        assert_eq!(c.segment_type("Сон").unwrap().prep(), DEFAULT_PREP_MIN);
        assert_eq!(c.segment_type("Прогулка").unwrap().prep(), 0);
        let back: Config = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back.segment_type("Сон").unwrap().prep_min, Some(DEFAULT_PREP_MIN));
    }

    #[test]
    fn pause_access_only_outside_lock() {
        let cfg = Config::default();
        let mut n = cfg.clone();
        n.pause_access = true;
        assert!(cfg.check_update(&n, EditContext { locked: true }).is_err());
        assert!(cfg.check_update(&n, EditContext { locked: false }).is_ok());
    }
}
