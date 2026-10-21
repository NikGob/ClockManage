//! Wolt, the first delivery service of Meals. Wolt has no public API for customers: the JSON
//! parsed here comes from the endpoints wolt.com itself uses, read without an account.

use serde_json::{json, Value};

use crate::meals::{km, short, Dish, Venue};

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
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
}
