//! The paired iPhone web app (`pwa/`), whether Meals is on or off: pairing, Web Push out, and the
//! ntfy.sh relay both ways (end-to-end sealed, `clockmanage_core::relay`). Down go the day (the
//! timer and the plan), the diet and the picks; up come its push subscription and, while Meals is
//! on, a pick asked for and diet edits. Its thread starts once an iPhone is linked.

use std::sync::{Arc, OnceLock};
use std::thread::Thread;
use std::time::Duration;

use clockmanage_core::clock::{self, Ts};
use clockmanage_core::i18n::tr;
use clockmanage_core::iphone::Iphone;
use clockmanage_core::relay::{self, NTFY};
use clockmanage_core::tr;
use clockmanage_core::phone;
use clockmanage_core::webpush::{self, Subscription};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::Emitter;
use tauri_plugin_notification::NotificationExt;

use crate::app::Shared;

const PAIR_MS: Ts = 10 * 60_000;
const VAPID_SUBJECT: &str = "https://github.com/NikGob/ClockManage";
/// ntfy.sh keeps a message 12 hours: the day is sent again before that even if it didn't change.
const DAY_REFRESH_MS: Ts = 10 * 3_600_000;
/// A change of the day waits this long, so a few quick presses go out as one message.
const DAY_SETTLE_MS: Ts = 3_000;

#[derive(Default)]
pub struct IphoneRt {
    pairing: Option<(String, Ts)>,
    error: Option<String>,
    /// The day last sent: (version, Meals on, when), and a change seen but not sent yet.
    day_sent: Option<(String, bool, Ts)>,
    day_seen: Option<(String, Ts)>,
}

#[derive(Serialize)]
pub struct PairView {
    pub code: String,
    pub url: String,
    pub qr: String,
    pub until: Ts,
}

#[derive(Serialize)]
pub struct IphoneView {
    /// The web app is linked and takes notifications.
    pub paired: bool,
    /// A link exists (it may still be waiting for the web app's subscription).
    pub linked: bool,
    pub pairing: Option<PairView>,
    pub error: Option<String>,
}

fn rt(shared: &Shared) -> std::sync::MutexGuard<'_, IphoneRt> {
    shared.iphone.lock().unwrap_or_else(|e| e.into_inner())
}

fn emit(shared: &Shared) {
    let _ = shared.app.emit("iphone", ());
}

fn save_cfg(shared: &Shared, f: impl FnOnce(&mut Iphone)) {
    {
        let mut g = shared.lock();
        f(&mut g.cfg.iphone);
        shared.store.save_config(&g.cfg);
    }
    let _ = shared.app.emit("config", ());
}

fn link(shared: &Shared) -> Option<(relay::Link, Vec<u8>)> {
    let link = shared.lock().cfg.iphone.link.clone()?;
    let key = webpush::unb64(&link.key).ok()?;
    Some((link, key))
}

/// The iPhone thread, once an iPhone has been linked since the app started.
static THREAD: OnceLock<Thread> = OnceLock::new();

/// Call at start and after pairing: starts the thread the first time an iPhone is linked and
/// wakes it after a change. Without a linked iPhone there is no thread, or it sleeps.
pub fn wake(shared: &Arc<Shared>) {
    if let Some(t) = THREAD.get() {
        t.unpark();
    } else if shared.lock().cfg.iphone.link.is_some() {
        THREAD.get_or_init(|| start(shared.clone()));
    }
}

fn start(shared: Arc<Shared>) -> Thread {
    let handle = std::thread::Builder::new().name("iphone".into()).spawn(move || {
        let mut last_poll: Ts = 0;
        loop {
            if shared.lock().cfg.iphone.link.is_none() {
                rt(&shared).pairing = None;
                std::thread::park();
                continue;
            }
            std::thread::park_timeout(Duration::from_secs(2));
            let now = clock::now_ts();
            let pairing = rt(&shared).pairing.is_some();
            if now - last_poll >= if pairing { 5_000 } else { 15_000 } {
                last_poll = now;
                if let Err(e) = poll(&shared) {
                    eprintln!("iphone relay: {e}");
                }
                crate::meals::publish_diet(&shared, now);
            }
            publish_day(&shared, now);
            let mut r = rt(&shared);
            if r.pairing.as_ref().is_some_and(|p| p.1 < now) {
                r.pairing = None;
                drop(r);
                emit(&shared);
            }
        }
    });
    handle.expect("iphone thread").thread().clone()
}

