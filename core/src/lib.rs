//! ClockManage core: platform independent logic of the study timer.

pub mod clock;
pub mod config;
pub mod day;
pub mod mcp;
pub mod phone;
pub mod segments;
pub mod stats;
pub mod view;

pub use clock::Ts;
pub use config::Config;
pub use day::DayState;
pub use chrono;
