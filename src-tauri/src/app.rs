//! Shared state, the ticker loop, reactions to timer events and Tauri commands.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use clockmanage_core::clock::{self, Ts, MIN};
use clockmanage_core::chrono::{self, Datelike, NaiveDate};
use clockmanage_core::config::{DayKind, EditContext, SchemeVariant, ThemeMode, MAX_PLAN_AHEAD_DAYS, WEEKDAYS};
use clockmanage_core::day::{fmt_change, fmt_day_min, fmt_min, same_name, worked_min, Event, Forecast, PlanBlock, QueuedSegment, SingleCfg};
use clockmanage_core::stats::{self, DayStats};
use clockmanage_core::view::{self, View};
use clockmanage_core::mcp::{McpHost, PlanRequest};
use clockmanage_core::{Config, DayState};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::menu::MenuItem;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, Wry};
use tauri_plugin_notification::NotificationExt;

use crate::blocker::Blocker;
use crate::mcp_server::{McpServer, McpStatus};
use crate::phone_server::{PhoneServer, PhoneStatus};
use crate::sound::{self, Sound};
use crate::store::Store;
use crate::system;

pub struct Captcha {
    id: u64,
    answers: Vec<i64>,
    created: Ts,
}

pub const CAPTCHA_WAIT_MS: i64 = 15_000;
/// The agent's "finish block" confirmation lives this long.
const FINISH_TOKEN_MS: i64 = 120_000;

pub struct FinishToken {
    token: String,
    block: String,
    until: Ts,
}

pub struct Inner {
    pub cfg: Config,
    pub day: DayState,
    pub captcha: Option<Captcha>,
    pub force_emit: bool,
    pub last_emit_sec: i64,
    pub last_save: Ts,
    pub last_kill_note: Ts,
}

#[derive(Clone)]
pub struct TrayItems {
    pub action: MenuItem<Wry>,
    pub quit: MenuItem<Wry>,
}

pub struct Shared {
    pub inner: Mutex<Inner>,
    pub store: Store,
    pub blocker: Mutex<Blocker>,
    pub mcp: McpServer,
    pub phone: PhoneServer,
    pub app: AppHandle,
    pub admin: bool,
    pub overlay: Mutex<Option<Value>>,
    pub tray: Mutex<Option<TrayItems>>,
    /// Monitor rect the overlay was last stretched over (resizing a WebView is slow: only on change).
    pub overlay_rect: Mutex<Option<(i32, i32, u32, u32)>>,
    pub finish_token: Mutex<Option<FinishToken>>,
    /// Bundled Android APK + apk.json (version), served to paired phones for self-update.
    pub apk_dir: Option<std::path::PathBuf>,
}

#[derive(Serialize, Clone)]
pub struct Meta {
    pub admin: bool,
    pub version: &'static str,
    pub mcp: McpStatus,
    pub phone: PhoneStatus,
    pub blocker_error: Option<String>,
    pub blocking_applied: bool,
    pub sound: bool,
    pub overlay: bool,
    pub pause_access: bool,
    pub pause_access_min: u32,
    pub emergency_min: u32,
    pub lunch_min: u32,
    /// Segment types, so the "Отрезок" picker opens without a round trip.
    pub segments: Vec<clockmanage_core::config::SegmentType>,
    pub seed: String,
    pub theme_mode: ThemeMode,
    pub variant: SchemeVariant,
    pub mini_contrast: bool,
    pub data_dir: String,
    pub tz_offset_min: i32,
}

#[derive(Serialize, Clone)]
pub struct Snapshot {
    pub view: View,
    pub meta: Meta,
}

impl Shared {
    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn snapshot_locked(&self, g: &Inner, now: Ts) -> Snapshot {
        let blocker = self.blocker.lock().unwrap_or_else(|e| e.into_inner());
        Snapshot {
            view: view::build(&g.day, &g.cfg, now),
            meta: Meta {
                admin: self.admin,
                version: env!("CARGO_PKG_VERSION"),
                mcp: self.mcp.status.lock().unwrap().clone(),
                phone: self.phone.status(&g.cfg, self.apk_dir.as_deref()),
                blocker_error: blocker.last_error.clone(),
                blocking_applied: blocker.applied.active,
                sound: g.cfg.sound,
                overlay: g.cfg.overlay,
                pause_access: g.cfg.pause_access,
                pause_access_min: g.cfg.pause_access_min,
                emergency_min: g.cfg.emergency_min,
                lunch_min: g.day.timing.lunch_min,
                segments: g.cfg.segments.clone(),
                seed: g.cfg.appearance.seed.clone(),
                theme_mode: g.cfg.appearance.mode,
                variant: g.cfg.appearance.variant,
                mini_contrast: g.cfg.appearance.mini_contrast,
                data_dir: self.store.dir.display().to_string(),
                tz_offset_min: g.cfg.tz_offset_min,
            },
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let g = self.lock();
        self.snapshot_locked(&g, clock::now_ts())
    }

    fn save_day(&self, g: &mut Inner, now: Ts) {
        g.day.saved_at = now;
        g.last_save = now;
        self.store.save_day(&g.day);
    }

    /// Run a state change, persist and broadcast it.
    pub fn mutate<T>(&self, f: impl FnOnce(&mut Inner, Ts) -> Result<T, String>) -> Result<T, String> {
        let now = clock::now_ts();
        let (res, snap) = {
            let mut g = self.lock();
            let res = f(&mut g, now);
            if res.is_ok() {
                self.save_day(&mut g, now);
            }
            (res, self.snapshot_locked(&g, now))
        };
        let _ = self.app.emit("state", &snap);
        self.update_tray(&snap.view);
        res
    }

    fn notify(&self, title: &str, body: &str) {
        let _ = self.app.notification().builder().title(title).body(body).show();
    }

    fn play(&self, s: Sound) {
        if self.lock().cfg.sound {
            sound::play(s);
        }
    }

    pub fn show_overlay(&self, payload: Value) {
        // The nap alarm shows even with cards turned off: it is the alarm clock.
        let forced = payload.get("force").and_then(Value::as_bool).unwrap_or(false);
        if !self.lock().cfg.overlay && !forced {
            return;
        }
        let passive = payload.get("passive").and_then(Value::as_bool).unwrap_or(false);
        *self.overlay.lock().unwrap() = Some(payload.clone());
        let Some(w) = self.app.get_webview_window("overlay") else { return };
        if let Ok(Some(m)) = w.current_monitor().or_else(|_| w.primary_monitor()) {
            let rect = (m.position().x, m.position().y, m.size().width, m.size().height);
            let mut last = self.overlay_rect.lock().unwrap_or_else(|e| e.into_inner());
            if *last != Some(rect) {
                let _ = w.set_position(PhysicalPosition::new(rect.0, rect.1));
                let _ = w.set_size(PhysicalSize::new(rect.2, rect.3));
                *last = Some(rect);
            }
        }
        // Passive notices never steal focus or clicks from whatever the user is doing.
        let _ = w.set_focusable(!passive);
        let _ = w.set_ignore_cursor_events(passive);
        let _ = w.set_always_on_top(true);
        // Visible first, then the content: a hidden WebView2 holds animation frames back, so
        // rendering into it first made the card appear late.
        let _ = w.show();
        let _ = self.app.emit_to("overlay", "overlay", &payload);
        if !passive {
            let _ = w.set_focus();
        }
    }

    /// End of a nap: the overlay that only "Встал" closes (the ticker rings meanwhile).
    fn wake_overlay(&self, name: &str, over_ms: i64) {
        self.show_overlay(json!({
            "kind": "wake", "passive": false, "force": true,
            "title": "Вставай!",
            "text": if over_ms > 0 { format!("{name} окончен {} назад", fmt_dur(over_ms)) } else { format!("{name} окончен") },
            "action": "Встал",
        }));
    }

    /// Getting ready for a nap is over: "Лёг" starts the countdown (nothing starts by itself).
    fn prep_overlay(&self, name: &str, over_ms: i64) {
        self.show_overlay(json!({
            "kind": "prep", "passive": false,
            "title": "Пора ложиться",
            "text": if over_ms > 0 {
                format!("Подготовка ко сну идёт уже на {} дольше. {name} начнётся, когда нажмёшь «Лёг».", fmt_dur(over_ms))
            } else {
                format!("Подготовка окончена. {name} и будильник пойдут от «Лёг».")
            },
            "action": "Лёг",
        }));
    }

    /// End of another segment: "Закончил" or keep going.
    fn segment_overlay(&self, name: &str, over_ms: i64, v: &View) {
        self.show_overlay(json!({
            "kind": "segment", "passive": false,
            "title": if over_ms > 0 { format!("{name}: +{}", fmt_dur(over_ms)) } else { format!("{name} окончен") },
            "text": if v.phase.queue.is_empty() { v.phase.subtitle.clone() } else { format!("Потом: {}", v.phase.queue.join(" → ")) },
            "action": format!("Закончил {}", name.to_lowercase()),
        }));
    }

    /// Playful "no-no-no" finger wag over the screen when something blocked is opened.
    pub fn nope(&self, text: &str) {
        self.play(Sound::Nope);
        self.show_overlay(json!({
            "kind": "nope", "passive": true, "auto_hide_ms": 3000,
            "title": "Не-не-не", "text": text,
        }));
    }

    /// Something was changed by the agent through MCP or from the phone, not in this window:
    /// say so in the UI.
    pub fn notice(&self, title: &str, text: &str) {
        let _ = self.app.emit("agent", json!({ "title": title, "text": text }));
        self.notify(title, text);
    }

    pub fn update_tray(&self, v: &View) {
        // Menu setters called off the main thread wait for the main thread. Sync commands run
        // on the main thread and call this too, so no lock may be held across the setters:
        // the ticker holding `tray` while waiting for the main thread, and a command waiting
        // for `tray` on the main thread, froze the whole app (a click on "Обед" just hung).
        let Some(items) = self.tray.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return };
        let (label, enabled) = if v.can.resume {
            ("Продолжить", true)
        } else if v.can.pause {
            ("Пауза", true)
        } else if v.phase.kind == "await" || v.phase.kind == "lunch" {
            ("Начать следующую часть", true)
        } else if v.can.lay_down {
            ("Лёг", true)
        } else if v.can.end_segment {
            (if v.phase.alarm { "Встал" } else { "Закончить отрезок" }, true)
        } else if v.can.start_day {
            ("Начать день", true)
        } else {
            ("Пауза", false)
        };
        let _ = items.action.set_text(label);
        let _ = items.action.set_enabled(enabled);
        let _ = items.quit.set_enabled(!v.lock.base);
        let _ = items.quit.set_text(if v.lock.base { "Выход недоступен во время учёбы" } else { "Выход" });
        if let Some(t) = self.app.tray_by_id("main") {
            let _ = t.set_tooltip(Some(tray_tooltip(v)));
        }
    }