pub fn view(shared: &Shared) -> IphoneView {
    let (paired, linked, url_base) = {
        let g = shared.lock();
        let i = &g.cfg.iphone;
        (i.subscription.is_some(), i.link.is_some(), i.pwa_url.clone())
    };
    let r = rt(shared);
    IphoneView {
        paired,
        linked,
        error: r.error.clone(),
        pairing: r.pairing.as_ref().map(|(code, until)| {
            let url = format!("{url_base}?pair={}", code.replace('-', ""));
            PairView { code: code.clone(), qr: qr_svg(&url), url, until: *until }
        }),
    }
}

fn qr_svg(text: &str) -> String {
    use qrcode::render::svg;
    qrcode::QrCode::new(text.as_bytes())
        .map(|q| q.render::<svg::Color>().min_dimensions(220, 220).dark_color(svg::Color("#000000")).light_color(svg::Color("#ffffff")).build())
        .unwrap_or_default()
}

fn vapid_key(shared: &Shared) -> String {
    let key = shared.lock().cfg.iphone.vapid_key.clone();
    if !key.is_empty() {
        return key;
    }
    let key = webpush::new_key();
    let k = key.clone();
    save_cfg(shared, |i| i.vapid_key = k);
    key
}

/// Sends a push to the paired iPhone; `Ok(false)` when there is none.
pub fn push(shared: &Shared, payload: &str) -> Result<bool, String> {
    let Some(sub) = shared.lock().cfg.iphone.subscription.clone() else { return Ok(false) };
    let key = vapid_key(shared);
    let body = webpush::encrypt(&sub, payload.as_bytes()).map_err(|e| tr!("meals_couldnt_send_to_the_iphone", error = e))?;
    let auth = webpush::vapid_auth(&key, &sub.endpoint, VAPID_SUBJECT, clock::now_ts() / 1000).map_err(|e| tr!("meals_couldnt_send_to_the_iphone", error = e))?;
    let res = crate::meals::http()
        .post(&sub.endpoint)
        .header("Authorization", auth)
        .header("Content-Encoding", "aes128gcm")
        .header("Content-Type", "application/octet-stream")
        .header("TTL", "7200")
        .header("Urgency", "high")
        .body(body)
        .send()
        .map_err(|e| tr!("meals_couldnt_send_to_the_iphone", error = e))?;
    match res.status().as_u16() {
        200..=299 => Ok(true),
        404 | 410 => {
            save_cfg(shared, |i| i.subscription = None);
            emit(shared);
            Err(tr("meals_the_iphone_turned_notifications_off").to_string())
        }
        code => Err(tr!("meals_couldnt_send_to_the_iphone", error = format!("HTTP {code} {}", res.text().unwrap_or_default().chars().take(120).collect::<String>()))),
    }
}

fn post_sealed(topic: &str, sealed: &str) -> Result<(), String> {
    for part in relay::parts(sealed) {
        let res = crate::meals::http().post(format!("{NTFY}/{topic}")).body(part).send().map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Err(format!("ntfy: HTTP {}", res.status()));
        }
    }
    Ok(())
}

/// Seals a message for the web app and posts it down the link; `Ok(false)` when none is linked.
pub fn send(shared: &Shared, msg: &Value) -> Result<bool, String> {
    let Some((link, key)) = link(shared) else { return Ok(false) };
    post_sealed(&link.down, &relay::seal(&key, msg.to_string().as_bytes()))?;
    Ok(true)
}

/// A new pairing: a fresh link, posted under the code's topic for the web app to pick up.
pub fn pair(shared: &Arc<Shared>) -> Result<PairView, String> {
    let link = relay::new_link();
    let code = relay::pair_code();
    let vapid = webpush::public_key(&vapid_key(shared)).unwrap_or_default();
    let l = link.clone();
    save_cfg(shared, |i| {
        i.link = Some(l);
        i.subscription = None;
        i.relay_seen = clock::now_ts() / 1000;
    });
    {
        let mut r = rt(shared);
        r.pairing = Some((code.clone(), clock::now_ts() + PAIR_MS));
        r.error = None;
        r.day_sent = None;
    }
    crate::meals::forget_published(shared);
    let msg = json!({ "up": link.up, "down": link.down, "key": link.key, "vapid": vapid, "pc": crate::system::pc_name() }).to_string();
    let sealed = relay::seal(&relay::pair_key(&code), msg.as_bytes());
    let topic = relay::pair_topic(&code);
    let s = shared.clone();
    std::thread::spawn(move || {
        if let Err(e) = post_sealed(&topic, &sealed) {
            rt(&s).error = Some(tr!("meals_couldnt_send_to_the_iphone", error = e));
            emit(&s);
        }
        wake(&s);
    });
    emit(shared);
    Ok(view(shared).pairing.expect("pairing just set"))
}

