//! Meals, an optional feature (off until turned on in Settings): a diet, the delivery address and
//! meal times; before each meal an agent picks restaurants and dishes of a delivery service that
//! fit, and the picks go to the Meals screen and the iPhone.
//!
//! The delivery services are listed in `SERVICES`; each one parses its own data (`crate::wolt`)
//! and is wired in by the app (`Service` in `src-tauri/src/meals.rs`).

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::clock::Ts;
use crate::i18n::tr;
use crate::relay::Link;
use crate::webpush::{Subscription, MAX_PLAINTEXT};

/// The MCP tools the agent may call: the meal ones only.
pub const AGENT_TOOLS: &str = "mcp__clockmanage__get_diet mcp__clockmanage__food_search mcp__clockmanage__food_menu mcp__clockmanage__set_meal_options";
/// The command of 0.5 builds, before the agent picker: a config holding it means Claude Code.
const OLD_DEFAULT_AGENT: &str = "claude -p --mcp-config \"{mcp_config}\" --strict-mcp-config --allowedTools \"mcp__clockmanage__get_diet mcp__clockmanage__wolt_search mcp__clockmanage__wolt_menu mcp__clockmanage__set_meal_options\"";
pub const DEFAULT_PWA: &str = "https://nikgob.github.io/ClockManage/";
/// The web app's address in builds of the Meals PR, before it moved to the owner's Pages: it
/// always turns into `DEFAULT_PWA`, so this exact address can't be chosen by hand.
const OLD_DEFAULT_PWA: &str = "https://danilt2000.github.io/ClockManage/";
pub const MAX_KM: f64 = 15.0;

/// Delivery services food can come from: (id in the config, the brand name shown). A new one
/// (Yandex Eda, Samokat, Kuper…) gets its id here and its `Service` in the app.
pub const SERVICES: [(&str, &str); 1] = [("wolt", "Wolt")];

/// The brand name of a service id.
pub fn service_name(id: &str) -> &'static str {
    SERVICES.iter().find(|s| s.0 == id).unwrap_or(&SERVICES[0]).1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meals {
    /// The whole feature: while off, nothing of it runs, polls or shows (the screen, MCP tools).
    pub enabled: bool,
    /// Where food comes from: an id of `SERVICES`.
    pub service: String,
    /// The user's diet in their own words (loaded through MCP, Settings or the iPhone).
    pub diet: String,
    pub address: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Restaurants farther than this from the address are never offered.
    pub max_km: f64,
    /// Meal times, minutes after midnight.
    pub times: Vec<u32>,
    /// How long before a meal the agent starts picking.
    pub lead_min: u32,
    /// "claude" (Claude Code, found by itself) or "custom" (`agent_cmd`).
    pub agent: String,
    /// The custom agent's shell command: the prompt comes on stdin, `{mcp_url}` is the MCP
    /// address and `{mcp_config}` a Claude-style config file.
    pub agent_cmd: String,
    /// Where the iPhone web app is hosted (the QR code opens it).
    pub pwa_url: String,
    /// VAPID private key of this PC, made on first use.
    pub vapid_key: String,
    pub link: Option<Link>,
    pub subscription: Option<Subscription>,
    /// ntfy time of the last phone message applied.
    pub relay_seen: i64,
}

/// A diet for a clear head on study days; the user's own replaces it.
pub fn default_diet() -> &'static str {
    tr("meals_goal_a_clear_steady_mind")
}

impl Default for Meals {
    fn default() -> Self {
        Self {
            enabled: false,
            service: SERVICES[0].0.into(),
            diet: default_diet().into(),
            address: String::new(),
            lat: None,
            lon: None,
            max_km: 3.0,
            times: vec![13 * 60, 19 * 60],
            lead_min: 30,
            agent: "claude".into(),
            agent_cmd: String::new(),
            pwa_url: DEFAULT_PWA.into(),
            vapid_key: String::new(),
            link: None,
            subscription: None,
            relay_seen: 0,
        }
    }
}

impl Meals {
    pub fn normalize(&mut self) {
        if self.diet.trim().is_empty() {
            self.diet = default_diet().into();
        }
        self.max_km = self.max_km.clamp(0.5, MAX_KM);
        self.lead_min = self.lead_min.clamp(5, 180);
        self.times.retain(|t| *t < 24 * 60);
        self.times.sort_unstable();
        self.times.dedup();
        if self.agent_cmd == OLD_DEFAULT_AGENT {
            self.agent_cmd.clear();
        }
        if self.agent != "custom" {
            self.agent = "claude".into();
        }
        if !SERVICES.iter().any(|s| s.0 == self.service) {
            self.service = SERVICES[0].0.into();
        }
        if self.pwa_url.trim().is_empty() || self.pwa_url == OLD_DEFAULT_PWA {
            self.pwa_url = DEFAULT_PWA.into();
        }
    }

