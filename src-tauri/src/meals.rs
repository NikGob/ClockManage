//! Meals: Wolt data for the agent, the agent run before each meal, the picks, and the paired
//! iPhone web app (Web Push out, ntfy.sh relay for its subscription and diet edits).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use clockmanage_core::chrono::NaiveDate;
use clockmanage_core::clock::{self, Ts};
use clockmanage_core::mcp::MealPick;
use clockmanage_core::meals::{self, Dish, PickDish, PickVenue, Picks, Venue};
use clockmanage_core::relay::{self, NTFY};
use clockmanage_core::webpush::{self, Subscription};
use clockmanage_core::i18n::tr;
use clockmanage_core::tr;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::Emitter;
use tauri_plugin_notification::NotificationExt;

use crate::app::Shared;

const CACHE_MS: Ts = 10 * 60_000;
const AGENT_TIMEOUT: Duration = Duration::from_secs(600);
const PAIR_MS: Ts = 10 * 60_000;
const VAPID_SUBJECT: &str = "https://github.com/NikGob/ClockManage";

#[derive(Default)]
pub struct MealsRt {
    picks: Option<Picks>,
    /// The run now: its meal time ("" by hand), kind, and whether the iPhone asked for it.
    running: Option<(String, String, bool)>,
    error: Option<String>,
    pairing: Option<(String, Ts)>,
    venues: Option<(Ts, (f64, f64, f64), Vec<Venue>)>,
    dishes: HashMap<String, Dish>,
    published: Option<(String, Ts)>,
}

#[derive(Serialize)]
pub struct PairView {
    pub code: String,
    pub url: String,
    pub qr: String,
    pub until: Ts,
}

#[derive(Serialize)]
pub struct MealsView {
    /// Where Claude Code was found; `None` when it isn't installed.
    pub claude: Option<String>,
    pub picks: Option<Picks>,
    pub running: bool,
    pub error: Option<String>,
    pub paired: bool,
    pub pairing: Option<PairView>,
}

fn rt(shared: &Shared) -> std::sync::MutexGuard<'_, MealsRt> {
    shared.meals.lock().unwrap_or_else(|e| e.into_inner())
}

fn http() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::blocking::Client::builder()
            .user_agent(concat!("ClockManage/", env!("CARGO_PKG_VERSION"), " (+https://github.com/NikGob/ClockManage)"))
            .timeout(Duration::from_secs(25))
            .build()
            .expect("http client")
    })
}

fn get_json(req: reqwest::blocking::RequestBuilder) -> Result<Value, String> {
    let res = req.send().map_err(|e| tr!("meals_wolt_didnt_answer", error = e))?;
    if !res.status().is_success() {
        return Err(tr!("meals_wolt_didnt_answer", error = res.status()));
    }
    res.json().map_err(|e| tr!("meals_wolt_didnt_answer", error = e))
}

fn picks_path(shared: &Shared) -> std::path::PathBuf {
    shared.store.dir.join("meals.json")
}

fn emit(shared: &Shared) {
    let _ = shared.app.emit("meals", ());
}

fn save_cfg(shared: &Shared, f: impl FnOnce(&mut clockmanage_core::meals::Meals)) {
    {
        let mut g = shared.lock();
        f(&mut g.cfg.meals);
        shared.store.save_config(&g.cfg);
    }
    let _ = shared.app.emit("config", ());
}

pub fn start(shared: Arc<Shared>) {
    rt(&shared).picks = crate::store::read_json(&picks_path(&shared));
    let _ = std::thread::Builder::new().name("meals".into()).spawn(move || {
        let mut last_poll: Ts = 0;
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let now = clock::now_ts();
            if let Some(meal) = due_meal(&shared, now) {
                let _ = run_agent(shared.clone(), Some(meal), meals::kind_at(meal), false);
            }
            let linked = shared.lock().cfg.meals.link.is_some();
            let pairing = rt(&shared).pairing.is_some();
            if linked && now - last_poll >= if pairing { 5_000 } else { 15_000 } {
                last_poll = now;
                if let Err(e) = poll_phone(&shared) {
                    eprintln!("meals relay: {e}");
                }
                publish_diet(&shared, now);
            }
            let mut r = rt(&shared);
            if r.pairing.as_ref().is_some_and(|p| p.1 < now) {
                r.pairing = None;
                drop(r);
                emit(&shared);
            }
        }
    });
}

