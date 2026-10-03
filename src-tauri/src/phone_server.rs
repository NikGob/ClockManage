//! Phone sync over Wi-Fi: `http://<pc>:<port>/api/...` for the Android app in the same network.
//!
//! - Discovery: the phone broadcasts `CLOCKMANAGE_DISCOVER` to UDP 47810, the PC answers with
//!   its name and API port.
//! - Pairing: the user opens a 6-digit PIN in the settings (2 minutes, 5 attempts) and types it
//!   on the phone; the phone gets a long random token and sends it with every request.
//! - `GET /api/state?v=<version>&wait=25` is a long poll: it answers at once when the day changed
//!   since `version`, otherwise holds the request until it does (or `wait` runs out). The phone
//!   keeps one such request open while its screen shows the timer, nothing when it does not.

use std::collections::HashMap;
use std::io::Read;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use clockmanage_core::clock::{self, Ts};
use clockmanage_core::config::{PhoneDevice, PHONE_DISCOVERY_PORT};
use clockmanage_core::day::PlanBlock;
use clockmanage_core::{phone, Config};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::Emitter;
use tiny_http::{Header, Method, Request, Response, Server};

use crate::app::Shared;

const PIN_TTL_MS: i64 = 120_000;
const PIN_ATTEMPTS: u32 = 5;
const MAX_DEVICES: usize = 5;
const MAX_WAIT_S: u64 = 30;

#[derive(Debug, Clone, Default, Serialize)]
pub struct PhoneDeviceStatus {
    pub id: String,
    pub name: String,
    pub paired_at: Ts,
    pub last_seen: Option<Ts>,
    /// The phone reported whether its accessibility blocker is on.
    pub blocker: Option<bool>,
}

/// The APK bundled with this PC build (CI puts it next to apk.json).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ApkInfo {
    pub available: bool,
    /// Android versionCode (grows with every CI build).
    pub code: u64,
    pub name: String,
    pub size: u64,
}

pub const APK_FILE: &str = "ClockManage-android.apk";

