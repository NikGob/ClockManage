//! Persistent user configuration (config.json).

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::day::PlanBlock;
use crate::i18n::{tr, LangPref};
use crate::tr;

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
                "2ch.su",
                "2ch.org",
                "2ch.life",
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
            DayKind::Full => tr("config_full"),
            DayKind::Light => tr("config_light"),
            DayKind::Off => tr("config_day_off"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DayProfile {
    /// Plan a new day of this kind starts with.
    pub plan: Vec<PlanBlock>,
    /// "Начать день" turns on blocking.
    pub block: bool,
    /// "Автоблокировка с …": the block list goes on at `autolock_min` when the day has not
    /// started by then, and holds until "Начать день". Configs from before 0.5.1 have none: on at
    /// 13:00. Never on a day off.
    pub autolock: bool,
    /// Minutes after midnight.
    pub autolock_min: u32,
}

impl Default for DayProfile {
    fn default() -> Self {
        Self { plan: vec![], block: false, autolock: true, autolock_min: DEFAULT_AUTOLOCK_MIN }
    }
}

/// "Автоблокировка с 13:00" unless the profile says otherwise.
pub const DEFAULT_AUTOLOCK_MIN: u32 = 13 * 60;

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
                    PlanBlock::new(tr("config_math"), 90),
                    PlanBlock::new(tr("config_languages"), 90),
                    PlanBlock::new(tr("config_project"), 150),
                ],
                block: true,
                ..Default::default()
            },
            light: DayProfile {
                plan: vec![PlanBlock::new(tr("config_math"), 60), PlanBlock::new(tr("config_languages"), 60)],
                block: true,
                ..Default::default()
            },
            off: DayProfile { autolock: false, ..Default::default() },
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
        Self { name: tr("config_segment").into(), minutes: 15, alarm: false, open_access: false, prep_min: None }
    }
}

