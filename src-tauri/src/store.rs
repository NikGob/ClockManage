//! Files on disk: config.json, days/YYYY-MM-DD.json (the log), exports.

use std::path::{Path, PathBuf};

use clockmanage_core::{Config, DayState};

pub struct Store {
    pub dir: PathBuf,
}

fn write_atomic(path: &Path, data: &str) -> std::io::Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(dir.join("days"));
        Self { dir }
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.join("config.json")
    }

    pub fn load_config(&self) -> Config {
        let mut cfg: Config = std::fs::read_to_string(self.config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        cfg.normalize();
        cfg
    }

    pub fn save_config(&self, cfg: &Config) {
        if let Ok(s) = serde_json::to_string_pretty(cfg) {
            let _ = write_atomic(&self.config_path(), &s);
        }
    }

    pub fn day_path(&self, date: &str) -> PathBuf {
        self.dir.join("days").join(format!("{date}.json"))
    }

    pub fn load_day(&self, date: &str) -> Option<DayState> {
        std::fs::read_to_string(self.day_path(date)).ok().and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn save_day(&self, day: &DayState) {
        if let Ok(s) = serde_json::to_string_pretty(day) {
            let _ = write_atomic(&self.day_path(&day.date.format("%Y-%m-%d").to_string()), &s);
        }
    }

    /// Dates with a saved log, newest first.
    pub fn list_dates(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.dir.join("days"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.strip_suffix(".json").filter(|d| d.len() == 10).map(|d| d.to_string())
            })
            .collect();
        v.sort_unstable_by(|a, b| b.cmp(a));
        v
    }

    /// Last day file other than `today`, used to finalize a day left open.
    pub fn last_other_day(&self, today: &str) -> Option<DayState> {
        self.list_dates().into_iter().find(|d| d.as_str() < today).and_then(|d| self.load_day(&d))
    }

    pub fn export_dir(&self) -> PathBuf {
        let base = std::env::var_os("USERPROFILE")
            .map(|p| PathBuf::from(p).join("Documents"))
            .filter(|p| p.exists())
            .unwrap_or_else(|| self.dir.clone());
        base.join("ClockManage")
    }

    pub fn write_export(&self, name: &str, data: &str) -> std::io::Result<PathBuf> {
        let dir = self.export_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(name);
        std::fs::write(&path, data)?;
        Ok(path)
    }
}
