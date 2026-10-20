//! Meals: a diet, the delivery address and meal times; before each meal an agent picks Wolt
//! restaurants and dishes that fit, and the picks go to the Meals screen and the iPhone.
//!
//! Wolt has no public API for customers: the JSON parsed here comes from the endpoints wolt.com
//! itself uses, read without an account.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::clock::Ts;
use crate::i18n::tr;
use crate::relay::Link;
use crate::webpush::{Subscription, MAX_PLAINTEXT};

/// The MCP tools the agent may call: the meal ones only.
pub const AGENT_TOOLS: &str = "mcp__clockmanage__get_diet mcp__clockmanage__wolt_search mcp__clockmanage__wolt_menu mcp__clockmanage__set_meal_options";
/// The command of 0.5 builds, before the agent picker: a config holding it means Claude Code.
const OLD_DEFAULT_AGENT: &str = "claude -p --mcp-config \"{mcp_config}\" --strict-mcp-config --allowedTools \"mcp__clockmanage__get_diet mcp__clockmanage__wolt_search mcp__clockmanage__wolt_menu mcp__clockmanage__set_meal_options\"";
pub const DEFAULT_PWA: &str = "https://danilt2000.github.io/ClockManage/";
pub const MAX_KM: f64 = 15.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meals {
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
        if self.pwa_url.trim().is_empty() {
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

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn short(text: &str, max: usize) -> String {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= max {
        t
    } else {
        format!("{}…", t.chars().take(max).collect::<String>().trim_end())
    }
}

/// Venues of `/v1/pages/restaurants` (or the grocery list) that deliver to `at` and are within
/// `max_km`, nearest first.
pub fn parse_venues(page: &Value, at: (f64, f64), max_km: f64) -> Vec<Venue> {
    // The restaurants page has `city`; the grocery list only `city_data.slug`.
    let city = Some(s(page, "city")).filter(|c| !c.is_empty()).unwrap_or_else(|| s(&page["city_data"], "slug"));
    let mut out: Vec<Venue> = Vec::new();
    for item in page["sections"].as_array().into_iter().flatten().flat_map(|sec| sec["items"].as_array().into_iter().flatten()) {
        let v = &item["venue"];
        let slug = s(v, "slug");
        let (Some(lon), Some(lat)) = (v["location"][0].as_f64(), v["location"][1].as_f64()) else { continue };
        if slug.is_empty() || v["delivers"] != json!(true) || out.iter().any(|o| o.slug == slug) {
            continue;
        }
        let d = km(at, (lat, lon));
        if d > max_km {
            continue;
        }
        let kind = if s(v, "product_line") == "restaurant" { "restaurant" } else { "venue" };
        out.push(Venue {
            url: format!("https://wolt.com/en/{}/{}/{kind}/{slug}", s(v, "country").to_lowercase(), city),
            slug,
            name: s(v, "name"),
            km: (d * 10.0).round() / 10.0,
            eta: s(v, "estimate_range"),
            rating: v["rating"]["score"].as_f64(),
            tags: v["tags"].as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect(),
            online: v["online"] == json!(true),
            currency: s(v, "currency"),
            store: kind == "venue",
            image: s(&item["image"], "url"),
        });
    }
    out.sort_by(|a, b| a.km.total_cmp(&b.km));
    out
}

/// Dishes of `/v1/pages/search` (target "items").
pub fn parse_search(page: &Value) -> Vec<Dish> {
    page["sections"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|sec| sec["items"].as_array().into_iter().flatten())
        .filter_map(|item| {
            let m = &item["menu_item"];
            let d = &item["link"]["menu_item_details"];
            let id = s(m, "id");
            (!id.is_empty()).then(|| Dish {
                id,
                venue: s(d, "venue_slug"),
                name: s(m, "name"),
                description: short(&s(d, "description"), 140),
                price: m["price"].as_i64().unwrap_or(0),
                currency: s(m, "currency"),
                available: m["is_available"] != json!(false),
                image: s(&m["image"], "url"),
            })
        })
        .collect()
}

/// Dishes of a venue's `/consumer-assortment/v1/venues/slug/{slug}/assortment`.
pub fn parse_menu(page: &Value, slug: &str, currency: &str) -> Vec<Dish> {
    page["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = s(m, "id");
            (!id.is_empty()).then(|| Dish {
                id,
                venue: slug.to_string(),
                name: s(m, "name"),
                description: short(&s(m, "description"), 140),
                price: m["price"].as_i64().unwrap_or(0),
                currency: currency.to_string(),
                available: m["disabled_info"].is_null(),
                image: s(&m["images"][0], "url"),
            })
        })
        .collect()
}

/// Wolt's image CDNs resize by `?w=`: a phone gets kilobytes, not a full photo.
pub fn sized(url: &str, width: u32) -> String {
    if url.is_empty() {
        String::new()
    } else {
        format!("{url}{}w={width}", if url.contains('?') { '&' } else { '?' })
    }
}

pub fn dish_url(venue_url: &str, id: &str) -> String {
    format!("{venue_url}/itemid-{id}")
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
         1) Call get_diet. 2) Find food that fits the diet with wolt_search (search dishes or cuisines; try several queries) \
         and look at promising restaurants with wolt_menu. Only restaurants these tools return deliver to the user and are close enough. \
         3) Choose up to 5 restaurants that are open now, and in each 1-4 available dishes that together make a meal fitting the diet. \
         Wolt menus have no nutrition data: judge by names and descriptions, and prefer dishes whose fit is clear. \
         Never pick drinks of any kind (water, juice, soda, coffee, tea, smoothies): the user buys their own. \
         Also choose up to 5 grocery stores (store: true in wolt_search) and in each 2-6 products to buy that fit the diet for the next days \
         (for example berries, nuts, oily fish, leafy greens, eggs, Greek yogurt, olive oil): search for them by name with wolt_search. \
         4) Call set_meal_options exactly once with your picks, restaurants and stores together; write each `why` in one short line in this language: {lang}. Then stop."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Value {
        json!({
            "city": "helsinki",
            "sections": [
                { "name": "category-list", "items": [{ "title": "Pizza" }] },
                { "name": "restaurants-delivering-venues", "items": [
                    { "venue": { "slug": "far-away", "name": "Far", "location": [25.10, 60.25], "delivers": true, "online": true, "country": "FIN", "product_line": "restaurant" } },
                    { "venue": { "slug": "salad-bar", "name": "Salad Bar", "location": [24.9400, 60.1690], "delivers": true, "online": true, "country": "FIN",
                                 "product_line": "restaurant", "estimate_range": "20-30", "rating": { "score": 9.1 }, "tags": ["salad"] } },
                    { "venue": { "slug": "no-delivery", "name": "Pickup", "location": [24.9390, 60.1700], "delivers": false, "country": "FIN" } },
                    { "venue": { "slug": "shop", "name": "Shop", "location": [24.9300, 60.1700], "delivers": true, "online": false, "country": "FIN", "product_line": "grocery" } }
                ]}
            ]
        })
    }

    #[test]
    fn venues_near_and_delivering_only() {
        let v = parse_venues(&page(), (60.1699, 24.9384), 3.0);
        assert_eq!(v.iter().map(|v| v.slug.as_str()).collect::<Vec<_>>(), ["salad-bar", "shop"]);
        assert_eq!(v[0].url, "https://wolt.com/en/fin/helsinki/restaurant/salad-bar");
        assert_eq!(v[1].url, "https://wolt.com/en/fin/helsinki/venue/shop");
        assert!(!v[0].store && v[1].store);
        assert_eq!((v[0].eta.as_str(), v[0].rating, v[0].online), ("20-30", Some(9.1), true));
        assert!(!v[1].online);
        assert_eq!(parse_venues(&page(), (60.1699, 24.9384), 20.0).len(), 3);
        let mut grocery = page();
        grocery.as_object_mut().unwrap().remove("city");
        grocery["city_data"] = json!({ "slug": "helsinki" });
        assert_eq!(parse_venues(&grocery, (60.1699, 24.9384), 3.0)[1].url, "https://wolt.com/en/fin/helsinki/venue/shop");
    }

    #[test]
    fn distance_is_about_right() {
        let d = km((60.1699, 24.9384), (59.4370, 24.7536));
        assert!((d - 82.0).abs() < 2.0, "{d}");
    }

    #[test]
    fn search_and_menu_dishes() {
        let search = json!({ "sections": [{ "items": [{
            "link": { "menu_item_details": { "venue_slug": "salad-bar", "description": "Chicken,\n fresh   salad" } },
            "menu_item": { "id": "a1", "name": "Chicken salad", "price": 1390, "currency": "EUR", "is_available": true, "image": { "url": "https://img/a1.jpg" } }
        }]}]});
        let d = parse_search(&search);
        assert_eq!(d[0].venue, "salad-bar");
        assert_eq!(d[0].description, "Chicken, fresh salad");
        assert_eq!((d[0].price, d[0].available, d[0].image.as_str()), (1390, true, "https://img/a1.jpg"));
        let menu = json!({ "items": [
            { "id": "m1", "name": "Bowl", "description": "x".repeat(200), "price": 1200, "disabled_info": null },
            { "id": "m2", "name": "Gone", "price": 900, "disabled_info": { "reason": "sold out" } }
        ]});
        let m = parse_menu(&menu, "salad-bar", "EUR");
        assert_eq!(m[0].description.chars().count(), 141);
        assert!(m[0].available && !m[1].available);
        assert_eq!(sized("https://imageproxy.wolt.com/assets/a", 160), "https://imageproxy.wolt.com/assets/a?w=160");
        assert_eq!(sized("", 160), "");
        assert_eq!(dish_url("https://wolt.com/en/fin/helsinki/restaurant/salad-bar", "m1"), "https://wolt.com/en/fin/helsinki/restaurant/salad-bar/itemid-m1");
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
    fn kinds_by_time_of_day() {
        assert_eq!([kind_at(8 * 60), kind_at(11 * 60), kind_at(15 * 60 + 59), kind_at(19 * 60)], ["breakfast", "lunch", "lunch", "dinner"]);
        assert!(agent_prompt("", "breakfast", "ru").contains("wants breakfast now"));
        assert!(agent_prompt("13:00", "lunch", "en").contains("Never pick drinks"));
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
        let p = Picks { at: 1, meal: "13:00".into(), kind: "lunch".into(), venues: vec![venue; 4] };
        let out = push_payload(&p, "Обед");
        assert!(out.len() <= MAX_PLAINTEXT, "{}", out.len());
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["title"], "Обед");
        assert!(!v["picks"]["venues"].as_array().unwrap().is_empty());
        let small = Picks { venues: vec![p.venues[0].clone()], ..p.clone() };
        assert!(push_payload(&Picks { venues: vec![PickVenue { dishes: vec![small.venues[0].dishes[0].clone()], ..small.venues[0].clone() }], ..small }, "t").contains("Белок"));
    }
}
