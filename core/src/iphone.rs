//! The paired iPhone web app (`pwa/`): where it is hosted and the encrypted ntfy.sh link to it.
//! It shows the day (the timer and the plan) and, while Meals is on, the meal picks. Nothing on
//! the iPhone is blocked: a web app can't close other apps.

use serde::{Deserialize, Serialize};

use crate::relay::Link;
use crate::webpush::Subscription;

pub const DEFAULT_PWA: &str = "https://nikgob.github.io/ClockManage/";
/// The web app's address in builds of the Meals PR, before it moved to the owner's Pages: it
/// always turns into `DEFAULT_PWA`, so this exact address can't be chosen by hand.
const OLD_DEFAULT_PWA: &str = "https://danilt2000.github.io/ClockManage/";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Iphone {
    /// Where the web app is hosted (the QR code opens it).
    pub pwa_url: String,
    /// VAPID private key of this PC, made on first use.
    pub vapid_key: String,
    pub link: Option<Link>,
    pub subscription: Option<Subscription>,
    /// ntfy time of the last phone message applied.
    pub relay_seen: i64,
}

impl Default for Iphone {
    fn default() -> Self {
        Self { pwa_url: DEFAULT_PWA.into(), vapid_key: String::new(), link: None, subscription: None, relay_seen: 0 }
    }
}

impl Iphone {
    pub fn normalize(&mut self) {
        if self.pwa_url.trim().is_empty() || self.pwa_url == OLD_DEFAULT_PWA {
            self.pwa_url = DEFAULT_PWA.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_web_app_address_moves_to_the_owners_pages() {
        let mut m = Iphone { pwa_url: OLD_DEFAULT_PWA.into(), ..Iphone::default() };
        m.normalize();
        assert_eq!(m.pwa_url, DEFAULT_PWA);
        let mut m = Iphone { pwa_url: "https://example.org/cm/".into(), ..Iphone::default() };
        m.normalize();
        assert_eq!(m.pwa_url, "https://example.org/cm/");
    }
}