fn due_meal(shared: &Shared, now: Ts) -> Option<u32> {
    let (times, lead, tz, ready) = {
        let g = shared.lock();
        let m = &g.cfg.meals;
        (m.times.clone(), m.lead_min, g.cfg.tz_offset_min, !m.diet.trim().is_empty() && m.location().is_some())
    };
    let r = rt(shared);
    if !ready || r.running.is_some() {
        return None;
    }
    let today = clock::local_date(now, tz);
    let picked: Vec<u32> = r
        .picks
        .iter()
        .filter(|p| clock::local_date(p.at, tz) == today)
        .filter_map(|p| clock::parse_hm(&p.meal))
        .collect();
    let tried: Vec<u32> = TRIED.lock().unwrap().iter().filter(|(d, _)| *d == today).map(|(_, m)| *m).collect();
    meals::due(&times, lead, clock::minute_of_day(now, tz), &[picked, tried].concat())
}

/// Meals already tried today (a failed run is not retried every few seconds).
static TRIED: Mutex<Vec<(NaiveDate, u32)>> = Mutex::new(Vec::new());

pub fn view(shared: &Shared) -> MealsView {
    let paired = shared.lock().cfg.meals.subscription.is_some();
    let url_base = shared.lock().cfg.meals.pwa_url.clone();
    let r = rt(shared);
    MealsView {
        claude: find_claude().map(|p| p.display().to_string()),
        picks: r.picks.clone(),
        running: r.running.is_some(),
        error: r.error.clone(),
        paired,
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

// ---- Wolt ----

fn nearby(shared: &Shared) -> Result<Vec<Venue>, String> {
    let (at, max_km) = {
        let g = shared.lock();
        (g.cfg.meals.location().ok_or(tr("meals_set_the_address_first"))?, g.cfg.meals.max_km)
    };
    let key = (at.0, at.1, max_km);
    let now = clock::now_ts();
    if let Some((t, k, v)) = &rt(shared).venues {
        if now - t < CACHE_MS && *k == key {
            return Ok(v.clone());
        }
    }
    let page = get_json(http().get(format!("https://consumer-api.wolt.com/v1/pages/restaurants?lat={}&lon={}", at.0, at.1)))?;
    let mut list = meals::parse_venues(&page, at, max_km);
    // Stores are not on the restaurants page; a store list that fails leaves just the restaurants.
    if let Ok(page) = get_json(http().get(format!("https://consumer-api.wolt.com/v1/pages/venue-list/category-grocery?lat={}&lon={}", at.0, at.1))) {
        let stores: Vec<Venue> = meals::parse_venues(&page, at, max_km).into_iter().filter(|v| v.store && !list.iter().any(|r| r.slug == v.slug)).collect();
        list.extend(stores);
    }
    rt(shared).venues = Some((now, key, list.clone()));
    Ok(list)
}

pub fn wolt_search(shared: &Shared, query: Option<&str>) -> Result<Value, String> {
    let venues = nearby(shared)?;
    let Some(q) = query else {
        let list = |store: bool, n: usize| venues.iter().filter(|v| v.store == store && v.online).take(n).map(|v| json!({
            "slug": v.slug, "name": v.name, "km": v.km, "eta": v.eta, "rating": v.rating, "tags": v.tags.iter().take(3).collect::<Vec<_>>()
        })).collect::<Vec<_>>();
        return Ok(json!({ "restaurants": list(false, 40), "stores": list(true, 15) }));
    };
    let at = shared.lock().cfg.meals.location().ok_or(tr("meals_set_the_address_first"))?;
    let page = get_json(http().post("https://restaurant-api.wolt.com/v1/pages/search").json(&json!({ "q": q, "target": "items", "lat": at.0, "lon": at.1 })))?;
    let near: HashMap<&str, &Venue> = venues.iter().map(|v| (v.slug.as_str(), v)).collect();
    // Results add up in the agent's context over many searches: few per venue, venue facts once.
    let mut per_venue: HashMap<String, usize> = HashMap::new();
    let found: Vec<Dish> = meals::parse_search(&page)
        .into_iter()
        .filter(|d| d.available && near.get(d.venue.as_str()).is_some_and(|v| v.online))
        .filter(|d| {
            let n = per_venue.entry(d.venue.clone()).or_default();
            *n += 1;
            *n <= 3
        })
        .take(20)
        .collect();
    let mut seen: HashMap<&str, Value> = HashMap::new();
    for d in &found {
        let v = near[d.venue.as_str()];
        seen.entry(v.slug.as_str()).or_insert_with(|| json!({ "name": v.name, "store": v.store, "km": v.km, "eta": v.eta }));
    }
    let items: Vec<Value> = found
        .iter()
        .map(|d| json!({ "id": d.id, "venue": d.venue, "name": d.name, "description": shorter(&d.description), "price": d.price }))
        .collect();
    let currency = found.first().map(|d| d.currency.clone()).unwrap_or_default();
    let venues_out = json!(seen);
    let mut r = rt(shared);
    for d in found {
        r.dishes.insert(d.id.clone(), d);
    }
    Ok(json!({ "query": q, "currency": currency, "venues": venues_out, "items": items }))
}

/// Descriptions for the agent: enough to judge a dish, short enough for dozens of results.
fn shorter(text: &str) -> String {
    if text.chars().count() <= 90 {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(90).collect::<String>().trim_end())
    }
}

pub fn wolt_menu(shared: &Shared, slug: &str) -> Result<Value, String> {
    let max_km = shared.lock().cfg.meals.max_km;
    let venues = nearby(shared)?;
    let v = venues.iter().find(|v| v.slug == slug).ok_or_else(|| tr!("meals_no_such_restaurant_nearby", venue = slug, km = max_km))?;
    let page = get_json(http().get(format!("https://consumer-api.wolt.com/consumer-api/consumer-assortment/v1/venues/slug/{slug}/assortment")))?;
    let dishes: Vec<Dish> = meals::parse_menu(&page, slug, &v.currency).into_iter().filter(|d| d.available).take(60).collect();
    if dishes.is_empty() {
        return Ok(json!({ "venue": v.slug, "name": v.name, "dishes": [], "note": tr("meals_no_menu_here_grocery_stores_list") }));
    }
    let out: Vec<Value> = dishes.iter().map(|d| json!({ "id": d.id, "name": d.name, "description": shorter(&d.description), "price": d.price })).collect();
    let mut r = rt(shared);
    for d in dishes {
        r.dishes.insert(d.id.clone(), d);
    }
    Ok(json!({ "venue": v.slug, "name": v.name, "km": v.km, "eta": v.eta, "online": v.online, "currency": v.currency, "dishes": out }))
}

pub fn set_meal_options(shared: &Shared, picks: Vec<MealPick>) -> Result<Value, String> {
    let max_km = shared.lock().cfg.meals.max_km;
    let venues = nearby(shared)?;
    let mut r = rt(shared);
    let mut out = Vec::new();
    let (mut restaurants, mut stores) = (0, 0);
    for p in &picks {
        let v = venues.iter().find(|v| v.slug == p.venue).ok_or_else(|| tr!("meals_no_such_restaurant_nearby", venue = p.venue, km = max_km))?;
        let mut dishes = Vec::new();
        for (id, why) in &p.dishes {
            let d = r.dishes.get(id).filter(|d| d.venue == v.slug).ok_or_else(|| tr!("meals_unknown_dish", id = id))?;
            dishes.push(PickDish {
                name: d.name.clone(),
                price: d.price,
                currency: d.currency.clone(),
                url: meals::dish_url(&v.url, &d.id),
                why: why.clone(),
                image: meals::sized(&d.image, 160),
            });
        }
        let (count, max) = if v.store { (&mut stores, 5) } else { (&mut restaurants, 5) };
        *count += 1;
        if *count > max {
            continue;
        }
        out.push(PickVenue { name: v.name.clone(), store: v.store, url: v.url.clone(), km: v.km, eta: v.eta.clone(), image: meals::sized(&v.image, 600), dishes });
    }
    let (meal, kind) = match &r.running {
        Some((meal, kind, _)) => (meal.clone(), kind.clone()),
        None => (String::new(), now_kind(shared).to_string()),
    };
    let picks = Picks { at: clock::now_ts(), meal, kind, venues: out };
    r.picks = Some(picks.clone());
    r.error = None;
    drop(r);
    let _ = crate::store::write_atomic(&picks_path(shared), &serde_json::to_string_pretty(&picks).unwrap_or_default());
    publish_picks(shared, &picks);
    emit(shared);
    let title = meals::picks_title(&picks);
    let body = picks.venues.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join(" · ");
    let _ = shared.app.notification().builder().title(&title).body(&body).show();
    let pushed = match push(shared, &meals::push_payload(&picks, &title)) {
        Ok(sent) => sent,
        Err(e) => {
            rt(shared).error = Some(e.clone());
            emit(shared);
            return Ok(json!({ "shown": true, "pushed": false, "push_error": e }));
        }
    };
    Ok(json!({ "shown": true, "pushed": pushed, "restaurants": picks.venues.len() }))
}

// ---- the agent ----

fn now_kind(shared: &Shared) -> &'static str {
    let tz = shared.lock().cfg.tz_offset_min;
    meals::kind_at(clock::minute_of_day(clock::now_ts(), tz))
}