pub fn unpair(shared: &Shared) {
    save_cfg(shared, |i| {
        i.link = None;
        i.subscription = None;
    });
    rt(shared).pairing = None;
    emit(shared);
}

/// What the web app shows of the day: the timer's timeline and the plan, in the PC's language.
/// Nothing on the iPhone is blocked, so the lock and the block lists stay home.
fn day_message(shared: &Shared, now: Ts) -> (String, bool, Value) {
    let (snap, meals) = {
        let g = shared.lock();
        (phone::snapshot(&g.day, &g.cfg, now), g.cfg.meals.enabled)
    };
    let mut day = json!(snap);
    if let Some(o) = day.as_object_mut() {
        for k in ["lock", "can", "sites", "apps", "mode"] {
            o.remove(k);
        }
    }
    // The PC's colours, as the Android app gets them: the web app looks the same.
    let theme = shared.phone_theme.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let msg = json!({ "t": "day", "day": day, "meals": meals, "pc": crate::system::pc_name(), "theme": theme });
    (snap.version, meals, msg)
}

/// Sends the day when it changed (and settled a moment), and every 10 hours.
fn publish_day(shared: &Shared, now: Ts) {
    let (version, meals, msg) = day_message(shared, now);
    let mut r = rt(shared);
    let fresh = r.day_sent.as_ref().is_some_and(|(v, m, at)| *v == version && *m == meals && now - at < DAY_REFRESH_MS);
    if fresh {
        r.day_seen = None;
        return;
    }
    // The first look at a change only notes it: the message goes once it held still (at once
    // when nothing was sent yet).
    let settled = match &r.day_seen {
        Some((v, at)) if *v == version => Some(now - at >= DAY_SETTLE_MS),
        _ => None,
    };
    match settled {
        Some(false) => return,
        None if r.day_sent.is_some() => {
            r.day_seen = Some((version, now));
            return;
        }
        _ => {}
    }
    drop(r);
    if send(shared, &msg).is_ok() {
        let mut r = rt(shared);
        r.day_sent = Some((version, meals, now));
        r.day_seen = None;
    }
}

/// Applies what the web app sent: its push subscription; while Meals is on, a pick and the diet.
fn poll(shared: &Arc<Shared>) -> Result<(), String> {
    let Some((link, key)) = link(shared) else { return Ok(()) };
    let seen = shared.lock().cfg.iphone.relay_seen;
    let text = crate::meals::http().get(format!("{NTFY}/{}/json?poll=1&since=12h", link.up)).send().and_then(|r| r.text()).map_err(|e| e.to_string())?;
    let bodies: Vec<(i64, String)> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["event"] == "message")
        .filter_map(|v| Some((v["time"].as_i64()?, v["message"].as_str()?.to_string())))
        .collect();
    for (time, sealed) in relay::assemble(&bodies) {
        if time <= seen {
            continue;
        }
        let msg: Option<Value> = relay::open(&key, &sealed).ok().and_then(|b| serde_json::from_slice(&b).ok());
        save_cfg(shared, |i| i.relay_seen = time);
        let Some(msg) = msg else { continue };
        if msg["t"].as_str() == Some("sub") {
            let Ok(sub) = serde_json::from_value::<Subscription>(msg["sub"].clone()) else { continue };
            save_cfg(shared, |i| i.subscription = Some(sub));
            rt(shared).pairing = None;
            emit(shared);
            let title = tr("meals_iphone_connected");
            let body = tr("iphone_it_shows_the_day_now_the");
            let _ = shared.app.notification().builder().title(title).body(body).show();
            let hello = json!({ "title": title, "body": body }).to_string();
            if let Err(e) = push(shared, &hello) {
                rt(shared).error = Some(e);
                emit(shared);
            }
        } else {
            crate::meals::from_phone(shared, &msg);
        }
    }
    Ok(())
}

// ---- commands ----

type S<'a> = tauri::State<'a, Arc<Shared>>;

#[tauri::command(async)]
pub fn iphone_state(s: S) -> IphoneView {
    view(&s)
}

#[tauri::command(async)]
pub fn iphone_pair(s: S) -> Result<PairView, String> {
    pair(s.inner())
}

#[tauri::command(async)]
pub fn iphone_unpair(s: S) {
    unpair(&s);
}