    fn react(&self, events: &[Event], snap: &Snapshot) {
        let v = &snap.view;
        for e in events {
            match e {
                Event::WorkEnded { block_done: false, block_name, part, parts } => {
                    self.play(Sound::BreakStart);
                    let brk = if v.phase.kind == "break" { v.phase.dur_ms / MIN } else { 0 };
                    let body = if *parts > 0 {
                        format!("{block_name}: часть {part} из {parts} готова. Перерыв {brk} мин.")
                    } else {
                        format!("Круг {part} готов. Перерыв {brk} мин.")
                    };
                    self.notify("Перерыв", &body);
                    self.show_overlay(json!({
                        "kind": "break", "passive": true, "auto_hide_ms": 5200,
                        "title": "Перерыв", "text": body,
                    }));
                }
                Event::WorkEnded { block_done: true, .. } => {}
                Event::BlockCompleted { block, block_name, work_ms, pauses, pause_ms } => {
                    self.play(Sound::Done);
                    let facts = format!(
                        "{} работы · пауз: {}{}",
                        fmt_dur(*work_ms),
                        pauses,
                        if *pauses > 0 { format!(" ({})", fmt_dur(*pause_ms)) } else { String::new() }
                    );
                    self.notify(&format!("Блок «{block_name}» закрыт"), "Что было скучно, куда отвлекался? Одна строка — в окне ClockManage или агенту.");
                    self.show_overlay(json!({
                        "kind": "block", "passive": false,
                        "title": format!("«{block_name}» закрыт"),
                        "text": facts,
                        "ask_note": true, "block": block,
                    }));
                }
                Event::DayCompleted { work_ms } => {
                    self.notify("День закрыт", &format!("{} учёбы. Блокировка снята.", fmt_dur(*work_ms)));
                    self.show_overlay(json!({
                        "kind": "day", "passive": false,
                        "title": "День закрыт",
                        "text": format!("{} учёбы. Блокировка снята.", fmt_dur(*work_ms)),
                    }));
                }
                Event::BreakEnded { next_name, next_part, next_parts, lunch } => {
                    self.play(Sound::Alarm);
                    let action = start_label(next_name, *next_part, *next_parts);
                    self.notify(if *lunch { "Обед окончен" } else { "Перерыв окончен" }, &format!("Жми «{action}». Блокировка не снята."));
                    self.show_overlay(json!({
                        "kind": "await", "passive": false,
                        "title": if *lunch { "Обед окончен" } else { "Перерыв окончен" },
                        "text": if *next_parts > 0 { format!("{next_name} · часть {next_part} из {next_parts}") } else { format!("Круг {next_part}") },
                        "action": action,
                    }));
                }
                Event::AwaitReminder { waiting_ms, next_name, next_part, next_parts } => {
                    self.play(Sound::Alarm);
                    let action = start_label(next_name, *next_part, *next_parts);
                    self.show_overlay(json!({
                        "kind": "await", "passive": false,
                        "title": format!("Ждём уже {}", fmt_dur(*waiting_ms)),
                        "text": if *next_parts > 0 { format!("{next_name} · часть {next_part} из {next_parts}") } else { format!("Круг {next_part}") },
                        "action": action,
                    }));
                }
                Event::PauseReminder { paused_ms } => {
                    self.play(Sound::Ping);
                    self.notify("Ты на паузе", &format!("Уже {}. Таймер ждёт тебя.", fmt_dur(*paused_ms)));
                }
                Event::PauseAccessWarning { left_ms } => {
                    self.play(Sound::Ping);
                    self.notify("Доступ скоро закроется", &format!("Осталось {}. Потом блокировка вернётся.", fmt_dur(*left_ms)));
                }
                Event::PauseAccessExpired => {
                    self.play(Sound::Ping);
                    self.notify("Доступ закрыт", "Блокировка вернулась. Продлить — в приложении, через мини-капчу.");
                    self.show_overlay(json!({
                        "kind": "access", "passive": true, "auto_hide_ms": 4200,
                        "title": "Доступ закрыт", "text": "Блокировка снова включена",
                    }));
                }
                Event::EmergencyEnded => {
                    self.play(Sound::Ping);
                    self.notify("Аварийный доступ закончился", "Блокировка снова включена.");
                }
                Event::DayEndReached => {
                    self.play(Sound::Done);
                    self.notify(&format!("{} — блокировка снята", v.day_end), "Учебный день закончился по времени.");
                }
                Event::SegmentWarning { name, left_ms } => {
                    self.play(Sound::Ping);
                    let text = format!("{name}: осталось {}", fmt_dur(*left_ms));
                    self.notify("Скоро конец", &text);
                    self.show_overlay(json!({ "kind": "break", "passive": true, "auto_hide_ms": 4200, "title": "Скоро конец", "text": text }));
                }
                Event::SegmentEnded { name, alarm: true } => {
                    // The nap alarm: the ticker keeps ringing until "Встал".
                    self.notify(&format!("{name} окончен — вставай!"), "Будильник звонит, пока не нажмёшь «Встал».");
                    self.wake_overlay(name, 0);
                }
                Event::SegmentEnded { name, alarm: false } => {
                    self.play(Sound::Alarm);
                    self.notify(&format!("{name} окончен"), "Нажми «Закончил», когда вернёшься. Напомню каждые 5 минут.");
                    self.segment_overlay(name, 0, &snap.view);
                }
                Event::SegmentOverrun { name, alarm, over_ms } => {
                    let title = format!("{name}: превышено на {}", fmt_dur(*over_ms));
                    if *alarm {
                        self.notify(&title, "Вставай — будильник не замолчит, пока не нажмёшь «Встал».");
                        self.wake_overlay(name, *over_ms);
                    } else {
                        self.play(Sound::Alarm);
                        self.notify(&title, "Нажми «Закончил», когда вернёшься.");
                        self.segment_overlay(name, *over_ms, &snap.view);
                    }
                }
                Event::SegmentPrepOver { name, over_ms } => {
                    self.play(Sound::Ping);
                    self.notify("Пора ложиться", &format!("Подготовка окончена — нажми «Лёг», и пойдёт отсчёт: {}.", name.to_lowercase()));
                    self.prep_overlay(name, *over_ms);
                }
                Event::SegmentLong { name, elapsed_ms } => {
                    // A stopwatch segment: no alarm, just a quiet word that it is still on.
                    self.notify(&format!("{name} идёт уже {}", fmt_dur(*elapsed_ms)), "Без времени. Нажми «Закончил», когда вернёшься.");
                }
                Event::AskWhatNow => {
                    self.play(Sound::Ping);
                    let types = self.lock().cfg.segments.iter().map(|t| json!({ "name": t.name, "minutes": t.minutes })).collect::<Vec<_>>();
                    self.notify("Что сейчас?", "Блок закрыт 10 минут назад, а дальше ничего не началось. Обед, сон, перерыв?");
                    self.show_overlay(json!({
                        "kind": "ask", "passive": false, "title": "Что сейчас?",
                        "text": "Блок закрыт 10 минут назад. Запусти таймер того, чем занят:",
                        "types": types,
                    }));
                }
            }
        }
    }
}