/// A notification on the iPhone that its pick failed (it would wait for nothing otherwise).
fn push_failure(shared: &Shared, error: &str) {
    let _ = push(shared, &json!({ "title": tr("meals_couldnt_pick_food"), "body": error }).to_string());
}

pub fn run_agent(shared: Arc<Shared>, meal: Option<u32>, kind: &str, from_phone: bool) -> Result<(), String> {
    let (cmd, ready_err, tz) = {
        let g = shared.lock();
        let m = &g.cfg.meals;
        let err = if m.diet.trim().is_empty() {
            Some(tr("meals_write_the_diet_first"))
        } else if m.location().is_none() {
            Some(tr("meals_set_the_address_first"))
        } else {
            None
        };
        (m.clone(), err, g.cfg.tz_offset_min)
    };
    let label = meal.map(clockmanage_core::config::fmt_hm).unwrap_or_default();
    if let Some(m) = meal {
        TRIED.lock().unwrap().push((clock::local_date(clock::now_ts(), tz), m));
    }
    let fail = |shared: &Shared, e: String| {
        rt(shared).error = Some(e.clone());
        emit(shared);
        if from_phone {
            push_failure(shared, &e);
        }
        Err(e)
    };
    if let Some(e) = ready_err {
        return fail(&shared, e.to_string());
    }
    let mcp = shared.mcp.status.lock().unwrap().clone();
    if !mcp.running {
        return fail(&shared, tr("meals_turn_on_the_mcp_server").to_string());
    }
    {
        let mut r = rt(&shared);
        if r.running.is_some() {
            drop(r);
            if from_phone {
                push_failure(&shared, tr("meals_the_agent_is_already_picking"));
            }
            return Err(tr("meals_the_agent_is_already_picking").to_string());
        }
        r.running = Some((label.clone(), kind.to_string(), from_phone));
        r.error = None;
    }
    emit(&shared);
    let config = shared.store.dir.join("meals-mcp.json");
    let _ = crate::store::write_atomic(&config, &json!({ "mcpServers": { "clockmanage": { "type": "http", "url": mcp.url } } }).to_string());
    let line = match agent_line(&cmd, &config.display().to_string(), &mcp.url) {
        Ok(line) => line,
        Err(e) => {
            rt(&shared).running = None;
            return fail(&shared, e);
        }
    };
    let prompt = meals::agent_prompt(&label, kind, clockmanage_core::i18n::get().key());
    std::thread::spawn(move || {
        let before = rt(&shared).picks.as_ref().map(|p| p.at);
        let res = spawn_and_wait(&line, &prompt);
        let mut r = rt(&shared);
        r.running = None;
        let got = r.picks.as_ref().map(|p| p.at) != before;
        let error = (!got).then(|| match res {
            Err(e) => e,
            Ok(out) => tr!("meals_the_agent_ended_without_picks", output = out),
        });
        if let Some(e) = &error {
            r.error = Some(e.clone());
        }
        drop(r);
        emit(&shared);
        if let (Some(e), true) = (error, from_phone) {
            push_failure(&shared, &e);
        }
    });
    Ok(())
}

