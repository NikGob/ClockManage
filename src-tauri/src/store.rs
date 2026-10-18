//! Files on disk: config.json, days/YYYY-MM-DD.json (the log), exports.

use std::path::{Path, PathBuf};

use clockmanage_core::{Config, DayState};

pub struct Store {
    pub dir: PathBuf,
}

fn bak_path(path: &Path) -> PathBuf {
    path.with_extension("bak")
}

/// Survives a power cut: the new text reaches the disk (fsync) before it replaces the file,
/// and the previous good version stays as `.bak`. Without the fsync, NTFS could keep the
/// rename but not the data, and the day came back as an empty file.
pub fn write_atomic(path: &Path, data: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data.as_bytes())?;
        f.sync_all()?;
    }
    // A broken file never replaces the good backup.
    let current_ok = std::fs::read_to_string(path).is_ok_and(|s| serde_json::from_str::<serde_json::Value>(&s).is_ok());
    if current_ok {
        std::fs::rename(path, bak_path(path))?;
    }
    std::fs::rename(&tmp, path)
}

/// The file, or its `.bak` when the file is missing or broken. A broken file with nothing to
/// fall back on is moved aside (`.broken`) before the caller writes a new one over it.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let parse = |p: &Path| std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok());
    if let Some(v) = parse(path) {
        return Some(v);
    }
    if let Some(v) = parse(&bak_path(path)) {
        return Some(v);
    }
    if path.exists() {
        let _ = std::fs::rename(path, path.with_extension("broken"));
    }
    None
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
        self.load_config_or_new().0
    }

    /// The saved config, or the defaults (`true`) on the first run.
    pub fn load_config_or_new(&self) -> (Config, bool) {
        let saved: Option<Config> = read_json(&self.config_path());
        let new = saved.is_none();
        let mut cfg = saved.unwrap_or_default();
        cfg.normalize();
        (cfg, new)
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
        read_json(&self.day_path(date))
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
        // Linux: the XDG documents folder, "~/Документы" on a Russian desktop.
        #[cfg(target_os = "linux")]
        let docs = dirs::document_dir();
        #[cfg(not(target_os = "linux"))]
        let docs = std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Documents"));
        let base = docs
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cm-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn keeps_previous_version_as_bak() {
        let p = dir("bak").join("a.json");
        write_atomic(&p, "{\"v\":1}").unwrap();
        write_atomic(&p, "{\"v\":2}").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{\"v\":2}");
        assert_eq!(std::fs::read_to_string(bak_path(&p)).unwrap(), "{\"v\":1}");
    }

    #[test]
    fn broken_file_falls_back_to_bak() {
        let p = dir("fallback").join("a.json");
        write_atomic(&p, "{\"v\":1}").unwrap();
        write_atomic(&p, "{\"v\":2}").unwrap();
        // A power cut left zeros instead of the text.
        std::fs::write(&p, [0u8; 16]).unwrap();
        let v: serde_json::Value = read_json(&p).unwrap();
        assert_eq!(v["v"], 1);
        // Writing over the broken file keeps the good backup.
        write_atomic(&p, "{\"v\":3}").unwrap();
        assert_eq!(std::fs::read_to_string(bak_path(&p)).unwrap(), "{\"v\":1}");
    }

    #[test]
    fn missing_file_reads_bak() {
        // A cut between the two renames: only the backup is there.
        let p = dir("between").join("a.json");
        std::fs::write(bak_path(&p), "{\"v\":1}").unwrap();
        let v: serde_json::Value = read_json(&p).unwrap();
        assert_eq!(v["v"], 1);
    }

    #[test]
    fn broken_file_without_bak_is_moved_aside() {
        let p = dir("aside").join("a.json");
        std::fs::write(&p, "").unwrap();
        assert!(read_json::<serde_json::Value>(&p).is_none());
        assert!(!p.exists());
        assert!(p.with_extension("broken").exists());
    }
}