fn start_label(name: &str, part: u32, parts: u32) -> String {
    if parts == 0 {
        format!("Начать круг {part}")
    } else if part == 1 {
        format!("Начать «{name}»")
    } else {
        format!("Начать часть {part}")
    }
}

/// "DiscordPTB.exe" -> "Discord", "steam.exe" -> "Steam". One app may run several processes.
pub fn app_display_name(exe: &str) -> String {
    let base = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    let stem = base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base);
    let lower = stem.to_lowercase();
    let known = [
        ("telegram", "Telegram"),
        ("ayugram", "AyuGram"),
        ("discord", "Discord"),
        ("steam", "Steam"),
        ("twitch", "Twitch"),
        ("spotify", "Spotify"),
        ("epicgameslauncher", "Epic Games"),
        ("riotclient", "Riot"),
    ];
    if let Some((_, name)) = known.iter().find(|(k, _)| lower.starts_with(k)) {
        return name.to_string();
    }
    let mut c = stem.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => stem.to_string(),
    }
}

pub fn fmt_dur(ms: i64) -> String {
    let m = (ms.max(0) + 30_000) / MIN;
    match (m / 60, m % 60) {
        (0, m) => format!("{m} мин"),
        (h, 0) => format!("{h} ч"),
        (h, m) => format!("{h} ч {m} мин"),
    }
}