/// Claude Code on PATH, or where its installers put it.
fn find_claude() -> Option<std::path::PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for (var, sub) in [("APPDATA", "npm"), ("USERPROFILE", ".local/bin"), ("HOME", ".local/bin"), ("HOME", ".npm-global/bin")] {
        if let Some(base) = std::env::var_os(var) {
            dirs.push(std::path::PathBuf::from(base).join(sub));
        }
    }
    dirs.iter().flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| p.is_file())
}

/// The shell line of the chosen agent. Claude Code runs on its own login (`claude`, then
/// `/login`) without the user's settings, hooks and plugins, and may call only the meal tools.
fn agent_line(m: &clockmanage_core::meals::Meals, config: &str, url: &str) -> Result<String, String> {
    if m.agent == "custom" {
        if m.agent_cmd.trim().is_empty() {
            return Err(tr("meals_write_the_agent_command_first").to_string());
        }
        return Ok(m.agent_cmd.replace("{mcp_config}", config).replace("{mcp_url}", url));
    }
    let claude = find_claude().ok_or(tr("meals_claude_code_isnt_found_install_it"))?;
    Ok(format!(
        "\"{}\" -p --setting-sources project,local --mcp-config \"{config}\" --strict-mcp-config --allowedTools \"{}\"",
        claude.display(),
        meals::AGENT_TOOLS
    ))
}