pub fn default_segments(lunch_min: u32) -> Vec<SegmentType> {
    vec![
        SegmentType { name: tr("config_lunch").into(), minutes: lunch_min, ..Default::default() },
        SegmentType { name: tr("config_nap").into(), minutes: 20, alarm: true, prep_min: Some(DEFAULT_PREP_MIN), ..Default::default() },
        SegmentType { name: tr("config_walk").into(), minutes: 15, ..Default::default() },
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

/// A default text still untouched in another language: the same default in the current one.
fn default_in_other_lang(text: &str, keys: &[&'static str]) -> Option<&'static str> {
    let now = crate::i18n::get();
    keys.iter().find_map(|&key| {
        let other = crate::i18n::langs().any(|l| l != now && crate::i18n::tr_in(l, key) == text);
        let ours = crate::i18n::tr_in(now, key);
        (other && ours != text).then_some(ours)
    })
}

/// Rename the untouched defaults among `names` into the current language, unless the new name
/// is taken already (two segment types or blocks with one name would merge).
fn follow_lang_names<'a>(names: impl Iterator<Item = &'a mut String>, keys: &[&'static str]) -> bool {
    let mut names: Vec<&mut String> = names.collect();
    let mut changed = false;
    for i in 0..names.len() {
        let Some(to) = default_in_other_lang(names[i], keys) else { continue };
        if !names.iter().any(|n| n.to_lowercase() == to.to_lowercase()) {
            *names[i] = to.into();
            changed = true;
        }
    }
    changed
}

impl Config {
    /// The defaults (plan names, segment types, the emergency phrase, the diet) are written in the
    /// language of the first run. While the user hasn't changed them they follow the interface
    /// language: a later language switch, or a config from an older version, doesn't leave them
    /// in another language. Returns whether anything changed.
    pub fn follow_lang(&mut self) -> bool {
        const BLOCKS: [&str; 3] = ["config_math", "config_languages", "config_project"];
        const SEGMENTS: [&str; 3] = ["config_lunch", "config_nap", "config_walk"];
        let mut changed = false;
        for k in [DayKind::Full, DayKind::Light, DayKind::Off] {
            changed |= follow_lang_names(self.profiles.get_mut(k).plan.iter_mut().map(|b| &mut b.name), &BLOCKS);
        }
        changed |= follow_lang_names(self.segments.iter_mut().map(|t| &mut t.name), &SEGMENTS);
        if let Some(to) = default_in_other_lang(&self.emergency_phrase, &["config_i_am_consciously_interrupting_my_study"]) {
            self.emergency_phrase = to.into();
            changed = true;
        }
        if let Some(to) = default_in_other_lang(&self.meals.diet, &["meals_goal_a_clear_steady_mind"]) {
            self.meals.diet = to.into();
            changed = true;
        }
        changed
    }
}

pub fn default_phrase() -> &'static str {
    tr("config_i_am_consciously_interrupting_my_study")
}

/// Latest end of the night for `NightStart::until_min` (12:00).
pub const MAX_NIGHT_UNTIL_MIN: u32 = 12 * 60;
/// Longest hold of the night start button, seconds.
pub const MAX_NIGHT_HOLD_SEC: u32 = 10;

/// "Start day" at night: only in the app window and after holding the button, so a stray
/// click doesn't start a new day.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NightStart {
    pub enabled: bool,
    /// The night runs from midnight until this time (minutes after midnight; 0 = no such part).
    pub until_min: u32,
    /// After today's day end it is night too.
    pub after_day_end: bool,
    /// How long to hold the button, seconds.
    pub hold_sec: u32,
}

impl Default for NightStart {
    fn default() -> Self {
        Self { enabled: true, until_min: 5 * 60, after_day_end: true, hold_sec: 2 }
    }
}

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
    /// Open blocked things during the break between blocks (until the break timer runs out;
    /// a paused break stays blocked). Took the place of access on pause in 0.5.0, so an old
    /// config keeps its choice.
    #[serde(alias = "pause_access")]
    pub break_access: bool,
    pub emergency_phrase: String,
    pub emergency_min: u32,
    /// Loud reminder period while the timer waits for "start next part".
    pub reminder_sec: u32,
    pub sound: bool,
    pub overlay: bool,
    /// Restart a running Firefox when path rules (Shorts) start to apply.
    pub restart_firefox: bool,
    pub autostart: bool,
    /// Check GitHub Releases for a new version, download it and install it while no day runs.
    pub auto_update: bool,
    pub mcp_enabled: bool,
    pub mcp_port: u16,
    pub appearance: Appearance,
    /// Interface language; "auto" follows the Windows UI language.
    pub lang: LangPref,
    /// Types of non-study segments ("Отрезок" button and `type: "break"` plan items).
    /// Missing in an old config.json -> empty, and `normalize` fills it keeping the old lunch length.
    #[serde(default = "Vec::new")]
    pub segments: Vec<SegmentType>,
    pub phone: Phone,
    pub night_start: NightStart,
    /// Plans set ahead for particular dates (MCP `set_plan` with `date`): that day starts with
    /// it instead of its profile's template. Dropped once that date's day exists.
    pub plan_overrides: BTreeMap<NaiveDate, Vec<PlanBlock>>,
    /// Diet, address, meal times and the paired iPhone web app.
    pub meals: crate::meals::Meals,
    /// Before profiles (0.1.x): study day flags, migrated into `week` by `normalize`.
    #[serde(skip_serializing)]
    pub study_days: Option<[bool; 7]>,
    /// Before profiles (0.1.x): the single plan template, migrated into the full profile.
    #[serde(skip_serializing)]
    pub plan_template: Option<Vec<PlanBlock>>,
}

/// Tests: the defaults with a study week, Mon–Fri Full and Sat–Sun off.
#[cfg(test)]
impl Config {
    pub(crate) fn study_week() -> Self {
        let mut c = Self::default();
        c.week = [DayKind::Full, DayKind::Full, DayKind::Full, DayKind::Full, DayKind::Full, DayKind::Off, DayKind::Off];
        c
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // A new user picks their study days themselves.
            week: [DayKind::Off; 7],
            profiles: Profiles::default(),
            day_end_min: 22 * 60,
            tz_offset_min: 180,
            timing: Timing::default(),
            blocklist: BlockList::default(),
            break_access: false,
            emergency_phrase: default_phrase().into(),
            emergency_min: 10,
            reminder_sec: 60,
            sound: true,
            overlay: true,
            restart_firefox: true,
            autostart: true,
            auto_update: true,
            mcp_enabled: true,
            mcp_port: 0,
            appearance: Appearance::default(),
            lang: LangPref::Auto,
            segments: default_segments(45),
            phone: Phone::default(),
            night_start: NightStart::default(),
            plan_overrides: BTreeMap::new(),
            meals: crate::meals::Meals::default(),
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

    /// "Автоблокировка с …" of a profile, minutes after midnight; `None`: off (always on a day off).
    pub fn autolock_min(&self, k: DayKind) -> Option<u32> {
        let p = self.profile(k);
        (k != DayKind::Off && p.autolock).then_some(p.autolock_min)
    }

    /// The segment type called `name` (case-insensitive).
    pub fn segment_type(&self, name: &str) -> Option<&SegmentType> {
        let n = name.trim();
        self.segments.iter().find(|t| t.name.eq_ignore_ascii_case(n) || t.name.to_lowercase() == n.to_lowercase())
    }

    pub fn normalize(&mut self) {
        self.meals.normalize();
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
        self.emergency_min = self.emergency_min.clamp(1, 60);
        self.reminder_sec = self.reminder_sec.clamp(15, 600);
        self.day_end_min = self.day_end_min.min(24 * 60 - 1);
        self.night_start.until_min = self.night_start.until_min.min(MAX_NIGHT_UNTIL_MIN);
        self.night_start.hold_sec = self.night_start.hold_sec.clamp(1, MAX_NIGHT_HOLD_SEC);
        for k in DayKind::ALL {
            let p = self.profiles.get_mut(k);
            p.autolock_min = p.autolock_min.min(24 * 60 - 1);
        }
        self.profiles.off.autolock = false;
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
            self.emergency_phrase = default_phrase().into();
        }
    }

    /// Validate a config change against the lock rules. Returns a human readable refusal.
    pub fn check_update(&self, new: &Config, ctx: EditContext) -> Result<(), String> {
        if !ctx.locked {
            return Ok(());
        }
        if new.break_access && !self.break_access {
            return Err(tr!(
                "config_access_during_the_break_between_blocks",
                day_end_min = fmt_hm(self.day_end_min),
            ));
        }
        let lower = |v: &[String]| v.iter().map(|s| s.to_ascii_lowercase()).collect::<Vec<_>>();
        let new_sites = lower(&new.blocklist.sites);
        if lower(&self.blocklist.sites).iter().any(|s| !new_sites.contains(s)) {
            return Err(tr!("config_while_the_lock_is_on_sites"));
        }
        let new_apps = lower(&new.blocklist.apps);
        if lower(&self.blocklist.apps).iter().any(|s| !new_apps.contains(s)) {
            return Err(tr!("config_while_the_lock_is_on_apps"));
        }
        let new_phone = lower(&new.phone.apps);
        if lower(&self.phone.apps).iter().any(|s| !new_phone.contains(s)) {
            return Err(tr!("config_while_the_lock_is_on_phone"));
        }
        // Time settings are templates and change freely: today keeps its work parts and breaks
        // (the day state holds them), and a day end moved earlier waits for tomorrow
        // (`Day::keep_today`). Only what opens something earlier or for longer is refused.
        if new.tz_offset_min != self.tz_offset_min {
            return Err(tr!("config_the_time_zone_can_only_be"));
        }
        for t in &new.segments {
            match self.segment_type(&t.name) {
                // A segment that opens access would open it for longer.
                Some(old) if old.open_access && (t.minutes > old.minutes || t.prep() > old.prep()) => {
                    return Err(tr!("config_during_the_study_day_name_opens", name = old.name));
                }
                Some(old) if t.open_access && !old.open_access => {
                    return Err(tr!("config_access_during_a_segment_can_only"));
                }
                None if t.open_access => return Err(tr!("config_access_during_a_segment_can_only")),
                _ => {}
            }
        }
        if new.emergency_min > self.emergency_min || new.emergency_phrase != self.emergency_phrase {
            return Err(tr!("config_emergency_access_can_only_be_configured"));
        }
        if !new.autostart && self.autostart {
            return Err(tr!("config_autostart_cant_be_turned_off_during"));
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

/// Processes that must never be blocked: Windows itself, the WebView runtime, ClockManage, and
/// the Linux session (on Linux `gnome-shell.exe` matches the process `gnome-shell`).
/// Blocking any of them would break the system (or the app) for the whole study day.
const PROTECTED_APPS: [&str; 37] = [
    "explorer.exe", "svchost.exe", "csrss.exe", "wininit.exe", "winlogon.exe", "lsass.exe",
    "services.exe", "smss.exe", "dwm.exe", "fontdrvhost.exe", "sihost.exe", "ctfmon.exe",
    "taskhostw.exe", "runtimebroker.exe", "searchhost.exe", "startmenuexperiencehost.exe",
    "shellexperiencehost.exe", "conhost.exe", "msedgewebview2.exe", "clockmanage.exe",
    "cm-guard.exe", "system",
    "systemd.exe", "init.exe", "dbus-daemon.exe", "dbus-broker.exe", "xorg.exe", "xwayland.exe",
    "gnome-shell.exe", "gnome-session-binary.exe", "plasmashell.exe", "kwin_wayland.exe",
    "kwin_x11.exe", "ksmserver.exe", "pipewire.exe", "wireplumber.exe", "webkitwebprocess.exe",
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
    fn locked_time_settings_change_freely() {
        let cfg = Config::default();
        let ctx = EditContext { locked: true };
        // Today keeps its own; these shape the days ahead.
        let mut n = cfg.clone();
        n.day_end_min = 21 * 60;
        n.timing.short_break_min = 25;
        n.timing.between_blocks_min = 40;
        n.timing.work_segment_min = 30;
        n.week[2] = DayKind::Light;
        assert!(cfg.check_update(&n, ctx).is_ok());
        // The time zone moves every clock of today.
        let mut n = cfg.clone();
        n.tz_offset_min += 60;
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
        // A segment without access keeps everything closed: it may get longer.
        let mut n = c.clone();
        n.segments[1].minutes = 90;
        n.segments[1].prep_min = Some(20);
        assert!(c.check_update(&n, ctx).is_ok());
        let mut n = c.clone();
        n.segments[0].open_access = true;
        assert!(c.check_update(&n, ctx).is_err());
        assert!(c.check_update(&n, EditContext { locked: false }).is_ok());
        // One with access only gets shorter.
        let open = n;
        let mut n = open.clone();
        n.segments[0].minutes += 15;
        assert!(open.check_update(&n, ctx).unwrap_err().contains(&open.segments[0].name));
        n.segments[0].minutes = 10;
        assert!(open.check_update(&n, ctx).is_ok());
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
    fn break_access_turns_on_only_outside_lock() {
        let cfg = Config::default();
        let mut n = cfg.clone();
        n.break_access = true;
        assert!(cfg.check_update(&n, EditContext { locked: true }).is_err());
        assert!(cfg.check_update(&n, EditContext { locked: false }).is_ok());
        // Turning it off only makes the lock stricter.
        assert!(n.check_update(&cfg, EditContext { locked: true }).is_ok());
    }

    #[test]
    fn autolock_defaults_to_13_and_never_on_a_day_off() {
        let c = Config::default();
        assert_eq!(c.autolock_min(DayKind::Full), Some(13 * 60));
        assert_eq!(c.autolock_min(DayKind::Light), Some(13 * 60));
        assert_eq!(c.autolock_min(DayKind::Off), None);
        // A config from before the setting: on at 13:00 for study profiles, off for a day off.
        let json = r#"{"profiles":{"full":{"plan":[],"block":true},"light":{"block":true},"off":{"block":false}}}"#;
        let mut old: Config = serde_json::from_str(json).unwrap();
        old.normalize();
        assert_eq!(old.autolock_min(DayKind::Full), Some(13 * 60));
        assert_eq!(old.autolock_min(DayKind::Light), Some(13 * 60));
        assert!(!old.profiles.off.autolock);
        let mut c = c;
        c.profiles.light.autolock = false;
        c.profiles.full.autolock_min = 99 * 60;
        c.profiles.off.autolock = true;
        c.normalize();
        assert_eq!(c.autolock_min(DayKind::Light), None);
        assert_eq!(c.autolock_min(DayKind::Full), Some(24 * 60 - 1));
        assert_eq!(c.autolock_min(DayKind::Off), None);
    }

    #[test]
    fn old_pause_access_carries_over() {
        let c: Config = serde_json::from_str(r#"{"pause_access": true, "pause_access_min": 15}"#).unwrap();
        assert!(c.break_access);
    }
}