fn mmss(ms: i64) -> String {
    let s = (ms.max(0) + 999) / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

fn tray_tooltip(v: &View) -> String {
    let p = &v.phase;
    let head = match p.kind.as_str() {
        "work" | "break" | "lunch_break" => {
            format!("{} — {}{}", p.title, mmss(p.remaining_ms), if p.paused { " (пауза)" } else { "" })
        }
        "segment" if p.prep => format!("{} — подготовка, {}", p.title, if p.remaining_ms < 0 { "пора ложиться".into() } else { mmss(p.remaining_ms) }),
        "segment" if p.stopwatch => format!("{} — без времени, {}", p.title, mmss(p.elapsed_ms)),
        "segment" if p.remaining_ms < 0 => format!("{} — превышено на {}", p.title, mmss(-p.remaining_ms)),
        "segment" => format!("{} — {}", p.title, mmss(p.remaining_ms)),
        _ => p.title.clone(),
    };
    let lock = if v.lock.blocked { "блокировка включена" } else if v.lock.base { "доступ временно открыт" } else { "без блокировки" };
    format!("ClockManage\n{head}\n{lock}")
}

// ---------------- day rollover & ticker ----------------

fn today_str(now: Ts, cfg: &Config) -> String {
    clock::local_date(now, cfg.tz_offset_min).format("%Y-%m-%d").to_string()
}

/// Load (or create) today's day, closing a day left open from an earlier date.
pub fn load_today(store: &Store, cfg: &Config, now: Ts) -> DayState {
    let today = today_str(now, cfg);
    if let Some(mut old) = store.last_other_day(&today) {
        // Yesterday's day end was moved past midnight and is still ahead: it is still "today".
        if old.is_live(now, cfg) {
            old.restore(now, cfg);
            return old;
        }
        if old.pause.is_some() || !matches!(old.phase, clockmanage_core::day::Phase::Idle | clockmanage_core::day::Phase::Done) {
            let end = old.saved_at;
            old.finalize(end);
            store.save_day(&old);
        }
    }
    match store.load_day(&today) {
        Some(mut d) => {
            d.restore(now, cfg);
            d
        }
        None => DayState::new(now, cfg),
    }
}

pub fn spawn_ticker(shared: Arc<Shared>) {
    std::thread::Builder::new()
        .name("ticker".into())
        .spawn(move || ticker(shared))
        .expect("spawn ticker");
}

fn ticker(shared: Arc<Shared>) {
    let mut last_blocked: Option<bool> = None;
    let mut last_verify: Ts = 0;
    let mut last_kill: Ts = 0;
    let mut last_tray: Ts = 0;
    let mut last_nope: Ts = 0;
    let mut last_tab_check: Ts = 0;
    let mut last_tab_hit: Option<(String, Ts)> = None;
    let mut last_beat: Ts = 0;
    let mut last_beat_blocked = false;
    let mut last_attempts: Ts = 0;
    let mut last_ring: Ts = 0;
    loop {
        std::thread::sleep(Duration::from_millis(200));
        let now = clock::now_ts();
        let (events, snap, emit, blocked, sites, apps, killed_note, restart_ff) = {
            let mut g = shared.lock();
            let g = &mut *g;
            if !g.day.is_live(now, &g.cfg) {
                g.day.finalize(now);
                shared.store.save_day(&g.day);
                g.day = DayState::new(now, &g.cfg);
                if g.cfg.drop_overrides_through(g.day.date) {
                    shared.store.save_config(&g.cfg);
                }
                g.force_emit = true;
            }
            let events = g.day.tick(now, &g.cfg);
            if !events.is_empty() || now - g.last_save > 10_000 {
                shared.save_day(g, now);
            }
            let sec = now / 1000;
            let emit = g.force_emit || !events.is_empty() || sec != g.last_emit_sec;
            g.force_emit = false;
            g.last_emit_sec = sec;
            let snap = shared.snapshot_locked(g, now);
            let killed_note = now - g.last_kill_note > 20_000;
            let blocked = snap.view.lock.blocked;
            (events, snap, emit, blocked, g.cfg.blocklist.sites.clone(), g.cfg.blocklist.apps.clone(), killed_note, g.cfg.restart_firefox)
        };

        if !events.is_empty() {
            shared.react(&events, &snap);
        }
        // Nap over: the alarm rings back to back until "Встал" — even with sounds turned off,
        // it is the alarm clock.
        let p = &snap.view.phase;
        if p.kind == "segment" && p.alarm && !p.prep && !p.stopwatch && p.remaining_ms <= 0 && now - last_ring >= 3_400 {
            last_ring = now;
            sound::play(Sound::Alarm);
        }

        // Enforcement: apply on change, verify/repair every 30 s.
        if last_blocked != Some(blocked) || now - last_verify > 30_000 {
            let note = shared
                .blocker
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .sync(blocked.then_some(sites.as_slice()), last_blocked.is_none(), restart_ff);
            if let Some(n) = note {
                shared.lock().day.log(now, "firefox", n.clone());
                shared.notify("Firefox", &n);
            }
            let restart_ff_now = {
                let mut b = shared.blocker.lock().unwrap_or_else(|e| e.into_inner());
                b.sync_launch_guard(blocked.then_some(apps.as_slice()), last_blocked == Some(blocked));
                b.take_firefox_restart()
            };
            if restart_ff_now {
                // Takes seconds (waits for Firefox to close and come back): never on this thread.
                let s = shared.clone();
                std::thread::spawn(move || {
                    let text = match crate::blocker::restart_firefox() {
                        Ok(()) => "Firefox перезапущен, чтобы блокировка в нём заработала. Вкладки восстановятся.".to_string(),
                        Err(e) => format!("Не удалось перезапустить Firefox ({e}) — перезапусти его вручную."),
                    };
                    s.lock().day.log(clock::now_ts(), "firefox", text.clone());
                    s.notify("Firefox", &text);
                });
            }
            last_verify = now;
            if last_blocked.is_some() && last_blocked != Some(blocked) {
                let mut g = shared.lock();
                let text = if blocked { "Блокировка включена".to_string() } else { format!("Блокировка снята ({})", snap.view.lock.reason) };
                g.day.log(now, "lock", text);
                g.force_emit = true;
            }
            last_blocked = Some(blocked);
        }
        if blocked && now - last_kill >= 1_000 {
            last_kill = now;
            let killed = shared.blocker.lock().unwrap_or_else(|e| e.into_inner()).kill_apps(&apps);
            if !killed.is_empty() {
                let mut names: Vec<String> = vec![];
                for k in &killed {
                    let n = app_display_name(k);
                    if !names.contains(&n) {
                        names.push(n);
                    }
                }
                if now - last_nope > 3_000 {
                    last_nope = now;
                    shared.nope(&format!("{} — после учёбы", names.join(", ")));
                }
                let mut g = shared.lock();
                g.day.log(now, "app_killed", format!("Закрыто: {}", killed.join(", ")));
                if killed_note {
                    g.last_kill_note = now;
                    drop(g);
                    shared.notify("Сейчас учёба", &format!("{} закрыт. Блокировка до конца блоков дня.", killed.join(", ")));
                }
            }
        }

        // Launch guard: heartbeat tells it the lock is live; it reports stopped launches back.
        if now - last_beat >= 4_000 || last_beat_blocked != blocked {
            last_beat = now;
            last_beat_blocked = blocked;
            let beat = shared.store.dir.join("guard.beat");
            if blocked {
                let _ = std::fs::write(&beat, now.to_string());
            } else {
                let _ = std::fs::remove_file(&beat);
            }
        }
        if now - last_attempts >= 400 {
            last_attempts = now;
            let dir = shared.store.dir.join("attempts");
            if let Ok(rd) = std::fs::read_dir(&dir) {
                let mut names: Vec<String> = vec![];
                for e in rd.flatten() {
                    let exe = std::fs::read_to_string(e.path()).unwrap_or_default();
                    let _ = std::fs::remove_file(e.path());
                    let n = app_display_name(exe.trim());
                    if !n.is_empty() && !names.contains(&n) {
                        names.push(n);
                    }
                }
                if !names.is_empty() {
                    shared.lock().day.log(now, "app_blocked", format!("Не дал запустить: {}", names.join(", ")));
                    if now - last_nope > 1_500 {
                        last_nope = now;
                        shared.nope(&format!("{} — после учёбы", names.join(", ")));
                    }
                }
            }
        }

        // Foreground browser tab showing a blocked site -> wag a finger (once per site per 20 s).
        if blocked && now - last_tab_check >= 800 {
            last_tab_check = now;
            let hit = shared.blocker.lock().unwrap_or_else(|e| e.into_inner()).blocked_tab(&sites);
            match hit {
                Some(site) if last_tab_hit.as_ref().map(|(s, t)| s != &site || now - t > 20_000).unwrap_or(true) => {
                    last_tab_hit = Some((site.clone(), now));
                    last_nope = now;
                    shared.lock().day.log(now, "site_attempt", format!("Попытка открыть {site}"));
                    shared.nope(&format!("{site} — после учёбы"));
                }
                _ => {}
            }
        }

        if emit {
            let _ = shared.app.emit("state", &snap);
        }
        if now - last_tray >= 1_000 {
            last_tray = now;
            shared.update_tray(&snap.view);
        }
    }
}

// ---------------- MCP bridge ----------------

pub struct McpBridge(pub Arc<Shared>);

impl McpHost for McpBridge {
    fn session_state(&self) -> Value {
        let s = self.0.snapshot();
        let v = s.view;
        json!({
            "date": v.date,
            "day": if v.completed { "completed" } else if v.started { "started" } else { "not_started" },
            "study_day": v.study_day,
            "profile": v.kind,
            "mode": v.mode,
            "phase": v.phase.kind,
            "title": v.phase.title,
            "subtitle": v.phase.subtitle,
            "paused": v.phase.paused,
            "remaining": mmss(v.phase.remaining_ms),
            "remaining_ms": v.phase.remaining_ms,
            "elapsed_ms": v.phase.elapsed_ms,
            "waiting_for_start_ms": v.phase.waiting_ms,
            "current_block": v.phase.block.and_then(|i| v.blocks.get(i)).map(|b| b.name.clone()),
            "blocks": v.blocks.iter().map(|b| if b.kind.is_study() {
                json!({
                    "name": b.name, "planned_min": b.minutes, "done_min": worked_min(b.work_ms),
                    "parts": b.parts, "parts_done": b.parts_done, "done": b.done, "current": b.current
                })
            } else {
                json!({ "name": b.name, "type": "break", "planned_min": b.minutes, "done": b.done, "taken": b.started || b.queued, "queued": b.queued })
            }).collect::<Vec<_>>(),
            "worked": fmt_dur(v.work_ms),
            "planned": fmt_dur(v.planned_ms),
            "blocking": { "active": v.lock.blocked, "day_lock": v.lock.base, "reason": v.lock.reason },
            "day_end": v.day_end,
            "day_end_default": v.day_end_base,
            "day_end_next_day": v.day_end_next_day,
            "plan_forecast": forecast_json(&v.forecast, s.meta.tz_offset_min),
            "segment": (v.phase.kind == "segment").then(|| {
                let (planned, prep_min) = segment_minutes(&self.0);
                json!({
                    "type": v.phase.title,
                    "planned_min": planned,
                    // While getting ready: minutes of the preparation and how far past it.
                    "elapsed_min": v.phase.elapsed_ms / MIN,
                    "overrun_min": (-v.phase.remaining_ms).max(0) / MIN,
                    "alarm": v.phase.alarm,
                    "prep": v.phase.prep,
                    "prep_min": prep_min,
                    "stopwatch": v.phase.stopwatch,
                    "queue": v.phase.queue,
                })
            }),
        })
    }

    fn day_stats(&self, date: Option<&str>) -> Result<Value, String> {
        let now = clock::now_ts();
        let g = self.0.lock();
        let tz = g.cfg.tz_offset_min;
        let today = g.day.date.format("%Y-%m-%d").to_string();
        let stats = match date {
            None => stats::day_stats(&g.day, tz, now),
            Some(d) if d == today => stats::day_stats(&g.day, tz, now),
            Some(d) => {
                let day = self.0.store.load_day(d).ok_or(format!("За {d} записей нет."))?;
                stats::day_stats(&day, tz, now)
            }
        };
        serde_json::to_value(stats).map_err(|e| e.to_string())
    }

    fn week_stats(&self, date: Option<&str>) -> Result<Value, String> {
        let (w, tsv) = week(&self.0, date)?;
        let mut v = serde_json::to_value(w).map_err(|e| e.to_string())?;
        v["tsv"] = json!(tsv);
        Ok(v)
    }

    fn get_plan(&self, date: Option<&str>) -> Result<Value, String> {
        let g = self.0.lock();
        let today = g.day.date;
        let mut v = json!({
            "date": today.format("%Y-%m-%d").to_string(),
            "today": g.day.plan,
            "profile": g.day.kind,
            "template": g.cfg.profile(g.day.kind).plan,
            "started": g.day.started_at.is_some(),
            "day_end": fmt_day_min(g.day.day_end(&g.cfg)),
            "day_end_default": clockmanage_core::config::fmt_hm(g.cfg.day_end_min),
            "segment_types": g.cfg.segments.iter().map(|t| json!({ "name": t.name, "minutes": t.minutes, "alarm": t.alarm })).collect::<Vec<_>>(),
            "templates": templates_json(&g.cfg),
            "week": week_json(&g.cfg),
            "upcoming": (1..=7).map(|n| day_ahead_json(&g.cfg, today + chrono::Days::new(n))).collect::<Vec<_>>(),
        });
        if let Some(d) = date {
            let d = parse_plan_date(d)?;
            v["day"] = if d == today {
                json!({ "date": d.format("%Y-%m-%d").to_string(), "weekday": WEEKDAYS[g.day.weekday()], "profile": g.day.kind, "plan": g.day.plan, "source": "today" })
            } else if d < today {
                return Err(format!("{d} уже прошёл — план прошлых дней смотри в get_today_stats с date."));
            } else {
                day_ahead_json(&g.cfg, d)
            };
        }
        Ok(v)
    }

    fn set_plan(&self, req: PlanRequest) -> Result<Value, String> {
        let (today, template) = {
            let g = self.0.lock();
            (g.day.date, g.cfg.profile(g.day.kind).plan.clone())
        };
        match req.date.as_deref().map(parse_plan_date).transpose()? {
            Some(d) if d != today => apply_plan_ahead(&self.0, d, req, "mcp"),
            // Today back to its template: a plain set_plan, with the usual rules for started blocks.
            Some(_) if req.use_template => apply_plan(&self.0, template, &[], false, "mcp"),
            _ => apply_plan(&self.0, req.plan, &req.close, req.save_as_template, "mcp"),
        }
    }

    fn set_template(&self, profile: DayKind, plan: Vec<PlanBlock>) -> Result<Value, String> {
        let (mut next, before) = {
            let g = self.0.lock();
            (g.cfg.clone(), g.day.plan.clone())
        };
        next.profiles.get_mut(profile).plan = plan;
        let saved = apply_config(&self.0, next)?;
        let today_updated = self.0.lock().day.plan != before;
        let overridden: Vec<String> = saved
            .plan_overrides
            .keys()
            .filter(|d| saved.kind_on(**d) == profile)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .collect();
        let days: Vec<&str> = (0..7).filter(|&i| saved.week[i] == profile).map(|i| WEEKDAYS[i]).collect();
        self.0.notice(&format!("Агент изменил шаблон «{}»", profile.label()), &plan_line(&saved.profile(profile).plan));
        Ok(json!({
            "ok": true,
            "profile": profile,
            "template": saved.profile(profile).plan,
            "weekdays": days,
            "today_plan_updated": today_updated,
            "overridden_dates": overridden,
            "upcoming": upcoming_json(&self.0),
        }))
    }

    fn set_week_schedule(&self, days: Vec<(usize, DayKind)>) -> Result<Value, String> {
        let (mut next, kind_before) = {
            let g = self.0.lock();
            (g.cfg.clone(), g.day.kind)
        };
        let mut changed = vec![];
        for (i, k) in days {
            if next.week[i] != k {
                changed.push(format!("{} → {}", WEEKDAYS[i], k.label()));
                next.week[i] = k;
            }
        }
        let saved = apply_config(&self.0, next)?;
        let kind_now = self.0.lock().day.kind;
        if !changed.is_empty() {
            self.0.notice("Агент изменил неделю", &changed.join(", "));
        }
        Ok(json!({
            "ok": true,
            "changed": changed,
            "week": week_json(&saved),
            "today_profile": kind_now,
            "today_profile_changed": kind_now != kind_before,
            "upcoming": upcoming_json(&self.0),
        }))
    }

    fn set_day_end(&self, time: &str, reason: Option<&str>) -> Result<Value, String> {
        apply_day_end(&self.0, time, reason, "mcp")
    }

    fn set_block_note(&self, name: Option<&str>, note: &str) -> Result<Value, String> {
        apply_block_note(&self.0, None, name, Some(note), "mcp")
    }

    fn finish_block(&self, name: Option<&str>, confirm_token: Option<&str>) -> Result<Value, String> {
        let now = clock::now_ts();
        let Some(typed) = confirm_token else {
            // Step 1: nothing changes, the agent has to ask the user first.
            let g = self.0.lock();
            let i = g.day.finish_target(name)?;
            let worked = g.day.block_work_live(i, now);
            let b = g.day.plan[i].clone();
            drop(g);
            if worked < MIN {
                return Err(format!("По «{}» отработано меньше минуты — закрывать нечего. Если он сегодня не нужен, убери его через set_plan: такой блок удаляется.", b.name));
            }
            // The same rounding as the close itself and done_min: whole minutes, down.
            let to_min = worked_min(worked).max(1);
            let token = {
                use rand::Rng;
                format!("{:08x}", rand::thread_rng().gen::<u32>())
            };
            *self.0.finish_token.lock().unwrap() = Some(FinishToken { token: token.clone(), block: b.name.clone(), until: now + FINISH_TOKEN_MS });
            let cut = b.minutes.saturating_sub(to_min);
            return Ok(json!({
                "needs_confirmation": true,
                "block": b.name,
                "worked_min": fmt_min(worked).replace(',', ".").parse::<f64>().unwrap_or(0.0),
                "planned_min": b.minutes,
                "new_planned_min": to_min,
                "cut_min": cut,
                "confirm_token": token,
                "expires_in_s": FINISH_TOKEN_MS / 1000,
                "ask_user": format!(
                    "Закрыть «{}» сейчас на {} мин из {}?{} Это попадёт в лог.",
                    b.name, fmt_min(worked), b.minutes,
                    if cut > 0 { format!(" {cut} мин уйдут из плана.") } else { String::new() }
                ),
                "next_step": "Задай пользователю вопрос из ask_user. Только после явного «да» вызови finish_block с этим confirm_token.",
            }));
        };
        // Step 2: one-time token for the block shown in step 1.
        let t = self.0.finish_token.lock().unwrap().take();
        let t = t.ok_or("Нет открытого подтверждения: сначала вызови finish_block без confirm_token и спроси пользователя.")?;
        if t.token != typed.trim() || now > t.until {
            return Err("confirm_token не подходит или истёк — вызови finish_block без токена и снова спроси пользователя.".into());
        }
        apply_finish(&self.0, Some(&t.block), "mcp")
    }
}

/// End-of-block line from the UI, the overlay or the agent.
pub fn apply_block_note(shared: &Arc<Shared>, block: Option<usize>, name: Option<&str>, note: Option<&str>, by: &str) -> Result<Value, String> {
    shared.mutate(|g, now| {
        let i = match (block, name.map(str::trim).filter(|n| !n.is_empty())) {
            (Some(i), _) => i,
            (None, Some(n)) => g.day.plan.iter().position(|b| !b.is_break() && same_name(&b.name, n)).ok_or(format!("В плане нет блока «{n}»."))?,
            (None, None) => g
                .day
                .pending_note()
                .or_else(|| (0..g.day.plan.len()).filter(|&i| g.day.is_block_done(i)).max_by_key(|&i| g.day.progress[i].completed_at))
                .ok_or("Сегодня ещё нет закрытых блоков — укажи name.")?,
        };
        g.day.set_block_note(now, i, note, by)?;
        Ok(json!({ "ok": true, "block": g.day.plan[i].name, "note": g.day.progress[i].note }))
    })
}

/// "Finish block" from the UI (`by = "ui"`) or the agent (`"mcp"`).
pub fn apply_finish(shared: &Arc<Shared>, name: Option<&str>, by: &str) -> Result<Value, String> {
    let (res, events) = shared.mutate(|g, now| {
        let (cfg, day) = (&g.cfg, &mut g.day);
        let (f, events) = day.finish_block(now, cfg, name, by)?;
        let v = view::build(&g.day, &g.cfg, now);
        let res = json!({
            "ok": true,
            "block": f.block,
            "worked_min": fmt_min(f.worked_ms).replace(',', ".").parse::<f64>().unwrap_or(0.0),
            "planned_min_before": f.from_min,
            "planned_min": f.to_min,
            "now": v.phase.title,
            "phase": v.phase.kind,
            "plan_forecast": forecast_json(&g.day.forecast(now, &g.cfg), g.cfg.tz_offset_min),
        });
        Ok((res, events))
    })?;
    let snap = shared.snapshot();
    shared.react(&events, &snap);
    if by == "mcp" {
        let text = format!("«{}»: {} из {} мин", res["block"].as_str().unwrap_or(""), res["worked_min"], res["planned_min_before"]);
        shared.notice("Агент закрыл блок", &text);
    }
    Ok(res)
}

/// Planned minutes of the running segment and its preparation (0 without one).
fn segment_minutes(shared: &Shared) -> (u32, u32) {
    let g = shared.lock();
    g.day.segment().map(|r| (r.planned_min, r.prep_min)).unwrap_or((0, 0))
}

fn forecast_json(f: &Forecast, tz: i32) -> Value {
    json!({
        "plan_left_min": (f.work_left_ms + MIN - 1) / MIN,
        "breaks_left_min": f.breaks_left_ms / MIN,
        "finish_estimate": clock::hm(f.finish_at, tz),
        "segments_left_min": (f.segments_left_ms + MIN - 1) / MIN,
        "fits": f.fits,
        "margin_min": f.margin_ms.div_euclid(MIN),
        "note": "Если продолжать прямо сейчас без пауз. Отрезки (обед, сон…) учтены: идущий, очередь и запланированные в плане, с подготовкой ко сну.",
    })
}

fn templates_json(cfg: &Config) -> Value {
    let mut v = json!({});
    for k in DayKind::ALL {
        v[k.key()] = json!({ "label": k.label(), "plan": cfg.profile(k).plan, "blocking": cfg.profile(k).block });
    }
    v
}

fn week_json(cfg: &Config) -> Value {
    let mut v = json!({});
    for (i, d) in WEEKDAYS.iter().enumerate() {
        v[*d] = json!(cfg.week[i]);
    }
    v
}

/// What a day after today starts with.
fn day_ahead_json(cfg: &Config, d: NaiveDate) -> Value {
    json!({
        "date": d.format("%Y-%m-%d").to_string(),
        "weekday": WEEKDAYS[d.weekday().num_days_from_monday() as usize],
        "profile": cfg.kind_on(d),
        "plan": cfg.plan_on(d),
        "source": if cfg.plan_overrides.contains_key(&d) { "date" } else { "template" },
    })
}

fn upcoming_json(shared: &Shared) -> Vec<Value> {
    let g = shared.lock();
    (1..=7).map(|n| day_ahead_json(&g.cfg, g.day.date + chrono::Days::new(n))).collect()
}

fn parse_plan_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| format!("Дата — YYYY-MM-DD, а не «{s}»."))
}