fn shell(line: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd");
        c.raw_arg(format!("/S /C \"{line}\"")).creation_flags(0x0800_0000);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new("sh");
        c.arg("-c").arg(line);
        c
    }
}

/// Runs the agent with the prompt on stdin; the tail of its output when it ends by itself.
fn spawn_and_wait(line: &str, prompt: &str) -> Result<String, String> {
    let mut child = shell(line)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| tr!("meals_couldnt_start_the_agent", error = e))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes());
    }
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_string(&mut s);
            }
            s
        })
    };
    let out = read(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = read(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let started = Instant::now();
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }
        if started.elapsed() > AGENT_TIMEOUT {
            let _ = child.kill();
            return Err(tr("meals_the_agent_took_too_long").to_string());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let text = format!("{}\n{}", out.join().unwrap_or_default(), err.join().unwrap_or_default());
    let tail: String = text.trim().chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
    Ok(tail)
}

// ---- the iPhone ----

fn vapid_key(shared: &Shared) -> String {
    let key = shared.lock().cfg.meals.vapid_key.clone();
    if !key.is_empty() {
        return key;
    }
    let key = webpush::new_key();
    let k = key.clone();
    save_cfg(shared, |m| m.vapid_key = k);
    key
}

/// Sends a push to the paired iPhone; `Ok(false)` when there is none.
fn push(shared: &Shared, payload: &str) -> Result<bool, String> {
    let Some(sub) = shared.lock().cfg.meals.subscription.clone() else { return Ok(false) };
    let key = vapid_key(shared);
    let body = webpush::encrypt(&sub, payload.as_bytes()).map_err(|e| tr!("meals_couldnt_send_to_the_iphone", error = e))?;
    let auth = webpush::vapid_auth(&key, &sub.endpoint, VAPID_SUBJECT, clock::now_ts() / 1000).map_err(|e| tr!("meals_couldnt_send_to_the_iphone", error = e))?;
    let res = http()
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
            save_cfg(shared, |m| m.subscription = None);
            Err(tr("meals_the_iphone_turned_notifications_off").to_string())
        }
        code => Err(tr!("meals_couldnt_send_to_the_iphone", error = format!("HTTP {code} {}", res.text().unwrap_or_default().chars().take(120).collect::<String>()))),
    }
}

