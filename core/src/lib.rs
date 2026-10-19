//! ClockManage core: platform independent logic of the study timer.
#![recursion_limit = "256"]

pub mod clock;
pub mod config;
pub mod day;
pub mod i18n;
pub mod mcp;
pub mod meals;
pub mod phone;
pub mod relay;
pub mod segments;
pub mod stats;
pub mod view;
pub mod webpush;

pub use clock::Ts;
pub use config::Config;
pub use day::DayState;
pub use chrono;