fn plan_line(plan: &[PlanBlock]) -> String {
    if plan.is_empty() {
        return "пусто".into();
    }
    plan.iter().map(|b| format!("{} {}", b.name, b.minutes)).collect::<Vec<_>>().join(" · ")
}

/// A plan for a later day (MCP `set_plan` with `date`): today's plan stays as it is.
pub fn apply_plan_ahead(shared: &Arc<Shared>, date: NaiveDate, req: PlanRequest, by: &str) -> Result<Value, String> {
    let (cfg, today, before) = {
        let g = shared.lock();
        (g.cfg.clone(), g.day.date, g.day.plan.clone())
    };
    if date < today {
        return Err(format!("{date} уже прошёл — план можно задать на сегодня или позже."));
    }
    if req.close.iter().any(|c| *c) {
        return Err("close закрывает начатый блок сегодняшнего дня — в плане на другой день закрывать нечего.".into());
    }
    if (date - today).num_days() > MAX_PLAN_AHEAD_DAYS {
        return Err(format!("План можно задать не дальше чем на {MAX_PLAN_AHEAD_DAYS} дней вперёд."));
    }
    let kind = cfg.kind_on(date);
    let mut plan = req.plan;
    for b in &mut plan {
        b.normalize();
    }
    if req.save_as_template {
        // Through the settings path: today follows an untouched template of the same profile.
        let mut next = cfg;
        next.profiles.get_mut(kind).plan = plan.clone();
        apply_config(shared, next)?;
    }
    let (plan, source, today_changed) = shared.mutate(|g, _| {
        if req.use_template {
            g.cfg.plan_overrides.remove(&date);
        } else {
            g.cfg.plan_overrides.insert(date, plan);
        }
        shared.store.save_config(&g.cfg);
        let source = if g.cfg.plan_overrides.contains_key(&date) { "date" } else { "template" };
        Ok((g.cfg.plan_on(date).clone(), source, g.day.plan != before))
    })?;
    let _ = shared.app.emit("config", ());
    if by == "mcp" {
        let label = date.format("%d.%m");
        let text = if req.use_template { format!("{label}: снова шаблон «{}»", kind.label()) } else { format!("{label}: {}", plan_line(&plan)) };
        shared.notice("Агент задал план на другой день", &text);
    }
    Ok(json!({
        "ok": true,
        "date": date.format("%Y-%m-%d").to_string(),
        "weekday": WEEKDAYS[date.weekday().num_days_from_monday() as usize],
        "profile": kind,
        "plan": plan,
        "source": source,
        "saved_as_template": req.save_as_template,
        "today_plan_changed": today_changed,
        "note": format!("День {date} начнётся с этого плана (профиль «{}»).{}", kind.label(), if today_changed { "" } else { " Сегодняшний план не менялся." }),
    }))
}

