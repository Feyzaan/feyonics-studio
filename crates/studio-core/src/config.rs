//! Persistent settings, stored as JSON (Windows: %APPDATA%\Feyonics\studio.json).
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub midi_port_name: String,       // output port the bridge writes to
    pub device_name_filter: String,   // BLE device name contains this
    pub auto_connect: bool,
    pub start_with_windows: bool,
    pub theme: String,
    pub routes: Vec<crate::routes::Route>,
}
impl Default for Config {
    fn default() -> Self {
        Config { midi_port_name: "Feyonics Zeus".into(), device_name_filter: "Zeus".into(), auto_connect: true, start_with_windows: false, theme: "dark".into(), routes: crate::routes::default_routes() }
    }
}

pub fn default_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Feyonics").join("studio.json")
}

impl Config {
    /// Missing or corrupt file yields defaults (a bad config must never stop the app starting).
    pub fn load(path: &Path) -> Config {
        std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap())?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_and_defaults() {
        let dir = std::env::temp_dir().join(format!("fs-cfg-{}", std::process::id()));
        let p = dir.join("studio.json");
        assert_eq!(Config::load(&p), Config::default());
        let c = Config { theme: "paper".into(), auto_connect: false, ..Default::default() };
        c.save(&p).unwrap();
        assert_eq!(Config::load(&p), c);
        std::fs::write(&p, "{ not json").unwrap();
        assert_eq!(Config::load(&p), Config::default());
        std::fs::write(&p, r#"{"theme":"ocean"}"#).unwrap();
        assert_eq!(Config::load(&p).theme, "ocean");
        let _ = std::fs::remove_dir_all(dir);
    }
}
