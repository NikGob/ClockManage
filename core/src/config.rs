//! Persistent user configuration (config.json).

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
}

impl Default for Appearance {
    fn default() -> Self {
        Self { seed: "#2E7D32".into(), mode: ThemeMode::System, variant: SchemeVariant::Fidelity }
    }
}

/// What kind of day a weekday is. Ordered by strictness: `Off < Light < Full`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum DayKind {
    /// Day off: the study day cannot be started.
    Off,
    /// Light day: its own plan and an earlier end of the day.
    Light,
    #[default]
    Full,
}

impl DayKind {
    pub fn label(self) -> &'static str {
        match self {
            DayKind::Off => "выходной",
            DayKind::Light => "лёгкий день",
            DayKind::Full => "полный день",
        }
    }
}

/// Settings of the light day.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LightDay {
    pub plan: Vec<PlanBlock>,
    /// Blocking ends here on light days; never later than the full day's end.
    pub day_end_min: u32,
}

impl Default for LightDay {
    fn default() -> Self {
        Self { plan: vec![PlanBlock::new("Математика", 60), PlanBlock::new("Словацкий", 60)], day_end_min: 18 * 60 }
    }
}

pub const DEFAULT_PHRASE: &str =
    "Я осознанно прерываю учебный день, понимаю что это попадёт в лог, и через десять минут вернусь к работе";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Monday..Sunday.
    pub week: [DayKind; 7],
    /// Pre-0.1.6 schedule (study day yes/no). Read once to fill `week`, never written back.
    #[serde(rename = "study_days", skip_serializing)]
    pub legacy_study_days: Option<[bool; 7]>,
    /// Minutes after local midnight when blocking ends on a full day (22:00 = 1320).
    pub day_end_min: u32,
    pub light: LightDay,
    /// Offset of the "study clock" from UTC. Moscow = +180, no DST.
    pub tz_offset_min: i32,
    pub timing: Timing,
    pub plan_template: Vec<PlanBlock>,
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            week: [DayKind::Full, DayKind::Full, DayKind::Light, DayKind::Full, DayKind::Full, DayKind::Off, DayKind::Off],
            legacy_study_days: None,
            day_end_min: 22 * 60,
            light: LightDay::default(),
            tz_offset_min: 180,
            timing: Timing::default(),
            plan_template: vec![
                PlanBlock::new("Математика", 90),
                PlanBlock::new("Словацкий", 90),
                PlanBlock::new("Экстернат", 150),
            ],
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
    /// Kind of the day `now` falls on, by the weekly schedule.
    pub fn kind_on(&self, now: crate::clock::Ts) -> DayKind {
        self.week[crate::clock::weekday_index(now, self.tz_offset_min)]
    }

    /// Plan template for a kind of day (a started day off uses the full plan).
    pub fn plan_for(&self, kind: DayKind) -> &Vec<PlanBlock> {
        match kind {
            DayKind::Light => &self.light.plan,
            DayKind::Full | DayKind::Off => &self.plan_template,
        }
    }

    pub fn plan_for_mut(&mut self, kind: DayKind) -> &mut Vec<PlanBlock> {
        match kind {
            DayKind::Light => &mut self.light.plan,
            DayKind::Full | DayKind::Off => &mut self.plan_template,
        }
    }

    /// End of the study day (minutes after midnight) for a kind of day.
    pub fn day_end_for(&self, kind: DayKind) -> u32 {
        match kind {
            DayKind::Light => self.light.day_end_min,
            DayKind::Full | DayKind::Off => self.day_end_min,
        }
    }

    pub fn normalize(&mut self) {
        if let Some(old) = self.legacy_study_days.take() {
            self.week = old.map(|study| if study { DayKind::Full } else { DayKind::Off });
        }
        self.timing.work_segment_min = self.timing.work_segment_min.clamp(5, 240);
        self.timing.short_break_min = self.timing.short_break_min.clamp(1, 120);
        self.timing.between_blocks_min = self.timing.between_blocks_min.clamp(1, 180);
        self.timing.lunch_min = self.timing.lunch_min.clamp(5, 180);
        self.pause_access_min = self.pause_access_min.clamp(1, 60);
        self.emergency_min = self.emergency_min.clamp(1, 60);
        self.reminder_sec = self.reminder_sec.clamp(15, 600);
        self.day_end_min = self.day_end_min.min(24 * 60 - 1);
        // A light day is never stricter than a full one: raising light -> full only moves the end later.
        self.light.day_end_min = self.light.day_end_min.min(self.day_end_min);
        self.blocklist.sites = normalize_list(&self.blocklist.sites, normalize_site);
        self.blocklist.apps = normalize_list(&self.blocklist.apps, |s| {
            // Only a bare file name: "C:\x\Steam.exe" -> "Steam.exe".
            let s = s.trim().rsplit(['\\', '/']).next().unwrap_or("").trim();
            let exe = if s.is_empty() {
                return None;
            } else if s.to_ascii_lowercase().ends_with(".exe") {
                s.to_string()
            } else {
                format!("{s}.exe")
            };
            (!is_protected_app(&exe)).then_some(exe)
        });
        for plan in [&mut self.plan_template, &mut self.light.plan] {
            for b in plan.iter_mut() {
                b.normalize();
            }
            plan.retain(|b| b.minutes > 0);
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
        if new.week != self.week
            || new.day_end_min != self.day_end_min
            || new.light.day_end_min != self.light.day_end_min
            || new.tz_offset_min != self.tz_offset_min
        {
            return Err("Расписание учебных дней меняется только вне блокировки.".into());
        }
        if new.timing != self.timing {
            return Err("Длительности отрезков и перерывов меняются только вне блокировки.".into());
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

pub fn is_protected_app(exe: &str) -> bool {
    let e = exe.to_ascii_lowercase();
    PROTECTED_APPS.contains(&e.as_str())
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
    fn pause_access_only_outside_lock() {
        let cfg = Config::default();
        let mut n = cfg.clone();
        n.pause_access = true;
        assert!(cfg.check_update(&n, EditContext { locked: true }).is_err());
        assert!(cfg.check_update(&n, EditContext { locked: false }).is_ok());
    }
}