/// Set today's plan from the UI (`by = "ui"`), the phone or the agent (`"mcp"`); `close[j]`
/// closes started block `j` as it is.
pub fn apply_plan(shared: &Arc<Shared>, plan: Vec<PlanBlock>, close: &[bool], save_as_template: bool, by: &str) -> Result<Value, String> {
    let (res, changes, events) = shared.mutate(|g, now| {
        let locked = g.day.plan_lock(now, &g.cfg);
        let (cfg, day) = (&g.cfg, &mut g.day);
        let (changes, events) = day.edit_plan(now, cfg, plan.clone(), close, locked, by)?;
        if save_as_template {
            let kind = g.day.kind;
            g.cfg.profiles.get_mut(kind).plan = g.day.plan.clone();
            shared.store.save_config(&g.cfg);
        }
        let res = json!({
            "ok": true,
            "plan": g.day.plan,
            "locked": locked,
            "changes": changes,
            "changed": changes.iter().map(fmt_change).collect::<Vec<_>>(),
            "plan_forecast": forecast_json(&g.day.forecast(now, &g.cfg), g.cfg.tz_offset_min),
        });
        Ok((res, changes, events))
    })?;
    if !events.is_empty() {
        let snap = shared.snapshot();
        shared.react(&events, &snap);
    }
    let _ = shared.app.emit("config", ());
    if by != "ui" && !changes.is_empty() {
        shared.notice(if by == "mcp" { "Агент изменил план" } else { "Телефон изменил план" }, &changes.iter().map(fmt_change).collect::<Vec<_>>().join(", "));
    }
    Ok(res)
}

/// One-off shift of today's day end from the UI or the agent.
pub fn apply_day_end(shared: &Arc<Shared>, time: &str, reason: Option<&str>, by: &str) -> Result<Value, String> {
    let (res, notice) = shared.mutate(|g, now| {
        let c = g.day.set_day_end(now, &g.cfg, time, reason, by)?;
        let tz = g.cfg.tz_offset_min;
        let changed = c.from_min != c.to_min;
        let lock = g.day.lock_state(now, &g.cfg);
        let res = json!({
            "ok": true,
            "changed": changed,
            "old": fmt_day_min(c.from_min),
            "new": fmt_day_min(c.to_min),
            "new_is_next_day": c.to_min >= 24 * 60,
            "day_end_default": clockmanage_core::config::fmt_hm(g.cfg.day_end_min),
            "reason": c.reason,
            "plan": forecast_json(&g.day.forecast(now, &g.cfg), tz),
            "blocking": { "active": lock.blocked, "day_lock": lock.base, "reason": lock.reason },
        });
        let notice = changed.then(|| {
            format!(
                "{} → {}{}",
                fmt_day_min(c.from_min),
                fmt_day_min(c.to_min),
                c.reason.as_ref().map(|r| format!(" ({r})")).unwrap_or_default()
            )
        });
        Ok((res, notice))
    })?;
    if let (Some(text), "mcp") = (notice, by) {
        shared.notice("Агент изменил конец дня", &text);
    }
    Ok(res)
}

// ---------------- commands ----------------

type S<'a> = State<'a, Arc<Shared>>;

#[tauri::command]
pub fn get_state(s: S) -> Snapshot {
    s.snapshot()
}

#[tauri::command]
pub fn get_config(s: S) -> Config {
    s.lock().cfg.clone()
}

#[tauri::command]
pub fn save_config(s: S, cfg: Config) -> Result<Config, String> {
    apply_config(s.inner(), cfg)
}

/// Save a whole config (settings screen, templates, the agent's templates and week) under the
/// lock rules; today follows a re-assigned weekday or an untouched template.
pub fn apply_config(shared: &Arc<Shared>, mut cfg: Config) -> Result<Config, String> {
    cfg.normalize();
    let shared = shared.clone();
    let (autostart_changed, mcp_changed, phone_changed, saved) = shared.mutate(|g, now| {
        // Paired phones are managed by pairing / "forget" only: a settings screen opened before
        // a phone was paired must not drop its token. Plans set ahead for dates come from MCP.
        cfg.phone.devices = g.cfg.phone.devices.clone();
        cfg.plan_overrides = g.cfg.plan_overrides.clone();
        let ctx = EditContext { locked: g.day.base_lock(now, &g.cfg) };
        g.cfg.check_update(&cfg, ctx)?;
        let autostart_changed = cfg.autostart != g.cfg.autostart;
        let mcp_changed = cfg.mcp_enabled != g.cfg.mcp_enabled || cfg.mcp_port != g.cfg.mcp_port;
        let phone_changed = cfg.phone.enabled != g.cfg.phone.enabled || cfg.phone.port != g.cfg.phone.port;
        let old = std::mem::replace(&mut g.cfg, cfg.clone());
        if g.day.started_at.is_none() {
            let wd = g.day.weekday();
            let kind = g.day.kind;
            if old.week[wd] != g.cfg.week[wd] {
                // Today was re-assigned in the week schedule: follow it.
                // (The config is already accepted; syncing today is best effort.)
                let (cfg, day) = (&g.cfg, &mut g.day);
                let _ = day.set_kind(now, cfg, cfg.week[wd], false);
            } else if g.day.plan == old.profile(kind).plan && old.profile(kind).plan != g.cfg.profile(kind).plan {
                // Today's plan was still the untouched template: take the new one.
                let plan = g.cfg.profile(kind).plan.clone();
                let _ = g.day.set_plan(now, plan, false);
            }
            g.day.study_day = g.cfg.profile(g.day.kind).block;
            g.day.timing = g.cfg.timing.clone();
        } else {
            // A running day takes shorter breaks at once; longer ones only outside the lock.
            let locked = ctx.locked;
            let (t, n) = (&mut g.day.timing, &cfg.timing);
            for (cur, new) in [
                (&mut t.short_break_min, n.short_break_min),
                (&mut t.between_blocks_min, n.between_blocks_min),
                (&mut t.lunch_min, n.lunch_min),
            ] {
                if new < *cur || !locked {
                    *cur = new;
                }
            }
        }
        shared.store.save_config(&g.cfg);
        Ok((autostart_changed, mcp_changed, phone_changed, g.cfg.clone()))
    })?;
    if autostart_changed {
        system::set_autostart(saved.autostart)?;
    }
    if mcp_changed {
        let port = shared.mcp.start(shared.clone(), saved.mcp_enabled, saved.mcp_port);
        if port != saved.mcp_port {
            shared.lock().cfg.mcp_port = port;
            shared.store.save_config(&shared.lock().cfg);
        }
    }
    if phone_changed {
        shared.phone.start(shared.clone(), &saved);
    }
    let _ = shared.app.emit("config", ());
    let _ = shared.app.emit("state", shared.snapshot());
    let cfg = shared.lock().cfg.clone();
    Ok(cfg)
}