pub fn apk_info(dir: Option<&std::path::Path>) -> ApkInfo {
    let Some(dir) = dir else { return ApkInfo::default() };
    let meta: Value = std::fs::read_to_string(dir.join("apk.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
    let size = std::fs::metadata(dir.join(APK_FILE)).map(|m| m.len()).unwrap_or(0);
    let code = meta.get("code").and_then(Value::as_u64).unwrap_or(0);
    ApkInfo { available: code > 0 && size > 0, code, name: meta.get("name").and_then(Value::as_str).unwrap_or("").to_string(), size }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PhoneStatus {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    pub address: Option<String>,
    pub pc_name: String,
    pub error: Option<String>,
    pub pin: Option<String>,
    pub pin_until: Option<Ts>,
    pub devices: Vec<PhoneDeviceStatus>,
    /// The phone app this PC build carries (phones update from it over Wi-Fi).
    pub apk: ApkInfo,
}

struct Pin {
    code: String,
    until: Ts,
    attempts: u32,
}

#[derive(Default)]
pub struct PhoneServer {
    server: Mutex<Option<Arc<Server>>>,
    running: Mutex<(bool, u16, Option<String>)>,
    pin: Mutex<Option<Pin>>,
    /// Device id -> (last seen, blocker on).
    seen: Mutex<HashMap<String, (Ts, bool)>>,
    discovery: AtomicBool,
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn reply(req: Request, code: u16, body: &Value) {
    let _ = req.respond(
        Response::from_string(body.to_string())
            .with_status_code(code)
            .with_header(header("Content-Type", "application/json; charset=utf-8"))
            .with_header(header("Cache-Control", "no-store")),
    );
}

fn error(req: Request, code: u16, msg: &str) {
    reply(req, code, &json!({ "error": msg }));
}

fn random_hex(bytes: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..bytes).map(|_| format!("{:02x}", rng.gen::<u8>())).collect()
}

fn query(url: &str, key: &str) -> Option<String> {
    url.split_once('?')?.1.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}

impl PhoneServer {
    pub fn status(&self, cfg: &Config, apk_dir: Option<&std::path::Path>) -> PhoneStatus {
        let (running, port, error) = self.running.lock().unwrap().clone();
        let now = clock::now_ts();
        let pin = self.pin.lock().unwrap();
        let pin = pin.as_ref().filter(|p| p.until > now && p.attempts < PIN_ATTEMPTS);
        let seen = self.seen.lock().unwrap();
        PhoneStatus {
            enabled: cfg.phone.enabled,
            running,
            port,
            address: running.then(crate::system::lan_ip_cached).flatten(),
            pc_name: crate::system::pc_name(),
            error,
            pin: pin.map(|p| p.code.clone()),
            pin_until: pin.map(|p| p.until),
            apk: apk_info(apk_dir),
            devices: cfg
                .phone
                .devices
                .iter()
                .map(|d| {
                    let s = seen.get(&d.id);
                    PhoneDeviceStatus {
                        id: d.id.clone(),
                        name: d.name.clone(),
                        paired_at: d.paired_at,
                        last_seen: s.map(|s| s.0),
                        blocker: s.map(|s| s.1),
                    }
                })
                .collect(),
        }
    }

    /// A fresh 6-digit pairing PIN, valid for two minutes.
    pub fn new_pin(&self) -> (String, Ts) {
        use rand::Rng;
        let code = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000));
        let until = clock::now_ts() + PIN_TTL_MS;
        *self.pin.lock().unwrap() = Some(Pin { code: code.clone(), until, attempts: 0 });
        (code, until)
    }

    pub fn stop(&self) {
        if let Some(s) = self.server.lock().unwrap().take() {
            s.unblock();
        }
        self.running.lock().unwrap().0 = false;
    }

    /// (Re)start according to the config.
    pub fn start(&self, shared: Arc<Shared>, cfg: &Config) {
        self.stop();
        let port = cfg.phone.port;
        if !cfg.phone.enabled {
            *self.running.lock().unwrap() = (false, port, None);
            return;
        }
        let server = match Server::http(("0.0.0.0", port)) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                *self.running.lock().unwrap() = (false, port, Some(format!("Порт {port} занят: {e}")));
                return;
            }
        };
        *self.running.lock().unwrap() = (true, port, None);
        *self.server.lock().unwrap() = Some(server.clone());
        thread::spawn(crate::system::allow_phone_firewall);
        if !self.discovery.swap(true, Ordering::SeqCst) {
            let s = shared.clone();
            thread::Builder::new().name("phone-discovery".into()).spawn(move || discovery(s)).expect("spawn discovery");
        }
        thread::Builder::new()
            .name("phone".into())
            .spawn(move || {
                for req in server.incoming_requests() {
                    let s = shared.clone();
                    // A long poll holds its request: one short thread per request (a phone or two).
                    thread::spawn(move || handle(&s, req));
                }
            })
            .expect("spawn phone server");
    }
}

/// Answer discovery broadcasts while the phone API is running.
fn discovery(shared: Arc<Shared>) {
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", PHONE_DISCOVERY_PORT)) else { return };
    let mut buf = [0u8; 256];
    loop {
        let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
        if &buf[..n] != b"CLOCKMANAGE_DISCOVER" {
            continue;
        }
        let (running, port, _) = shared.phone.running.lock().unwrap().clone();
        if !running {
            continue;
        }
        let msg = json!({ "app": "clockmanage", "name": crate::system::pc_name(), "port": port });
        let _ = sock.send_to(msg.to_string().as_bytes(), from);
    }
}

fn body(req: &mut Request) -> Option<Value> {
    let mut s = String::new();
    req.as_reader().take(64 * 1024).read_to_string(&mut s).ok()?;
    serde_json::from_str(&s).ok()
}

/// The paired device this request comes from.
fn device(shared: &Shared, req: &Request) -> Option<PhoneDevice> {
    let auth = req.headers().iter().find(|h| h.field.equiv("Authorization"))?.value.as_str().to_string();
    let token = auth.strip_prefix("Bearer ")?.trim();
    let g = shared.lock();
    g.cfg.phone.devices.iter().find(|d| d.token == token && !token.is_empty()).cloned()
}

fn snapshot(shared: &Shared) -> phone::PhoneSnapshot {
    let g = shared.lock();
    phone::snapshot(&g.day, &g.cfg, clock::now_ts())
}