fn post_sealed(topic: &str, sealed: &str) -> Result<(), String> {
    for part in relay::parts(sealed) {
        let res = http().post(format!("{NTFY}/{topic}")).body(part).send().map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Err(format!("ntfy: HTTP {}", res.status()));
        }
    }
    Ok(())
}

/// A new pairing: a fresh link, posted under the code's topic for the web app to pick up.
pub fn pair(shared: &Arc<Shared>) -> PairView {
    let link = relay::new_link();
    let code = relay::pair_code();
    let vapid = webpush::public_key(&vapid_key(shared)).unwrap_or_default();
    let l = link.clone();
    save_cfg(shared, |m| {
        m.link = Some(l);
        m.subscription = None;
        m.relay_seen = clock::now_ts() / 1000;
    });
    {
        let mut r = rt(shared);
        r.pairing = Some((code.clone(), clock::now_ts() + PAIR_MS));
        r.published = None;
    }
    let msg = json!({ "up": link.up, "down": link.down, "key": link.key, "vapid": vapid, "pc": crate::system::pc_name() }).to_string();
    let sealed = relay::seal(&relay::pair_key(&code), msg.as_bytes());
    let topic = relay::pair_topic(&code);
    let s = shared.clone();
    std::thread::spawn(move || {
        if let Err(e) = post_sealed(&topic, &sealed) {
            rt(&s).error = Some(tr!("meals_couldnt_send_to_the_iphone", error = e));
            emit(&s);
        }
        publish_diet(&s, clock::now_ts());
    });
    emit(shared);
    view(shared).pairing.expect("pairing just set")
}

pub fn unpair(shared: &Shared) {
    save_cfg(shared, |m| {
        m.link = None;
        m.subscription = None;
    });
    rt(shared).pairing = None;
    emit(shared);
}

/// The whole picks for the web app: a push holds 4 KB, so it may carry fewer of them.
fn publish_picks(shared: &Shared, picks: &Picks) {
    let Some(link) = shared.lock().cfg.meals.link.clone() else { return };
    let Ok(key) = webpush::unb64(&link.key) else { return };
    let sealed = relay::seal(&key, json!({ "t": "picks", "picks": picks }).to_string().as_bytes());
    let _ = post_sealed(&link.down, &sealed);
}

/// Sends the diet to the web app when it changed, and every 10 hours (ntfy keeps messages 12 h).
fn publish_diet(shared: &Shared, now: Ts) {
    let (link, diet) = {
        let g = shared.lock();
        (g.cfg.meals.link.clone(), g.cfg.meals.diet.clone())
    };
    let Some(link) = link else { return };
    if rt(shared).published.as_ref().is_some_and(|(d, at)| *d == diet && now - at < 10 * 3_600_000) {
        return;
    }
    let Ok(key) = webpush::unb64(&link.key) else { return };
    let sealed = relay::seal(&key, json!({ "t": "diet", "text": diet, "at": now }).to_string().as_bytes());
    if post_sealed(&link.down, &sealed).is_ok() {
        rt(shared).published = Some((diet, now));
    }
}