#[tauri::command]
pub fn regenerate_port(s: S) -> Result<u16, String> {
    let shared = s.inner().clone();
    let enabled = shared.lock().cfg.mcp_enabled;
    let port = shared.mcp.start(shared.clone(), enabled, crate::mcp_server::random_port());
    {
        let mut g = shared.lock();
        g.cfg.mcp_port = port;
        shared.store.save_config(&g.cfg);
    }
    let _ = shared.app.emit("config", ());
    let _ = shared.app.emit("state", shared.snapshot());
    Ok(port)
}

#[derive(Serialize)]
pub struct PinView {
    pin: String,
    until: Ts,
}

/// Open a 6-digit PIN for pairing a phone (two minutes).
#[tauri::command]
pub fn phone_pin(s: S) -> Result<PinView, String> {
    if !s.lock().cfg.phone.enabled {
        return Err("Сначала включи синхронизацию с телефоном.".into());
    }
    let (pin, until) = s.phone.new_pin();
    let _ = s.app.emit("state", s.snapshot());
    Ok(PinView { pin, until })
}

#[tauri::command]
pub fn phone_forget(s: S, id: String) -> Result<(), String> {
    s.mutate(|g, now| {
        let name = g.cfg.phone.devices.iter().find(|d| d.id == id).map(|d| d.name.clone()).ok_or("Такого телефона нет.")?;
        g.cfg.phone.devices.retain(|d| d.id != id);
        s.store.save_config(&g.cfg);
        g.day.log(now, "phone", format!("Телефон «{name}» отключён"));
        Ok(())
    })?;
    let _ = s.app.emit("config", ());
    Ok(())
}

#[tauri::command]
pub fn start_day(s: S) -> Result<(), String> {
    s.mutate(|g, now| g.day.start_day(now, &g.cfg))
}

#[tauri::command]
pub fn set_day_kind(s: S, kind: DayKind) -> Result<(), String> {
    s.mutate(|g, now| {
        let locked = g.day.plan_lock(now, &g.cfg);
        let (cfg, day) = (&g.cfg, &mut g.day);
        day.set_kind(now, cfg, kind, locked)
    })
}

#[tauri::command]
pub fn extend_day_end(s: S, minutes: u32) -> Result<(), String> {
    s.mutate(|g, now| {
        let (cfg, day) = (&g.cfg, &mut g.day);
        day.extend_day_end(now, cfg, minutes)
    })
}

#[tauri::command]
pub fn pause(s: S) -> Result<(), String> {
    s.mutate(|g, now| g.day.pause(now, &g.cfg))
}

#[tauri::command]
pub fn resume(s: S) -> Result<(), String> {
    s.mutate(|g, now| g.day.resume(now))
}

#[tauri::command]
pub fn start_next(s: S, expect: Option<String>) -> Result<(), String> {
    let r = s.mutate(|g, now| {
        // The button was drawn for a phase; if the timer moved on since (a click queued
        // behind another one), don't skip into the next part by accident.
        if let Some(e) = &expect {
            if view::build(&g.day, &g.cfg, now).phase.kind != *e {
                return Err("Таймер уже перешёл дальше — ничего не сделал.".into());
            }
        }
        g.day.start_next(now)
    });
    if r.is_ok() {
        hide_overlay_window(&s.app);
    }
    r
}

/// `note: None` = skip ("не сейчас").
#[tauri::command]
pub fn set_block_note(s: S, block: usize, note: Option<String>) -> Result<(), String> {
    apply_block_note(s.inner(), Some(block), None, note.as_deref(), "ui").map(|_| ())
}

#[derive(serde::Deserialize)]
pub struct SegmentPick {
    name: String,
    minutes: Option<u32>,
    /// "Без времени": a stopwatch instead of a countdown.
    #[serde(default)]
    stopwatch: bool,
}

/// Start segments now (or queue them behind the running one): lunch → nap.
#[tauri::command]
pub fn start_segments(s: S, items: Vec<SegmentPick>) -> Result<(), String> {
    let r = s.mutate(|g, now| {
        let items = items
            .iter()
            .map(|i| {
                let q = QueuedSegment::of(&g.cfg, &i.name, i.minutes);
                if i.stopwatch { q.stopwatch() } else { q }
            })
            .collect();
        g.day.start_segments(now, items)
    });
    if r.is_ok() {
        hide_overlay_window(&s.app);
    }
    r
}

#[tauri::command]
pub fn end_segment(s: S) -> Result<(), String> {
    let r = s.mutate(|g, now| g.day.end_segment(now));
    if r.is_ok() {
        hide_overlay_window(&s.app);
    }
    r
}

/// "Лёг": the nap's countdown and alarm start now.
#[tauri::command]
pub fn lay_down(s: S) -> Result<(), String> {
    let r = s.mutate(|g, now| g.day.lay_down(now));
    if r.is_ok() {
        hide_overlay_window(&s.app);
    }
    r
}

/// The running segment: a stopwatch ("без времени") or back to the countdown.
#[tauri::command]
pub fn set_segment_mode(s: S, stopwatch: bool) -> Result<(), String> {
    s.mutate(|g, now| g.day.set_segment_mode(now, stopwatch))
}

#[tauri::command]
pub fn drop_queued(s: S, index: usize) -> Result<(), String> {
    s.mutate(|g, now| g.day.drop_queued(now, index))
}

#[tauri::command]
pub fn undo_skip(s: S) -> Result<(), String> {
    s.mutate(|g, now| g.day.undo_skip(now))
}

/// Tray / mini window "main button": whatever the primary action is right now.
pub fn primary_action(shared: &Arc<Shared>) -> Result<(), String> {
    let v = shared.snapshot().view;
    if v.can.resume {
        shared.mutate(|g, now| g.day.resume(now))
    } else if v.can.pause {
        shared.mutate(|g, now| g.day.pause(now, &g.cfg))
    } else if v.phase.kind == "await" || v.phase.kind == "lunch" {
        let r = shared.mutate(|g, now| g.day.start_next(now));
        hide_overlay_window(&shared.app);
        r
    } else if v.can.lay_down {
        let r = shared.mutate(|g, now| g.day.lay_down(now));
        hide_overlay_window(&shared.app);
        r
    } else if v.can.end_segment {
        let r = shared.mutate(|g, now| g.day.end_segment(now));
        hide_overlay_window(&shared.app);
        r
    } else if v.can.start_day {
        shared.mutate(|g, now| g.day.start_day(now, &g.cfg))
    } else {
        Ok(())
    }
}

#[tauri::command]
pub fn primary(s: S) -> Result<(), String> {
    primary_action(s.inner())
}

#[tauri::command]
pub fn start_single(s: S, cfg: SingleCfg) -> Result<(), String> {
    s.mutate(|g, now| g.day.start_single(now, cfg))
}

#[tauri::command]
pub fn stop_single(s: S) -> Result<(), String> {
    s.mutate(|g, now| g.day.stop_single(now))
}

#[tauri::command]
pub fn set_plan(s: S, blocks: Vec<PlanBlock>, save_template: bool) -> Result<(), String> {
    apply_plan(s.inner(), blocks, &[], save_template, "ui").map(|_| ())
}

#[tauri::command]
pub fn set_day_end(s: S, time: String, reason: Option<String>) -> Result<Value, String> {
    apply_day_end(s.inner(), &time, reason.as_deref(), "ui")
}

#[tauri::command]
pub fn finish_block(s: S, name: Option<String>) -> Result<Value, String> {
    apply_finish(s.inner(), name.as_deref(), "ui")
}

#[tauri::command]
pub fn emergency(s: S, phrase: String) -> Result<Ts, String> {
    s.mutate(|g, now| g.day.emergency(now, &g.cfg, &phrase))
}

#[tauri::command]
pub fn end_access(s: S) -> Result<(), String> {
    s.mutate(|g, now| g.day.end_access_early(now))
}

#[derive(Serialize)]
pub struct CaptchaView {
    id: u64,
    problems: Vec<String>,
    wait_ms: i64,
}

#[tauri::command]
pub fn captcha_new(s: S) -> CaptchaView {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut problems = vec![];
    let mut answers = vec![];
    let a: i64 = rng.gen_range(13..=99);
    let b: i64 = rng.gen_range(4..=9);
    problems.push(format!("{a} × {b}"));
    answers.push(a * b);
    let a: i64 = rng.gen_range(120..=899);
    let b: i64 = rng.gen_range(120..=899);
    problems.push(format!("{a} + {b}"));
    answers.push(a + b);
    let a: i64 = rng.gen_range(300..=999);
    let b: i64 = rng.gen_range(101..=299);
    let c: i64 = rng.gen_range(3..=9);
    problems.push(format!("{a} − {b} × {c}"));
    answers.push(a - b * c);
    let id = rng.gen::<u32>() as u64;
    s.lock().captcha = Some(Captcha { id, answers, created: clock::now_ts() });
    CaptchaView { id, problems, wait_ms: CAPTCHA_WAIT_MS }
}