fn handle(shared: &Arc<Shared>, mut req: Request) {
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or("").to_string();
    let method = req.method().clone();
    match (method, path.as_str()) {
        (Method::Get, "/api/hello") => {
            reply(req, 200, &json!({ "app": "clockmanage", "name": crate::system::pc_name(), "version": env!("CARGO_PKG_VERSION") }))
        }
        (Method::Post, "/api/pair") => {
            let b = body(&mut req).unwrap_or(Value::Null);
            match pair(shared, &b) {
                Ok(v) => reply(req, 200, &v),
                Err(e) => error(req, 403, &e),
            }
        }
        (m, p) if p.starts_with("/api/") => {
            let Some(dev) = device(shared, &req) else {
                return error(req, 401, "Телефон не подключён к этому ПК — подключи заново по PIN.");
            };
            match (m, p) {
                (Method::Get, "/api/state") => {
                    let blocker = query(&url, "blocker").map(|b| b == "1");
                    seen(shared, &dev, blocker);
                    let have = query(&url, "v").unwrap_or_default();
                    let wait = query(&url, "wait").and_then(|w| w.parse().ok()).unwrap_or(0u64).min(MAX_WAIT_S);
                    let deadline = std::time::Instant::now() + Duration::from_secs(wait);
                    let mut snap = snapshot(shared);
                    while snap.version == have && std::time::Instant::now() < deadline {
                        thread::sleep(Duration::from_millis(500));
                        snap = snapshot(shared);
                    }
                    reply(req, 200, &json!(snap));
                }
                (Method::Get, "/api/apk/info") => reply(req, 200, &json!(apk_info(shared.apk_dir.as_deref()))),
                (Method::Get, "/api/apk") => {
                    let info = apk_info(shared.apk_dir.as_deref());
                    let file = shared.apk_dir.as_ref().filter(|_| info.available).and_then(|d| std::fs::File::open(d.join(APK_FILE)).ok());
                    match file {
                        Some(f) => {
                            let _ = req.respond(
                                Response::from_file(f)
                                    .with_header(header("Content-Type", "application/vnd.android.package-archive"))
                                    .with_header(header("Cache-Control", "no-store")),
                            );
                        }
                        None => error(req, 404, "В этой сборке ПК нет APK для телефона."),
                    }
                }
                (Method::Post, "/api/action") => {
                    let b = body(&mut req).unwrap_or(Value::Null);
                    seen(shared, &dev, None);
                    match action(shared, &b) {
                        Ok(()) => reply(req, 200, &json!(snapshot(shared))),
                        Err(e) => error(req, 409, &e),
                    }
                }
                (Method::Post, "/api/plan") => {
                    let b = body(&mut req).unwrap_or(Value::Null);
                    match plan(shared, &b) {
                        Ok(()) => reply(req, 200, &json!(snapshot(shared))),
                        Err(e) => error(req, 409, &e),
                    }
                }
                (Method::Post, "/api/apps") => {
                    let b = body(&mut req).unwrap_or(Value::Null);
                    match apps(shared, &b) {
                        Ok(()) => reply(req, 200, &json!(snapshot(shared))),
                        Err(e) => error(req, 409, &e),
                    }
                }
                _ => error(req, 404, "Нет такого запроса."),
            }
        }
        _ => error(req, 404, "Нет такого запроса."),
    }
}