/// Applies what the web app sent: its push subscription, a diet edit.
fn poll_phone(shared: &Arc<Shared>) -> Result<(), String> {
    let (link, seen) = {
        let g = shared.lock();
        (g.cfg.meals.link.clone(), g.cfg.meals.relay_seen)
    };
    let Some(link) = link else { return Ok(()) };
    let key = webpush::unb64(&link.key)?;
    let text = http().get(format!("{NTFY}/{}/json?poll=1&since=12h", link.up)).send().and_then(|r| r.text()).map_err(|e| e.to_string())?;
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
        save_cfg(shared, |m| m.relay_seen = time);
        let Some(msg) = msg else { continue };
        match msg["t"].as_str() {
            Some("sub") => {
                let Ok(sub) = serde_json::from_value::<Subscription>(msg["sub"].clone()) else { continue };
                save_cfg(shared, |m| m.subscription = Some(sub));
                rt(shared).pairing = None;
                emit(shared);
                let title = tr("meals_iphone_connected");
                let body = tr("meals_meal_picks_will_come_to_it");
                let _ = shared.app.notification().builder().title(title).body(body).show();
                let hello = json!({ "title": title, "body": body }).to_string();
                if let Err(e) = push(shared, &hello) {
                    rt(shared).error = Some(e);
                    emit(shared);
                }
            }
            Some("pick") => {
                if let Some(kind) = meals::KINDS.into_iter().find(|k| msg["kind"].as_str() == Some(*k)) {
                    let _ = run_agent(shared.clone(), None, kind, true);
                }
            }
            Some("diet") => {
                let text: String = msg["text"].as_str().unwrap_or("").chars().take(8000).collect();
                save_cfg(shared, |m| m.diet = text);
                emit(shared);
            }
            _ => {}
        }
    }
    Ok(())
}

// ---- address ----

/// "60.17, 24.94" as it is, anything else through OpenStreetMap's geocoder.
pub fn geocode(address: &str) -> Result<(f64, f64, String), String> {
    let nums: Vec<f64> = address.split([',', ' ', ';']).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
    if nums.len() == 2 && address.chars().all(|c| c.is_ascii_digit() || " .,;-+".contains(c)) {
        if nums[0].abs() <= 90.0 && nums[1].abs() <= 180.0 {
            return Ok((nums[0], nums[1], address.trim().to_string()));
        }
    }
    let url = reqwest::Url::parse_with_params("https://nominatim.openstreetmap.org/search", &[("q", address), ("format", "jsonv2"), ("limit", "1")]).map_err(|e| e.to_string())?;
    let list = get_json(http().get(url))?;
    let first = list.get(0).ok_or(tr("meals_address_not_found"))?;
    let num = |k: &str| first[k].as_str().and_then(|s| s.parse::<f64>().ok());
    match (num("lat"), num("lon")) {
        (Some(lat), Some(lon)) => Ok((lat, lon, first["display_name"].as_str().unwrap_or(address).to_string())),
        _ => Err(tr("meals_address_not_found").to_string()),
    }
}

// ---- commands ----

type S<'a> = tauri::State<'a, Arc<Shared>>;

#[tauri::command]
pub fn meals_state(s: S) -> MealsView {
    view(&s)
}

#[tauri::command]
pub fn meals_pick_now(s: S) -> Result<(), String> {
    run_agent(s.inner().clone(), None, now_kind(&s), false)
}

#[tauri::command]
pub fn meals_pair(s: S) -> PairView {
    pair(s.inner())
}

#[tauri::command]
pub fn meals_unpair(s: S) {
    unpair(&s);
}

#[tauri::command]
pub async fn meals_geocode(address: String) -> Result<Value, String> {
    let (lat, lon, label) = tauri::async_runtime::spawn_blocking(move || geocode(&address)).await.map_err(|e| e.to_string())??;
    Ok(json!({ "lat": lat, "lon": lon, "label": label }))
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    open_link(&url)
}

pub fn open_link(url: &str) -> Result<(), String> {
    if !url.starts_with("https://wolt.com/") || url.contains(['"', '\'', ' ', '\n']) {
        return Err(tr("meals_only_wolt_links_open_from_here").to_string());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).creation_flags(0x0800_0000).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    Command::new("xdg-open").arg(url).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