    pub fn location(&self) -> Option<(f64, f64)> {
        Some((self.lat?, self.lon?))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Picks {
    pub at: Ts,
    /// "13:00", or empty for a pick asked for by hand.
    pub meal: String,
    /// breakfast, lunch or dinner.
    pub kind: String,
    /// The brand name of the delivery service the picks are from ("Wolt").
    pub service: String,
    pub venues: Vec<PickVenue>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PickVenue {
    pub name: String,
    /// A grocery store (products to buy), not a restaurant.
    pub store: bool,
    pub url: String,
    pub km: f64,
    pub eta: String,
    /// A photo of the venue, sized for a card.
    pub image: String,
    pub dishes: Vec<PickDish>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PickDish {
    pub name: String,
    /// In minor units of `currency` (ISO 4217: cents, kopecks; yen have none).
    pub price: i64,
    pub currency: String,
    pub url: String,
    pub why: String,
    /// A thumbnail of the dish or product.
    pub image: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Venue {
    pub slug: String,
    pub name: String,
    pub km: f64,
    pub eta: String,
    pub rating: Option<f64>,
    pub tags: Vec<String>,
    pub online: bool,
    pub url: String,
    pub currency: String,
    pub store: bool,
    pub image: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dish {
    pub id: String,
    pub venue: String,
    pub name: String,
    pub description: String,
    /// In minor units of `currency` (ISO 4217).
    pub price: i64,
    pub currency: String,
    pub available: bool,
    pub image: String,
}

/// Great-circle distance in km.
pub fn km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1, la2, lo2) = (a.0.to_radians(), a.1.to_radians(), b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    6371.0 * 2.0 * h.sqrt().asin()
}

/// Whitespace folded, cut to `max` characters with an ellipsis.
pub fn short(text: &str, max: usize) -> String {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= max {
        t
    } else {
        format!("{}…", t.chars().take(max).collect::<String>().trim_end())
    }
}

pub const KINDS: [&str; 3] = ["breakfast", "lunch", "dinner"];

/// The meal a time of day is for: breakfast before 11:00, lunch before 16:00, then dinner.
pub fn kind_at(min: u32) -> &'static str {
    if min < 11 * 60 {
        "breakfast"
    } else if min < 16 * 60 {
        "lunch"
    } else {
        "dinner"
    }
}

pub fn kind_name(kind: &str) -> &'static str {
    match kind {
        "breakfast" => tr("meals_breakfast"),
        "lunch" => tr("meals_lunch"),
        "dinner" => tr("meals_dinner"),
        _ => tr("meals_food_picks"),
    }
}

/// The title of picks: "Lunch · 13:00", or just "Breakfast" for one asked for by hand.
pub fn picks_title(p: &Picks) -> String {
    if p.meal.is_empty() {
        kind_name(&p.kind).to_string()
    } else {
        crate::tr!("meals_kind_at", kind = kind_name(&p.kind), time = p.meal)
    }
}

/// The meal to pick for now: its start is at most `lead` minutes away and it wasn't picked today.
pub fn due(times: &[u32], lead: u32, now_min: u32, picked_today: &[u32]) -> Option<u32> {
    times.iter().copied().find(|t| *t >= now_min && *t - now_min <= lead && !picked_today.contains(t))
}

/// The push for the iPhone: what the notification shows plus the picks, cut to fit one push.
pub fn push_payload(p: &Picks, title: &str) -> String {
    let body = p.venues.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join(" · ");
    let mut picks = p.clone();
    let make = |picks: &Picks| json!({ "title": title, "body": body, "picks": picks }).to_string();
    let mut out = make(&picks);
    if out.len() > MAX_PLAINTEXT {
        // The relay brings the whole picks to the web app anyway: drop pictures first.
        picks.venues.iter_mut().for_each(|v| v.image.clear());
        picks.venues.iter_mut().flat_map(|v| v.dishes.iter_mut()).for_each(|d| d.image.clear());
        out = make(&picks);
    }
    if out.len() > MAX_PLAINTEXT {
        picks.venues.iter_mut().flat_map(|v| v.dishes.iter_mut()).for_each(|d| d.why.clear());
        out = make(&picks);
    }
    while out.len() > MAX_PLAINTEXT {
        let Some(v) = picks.venues.iter_mut().max_by_key(|v| v.dishes.len()) else { break };
        if v.dishes.len() > 1 {
            v.dishes.pop();
        } else {
            picks.venues.pop();
        }
        out = make(&picks);
    }
    out
}

/// What the agent is told before a meal (English: the model reads it).
pub fn agent_prompt(meal: &str, kind: &str, lang: &str) -> String {
    let when = if meal.is_empty() { "now".to_string() } else { format!("at {meal}") };
    format!(
        "You run unattended inside ClockManage: nobody will answer questions. The user wants {kind} {when}: pick food that suits {kind}. \
         1) Call get_diet. The diet is the user's own words and the only measure of what fits. Where it contradicts itself, \
         a specific rule wins over a general one; otherwise what the user excludes or limits wins over what it recommends. \
         Judge a grocery product as you judge a dish: if the user doesn't want a food on its own, don't offer a pack of just that food. \
         2) Find food that fits the diet with food_search (search dishes or cuisines; try several queries) \
         and look at promising restaurants with food_menu. Only restaurants these tools return deliver to the user and are close enough. \
         3) Choose up to 5 restaurants that are open now, and in each 1-4 available dishes that together make a meal fitting the diet. \
         Menus have no nutrition data: judge by names and descriptions, and prefer dishes whose fit is clear. \
         Be economical: at most 20 tool calls in all; one food_search per dish or product idea, and food_menu only for a restaurant you are about to pick. \
         Never pick drinks of any kind (water, juice, soda, coffee, tea, smoothies): the user buys their own. \
         Also choose up to 5 grocery stores (store: true in food_search) and in each 4-10 products to buy for the next days \
         that the diet calls for: search for them by name with food_search. \
         4) Before saving, check every dish and product against every rule of the diet and drop the ones that break any. \
         5) Call set_meal_options exactly once with your picks, restaurants and stores together; write each `why` in one short line in this language: {lang}. Then stop."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn meals_are_off_until_turned_on_and_the_service_is_known() {
        let mut m = Meals::default();
        assert!(!m.enabled);
        assert_eq!((m.service.as_str(), service_name(&m.service)), ("wolt", "Wolt"));
        m.service = "nowhere".into();
        m.normalize();
        assert_eq!(m.service, "wolt");
        let old: Meals = serde_json::from_str(r#"{ "diet": "x" }"#).unwrap();
        assert!(!old.enabled);
    }

    #[test]
    fn the_old_default_command_means_claude_code() {
        let mut m = Meals { agent: String::new(), agent_cmd: OLD_DEFAULT_AGENT.into(), ..Meals::default() };
        m.normalize();
        assert_eq!((m.agent.as_str(), m.agent_cmd.as_str()), ("claude", ""));
        let mut m = Meals { agent: "custom".into(), agent_cmd: "codex exec -".into(), ..Meals::default() };
        m.normalize();
        assert_eq!((m.agent.as_str(), m.agent_cmd.as_str()), ("custom", "codex exec -"));
    }

    #[test]
    fn the_old_web_app_address_moves_to_the_owners_pages() {
        let mut m = Meals { pwa_url: OLD_DEFAULT_PWA.into(), ..Meals::default() };
        m.normalize();
        assert_eq!(m.pwa_url, DEFAULT_PWA);
        let mut m = Meals { pwa_url: "https://example.org/cm/".into(), ..Meals::default() };
        m.normalize();
        assert_eq!(m.pwa_url, "https://example.org/cm/");
    }

    #[test]
    fn kinds_by_time_of_day() {
        assert_eq!([kind_at(8 * 60), kind_at(11 * 60), kind_at(15 * 60 + 59), kind_at(19 * 60)], ["breakfast", "lunch", "lunch", "dinner"]);
        assert!(agent_prompt("", "breakfast", "ru").contains("wants breakfast now"));
        assert!(agent_prompt("13:00", "lunch", "en").contains("Never pick drinks"));
    }

    #[test]
    fn the_diet_alone_says_what_to_buy() {
        let p = agent_prompt("13:00", "lunch", "en");
        assert!(!p.contains("leafy greens") && !p.contains("eggs"), "{p}");
        assert!(p.contains("excludes or limits wins over what it recommends"));
    }

    #[test]
    fn due_once_per_meal_in_the_lead_window() {
        let times = [13 * 60, 19 * 60];
        assert_eq!(due(&times, 30, 12 * 60 + 29, &[]), None);
        assert_eq!(due(&times, 30, 12 * 60 + 30, &[]), Some(780));
        assert_eq!(due(&times, 30, 12 * 60 + 59, &[780]), None);
        assert_eq!(due(&times, 30, 13 * 60 + 1, &[]), None);
        assert_eq!(due(&times, 30, 18 * 60 + 45, &[780]), Some(1140));
    }

    #[test]
    fn push_fits_one_message() {
        let dish = PickDish { name: "Блюдо с длинным названием".repeat(3), price: 1290, currency: "EUR".into(), url: format!("https://wolt.com/en/fin/helsinki/restaurant/{}/itemid-0123456789abcdef01234567", "x".repeat(40)), why: "Белок и овощи, без сахара".repeat(2), image: format!("https://wolt-menu-images-cdn.wolt.com/menu-images/{}.jpg?w=160", "x".repeat(60)) };
        let venue = PickVenue { name: "Ресторан".into(), store: false, url: "https://wolt.com/en/fin/helsinki/restaurant/x".into(), km: 1.2, eta: "20-30".into(), image: "https://imageproxy.wolt.com/assets/x?w=600".into(), dishes: vec![dish; 6] };
        let p = Picks { at: 1, meal: "13:00".into(), kind: "lunch".into(), service: "Wolt".into(), venues: vec![venue; 4] };
        let out = push_payload(&p, "Обед");
        assert!(out.len() <= MAX_PLAINTEXT, "{}", out.len());
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["title"], "Обед");
        assert!(!v["picks"]["venues"].as_array().unwrap().is_empty());
        let small = Picks { venues: vec![p.venues[0].clone()], ..p.clone() };
        assert!(push_payload(&Picks { venues: vec![PickVenue { dishes: vec![small.venues[0].dishes[0].clone()], ..small.venues[0].clone() }], ..small }, "t").contains("Белок"));
    }
}