#[tauri::command]
pub fn captcha_submit(s: S, id: u64, answers: Vec<i64>) -> Result<Ts, String> {
    s.mutate(|g, now| {
        let c = g.captcha.take().ok_or("Капча устарела — открой заново.")?;
        if c.id != id {
            return Err("Капча устарела — открой заново.".into());
        }
        if now - c.created < CAPTCHA_WAIT_MS {
            g.captcha = Some(c);
            return Err("Слишком быстро. Подумай ещё пару секунд.".into());
        }
        if c.answers != answers {
            return Err("Есть ошибка. Держи новые примеры.".into());
        }
        g.day.extend_pause_access(now, &g.cfg)
    })
}

#[derive(Serialize)]
pub struct DaySummary {
    date: String,
    planned_min: u32,
    actual_min: f64,
    blocks_done: usize,
    blocks: usize,
    pauses: usize,
    pauses_min: f64,
    emergencies: usize,
    pause_access_min: f64,
    kind: DayKind,
    study_day: bool,
    started: bool,
}

#[tauri::command]
pub fn list_days(s: S) -> Vec<DaySummary> {
    let now = clock::now_ts();
    let (tz, today) = {
        let g = s.lock();
        (g.cfg.tz_offset_min, g.day.clone())
    };
    let today_str = today.date.format("%Y-%m-%d").to_string();
    let mut out = vec![];
    let mut dates = s.store.list_dates();
    if !dates.contains(&today_str) {
        dates.insert(0, today_str.clone());
    }
    for d in dates.into_iter().take(120) {
        let day = if d == today_str { Some(today.clone()) } else { s.store.load_day(&d) };
        let Some(day) = day else { continue };
        let st = stats::day_stats(&day, tz, now);
        out.push(DaySummary {
            date: st.date,
            planned_min: st.planned_min,
            actual_min: st.actual_min,
            blocks_done: st.blocks.iter().filter(|b| b.done).count(),
            blocks: st.blocks.len(),
            pauses: st.pauses_count,
            pauses_min: st.pauses_min,
            emergencies: st.emergency_count,
            pause_access_min: st.pause_access_min,
            kind: st.kind,
            study_day: st.study_day,
            started: st.started_at.is_some(),
        });
    }
    out
}

#[tauri::command]
pub fn day_stats(s: S, date: String) -> Result<DayStats, String> {
    let now = clock::now_ts();
    let g = s.lock();
    if g.day.date.format("%Y-%m-%d").to_string() == date {
        return Ok(stats::day_stats(&g.day, g.cfg.tz_offset_min, now));
    }
    let tz = g.cfg.tz_offset_min;
    drop(g);
    let day = s.store.load_day(&date).ok_or(format!("За {date} записей нет."))?;
    Ok(stats::day_stats(&day, tz, now))
}

/// The week (Monday..Sunday) containing `date` (default: today) from the saved days.
pub fn week(shared: &Shared, date: Option<&str>) -> Result<(stats::WeekStats, String), String> {
    let now = clock::now_ts();
    let (tz, today) = {
        let g = shared.lock();
        (g.cfg.tz_offset_min, g.day.clone())
    };
    let anchor = match date {
        Some(d) => clockmanage_core::chrono::NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").map_err(|_| "Дата нужна в виде ГГГГ-ММ-ДД.")?,
        None => today.date,
    };
    let monday = stats::week_monday(anchor);
    let mut days = vec![];
    for i in 0..7 {
        let d = monday + clockmanage_core::chrono::Duration::days(i);
        let day = if d == today.date { Some(today.clone()) } else { shared.store.load_day(&d.format("%Y-%m-%d").to_string()) };
        if let Some(day) = day {
            days.push(stats::day_stats(&day, tz, now));
        }
    }
    let w = stats::week_stats(monday, &days);
    let tsv = stats::week_tsv(&w);
    Ok((w, tsv))
}

#[derive(Serialize)]
pub struct WeekView {
    week: stats::WeekStats,
    tsv: String,
}

#[tauri::command]
pub fn week_stats(s: S, date: Option<String>) -> Result<WeekView, String> {
    let (week, tsv) = week(s.inner(), date.as_deref())?;
    Ok(WeekView { week, tsv })
}

#[tauri::command]
pub fn export_log(s: S, format: String) -> Result<String, String> {
    let now = clock::now_ts();
    let (tz, today) = {
        let g = s.lock();
        (g.cfg.tz_offset_min, g.day.clone())
    };
    let today_str = today.date.format("%Y-%m-%d").to_string();
    let mut days: Vec<DayStats> = vec![];
    for d in s.store.list_dates() {
        if d == today_str {
            continue;
        }
        if let Some(day) = s.store.load_day(&d) {
            days.push(stats::day_stats(&day, tz, now));
        }
    }
    days.insert(0, stats::day_stats(&today, tz, now));
    days.sort_by(|a, b| a.date.cmp(&b.date));
    let stamp = clock::local(now, tz).format("%Y-%m-%d_%H-%M").to_string();
    let (name, data) = match format.as_str() {
        "csv" => (format!("clockmanage_{stamp}.csv"), stats::to_csv(&days)),
        _ => (format!("clockmanage_{stamp}.json"), serde_json::to_string_pretty(&days).map_err(|e| e.to_string())?),
    };
    let path = s.store.write_export(&name, &data).map_err(|e| format!("Не удалось сохранить: {e}"))?;
    system::reveal(&path);
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn open_data_dir(s: S) {
    system::open_path(&s.store.dir);
}

#[tauri::command]
pub fn test_sound(kind: String) {
    if let Some(k) = Sound::parse(&kind) {
        sound::play(k);
    }
}

#[tauri::command]
pub fn preview_overlay(s: S, kind: String) {
    let p = match kind.as_str() {
        "break" => json!({"kind": "break", "passive": true, "auto_hide_ms": 5200, "title": "Перерыв", "text": "Математика: часть 1 из 2 готова. Перерыв 10 мин.", "preview": true}),
        "nope" => json!({"kind": "nope", "passive": true, "auto_hide_ms": 3000, "title": "Не-не-не", "text": "Telegram — после учёбы", "preview": true}),
        "block" => json!({"kind": "block", "passive": false, "title": "«Математика» закрыт", "text": "1 ч 30 мин работы · пауз: 1 (6 мин)", "ask_note": true, "block": 0, "preview": true}),
        _ => json!({"kind": "await", "passive": false, "title": "Перерыв окончен", "text": "Математика · часть 2 из 2", "action": "Начать часть 2", "preview": true}),
    };
    let was = s.lock().cfg.overlay;
    s.lock().cfg.overlay = true;
    s.show_overlay(p);
    s.lock().cfg.overlay = was;
}

#[tauri::command]
pub fn get_overlay(s: S) -> Option<Value> {
    s.overlay.lock().unwrap().clone()
}

pub fn hide_overlay_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
}

#[tauri::command]
pub fn hide_overlay(app: AppHandle) {
    hide_overlay_window(&app);
}

#[tauri::command]
pub fn show_main(app: AppHandle, route: Option<String>) {
    crate::windows::show_main(&app);
    if let Some(r) = route {
        let _ = app.emit_to("main", "navigate", r);
    }
}

#[tauri::command]
pub fn toggle_mini(app: AppHandle) {
    crate::windows::toggle_mini(&app);
}

#[tauri::command]
pub fn style_titlebar(window: tauri::WebviewWindow, bg: String, fg: String, dark: bool) {
    system::style_titlebar(&window, &bg, &fg, dark);
}

#[tauri::command]
pub async fn restart_firefox(s: State<'_, Arc<Shared>>) -> Result<(), String> {
    let shared = s.inner().clone();
    // Restarting waits for Firefox to close and come back: keep it off the UI thread.
    tauri::async_runtime::spawn_blocking(move || {
        // Restart without holding the blocker lock, so the timer thread keeps ticking.
        let r = crate::blocker::restart_firefox();
        if r.is_ok() {
            shared.blocker.lock().unwrap_or_else(|e| e.into_inner()).mark_firefox_restarted();
        }
        let text = match &r {
            Ok(()) => "Firefox перезапущен вручную".to_string(),
            Err(e) => format!("Перезапуск Firefox не удался: {e}"),
        };
        shared.lock().day.log(clock::now_ts(), "firefox", text);
        r
    })
    .await
    .map_err(|e| e.to_string())?
}