fn pair(shared: &Arc<Shared>, b: &Value) -> Result<Value, String> {
    let typed = b.get("pin").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let name: String = b.get("name").and_then(Value::as_str).unwrap_or("Телефон").trim().chars().take(40).collect();
    {
        let mut pin = shared.phone.pin.lock().unwrap();
        let p = pin.as_mut().filter(|p| p.until > clock::now_ts()).ok_or("PIN не открыт: нажми «Подключить телефон» в настройках ПК.")?;
        if p.attempts >= PIN_ATTEMPTS {
            return Err("Слишком много попыток — открой новый PIN на ПК.".into());
        }
        if p.code != typed {
            p.attempts += 1;
            return Err("PIN не подходит.".into());
        }
        *pin = None;
    }
    let dev = PhoneDevice {
        id: random_hex(6),
        name: if name.is_empty() { "Телефон".into() } else { name },
        token: random_hex(24),
        paired_at: clock::now_ts(),
    };
    shared.mutate(|g, now| {
        g.cfg.phone.devices.push(dev.clone());
        let n = g.cfg.phone.devices.len();
        if n > MAX_DEVICES {
            g.cfg.phone.devices.drain(..n - MAX_DEVICES);
        }
        shared.store.save_config(&g.cfg);
        g.day.log(now, "phone", format!("Подключён телефон «{}»", dev.name));
        Ok(())
    })?;
    let _ = shared.app.emit("config", ());
    shared.notice("Телефон подключён", &dev.name);
    Ok(json!({ "token": dev.token, "id": dev.id, "pc_name": crate::system::pc_name() }))
}

fn seen(shared: &Shared, dev: &PhoneDevice, blocker: Option<bool>) {
    let now = clock::now_ts();
    let mut seen = shared.phone.seen.lock().unwrap();
    let prev = seen.get(&dev.id).map(|s| s.1);
    let blocker = blocker.or(prev).unwrap_or(false);
    seen.insert(dev.id.clone(), (now, blocker));
    drop(seen);
    if prev.is_some_and(|p| p != blocker) {
        let mut g = shared.lock();
        let text = format!("Телефон «{}»: блокировка {}", dev.name, if blocker { "включена" } else { "выключена" });
        g.day.log(now, "phone", text);
    }
}

fn action(shared: &Arc<Shared>, b: &Value) -> Result<(), String> {
    let act = b.get("action").and_then(Value::as_str).unwrap_or("");
    let expect = b.get("expect").and_then(Value::as_str).map(str::to_string);
    let r = shared.mutate(|g, now| {
        if let Some(e) = &expect {
            if clockmanage_core::view::build(&g.day, &g.cfg, now).phase.kind != *e {
                return Err("Таймер уже перешёл дальше — ничего не сделал.".into());
            }
        }
        match act {
            "start_day" => g.day.start_day(now, &g.cfg),
            "pause" => g.day.pause(now, &g.cfg),
            "resume" => g.day.resume(now),
            "start_next" => g.day.start_next(now),
            "end_segment" => g.day.end_segment(now),
            _ => Err("Неизвестное действие.".into()),
        }
    });
    if r.is_ok() && act == "start_next" {
        crate::app::hide_overlay_window(&shared.app);
    }
    r
}

/// `{"index": 1, "minutes": 105}`: new length of one block (the phone's "+ / −").
fn plan(shared: &Arc<Shared>, b: &Value) -> Result<(), String> {
    let i = b.get("index").and_then(Value::as_u64).ok_or("Нужен index.")? as usize;
    let minutes = b.get("minutes").and_then(Value::as_u64).ok_or("Нужны minutes.")?;
    if !(1..=480).contains(&minutes) {
        return Err("Длительность — от 1 до 480 минут.".into());
    }
    let mut blocks: Vec<PlanBlock> = shared.lock().day.plan.clone();
    let blk = blocks.get_mut(i).ok_or("Нет такого блока.")?;
    blk.minutes = minutes as u32;
    crate::app::apply_plan(shared, blocks, false, "phone").map(|_| ())
}

/// `{"apps": ["org.telegram.messenger", ...]}`: phone block list (during the lock only grows).
fn apps(shared: &Arc<Shared>, b: &Value) -> Result<(), String> {
    let list: Vec<String> = b
        .get("apps")
        .and_then(Value::as_array)
        .ok_or("Нужен массив apps.")?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    shared.mutate(|g, now| {
        let mut next = g.cfg.clone();
        next.phone.apps = list.clone();
        next.normalize();
        let ctx = clockmanage_core::config::EditContext { locked: g.day.base_lock(now, &g.cfg) };
        g.cfg.check_update(&next, ctx)?;
        g.cfg = next;
        shared.store.save_config(&g.cfg);
        g.day.log(now, "phone", format!("Приложения телефона: {}", g.cfg.phone.apps.join(", ")));
        Ok(())
    })?;
    let _ = shared.app.emit("config", ());
    Ok(())
}
