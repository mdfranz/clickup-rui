use std::env;
use std::path::PathBuf;

pub fn get_config_path() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            let mut p = PathBuf::from(xdg);
            p.push("clickup-tui");
            p.push("config.toml");
            return p;
        }
    }

    if let Some(home) = dirs::home_dir() {
        let mut p = home;
        p.push(".config");
        p.push("clickup-tui");
        p.push("config.toml");
        return p;
    }

    PathBuf::from(".config/clickup-tui/config.toml")
}

pub fn get_legacy_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|mut h| {
        h.push(".local");
        h.push("clickup-tui.toml");
        h
    })
}

pub fn get_cache_path() -> PathBuf {
    if let Some(cache_dir) = dirs::cache_dir() {
        let mut p = cache_dir;
        p.push("clickup-tui");
        p.push("cache.json");
        p
    } else {
        let mut p = env::temp_dir();
        p.push("clickup-tui");
        p.push("cache.json");
        p
    }
}

pub fn get_log_path() -> PathBuf {
    if crate::util::env::is_log_local() {
        PathBuf::from("app.log")
    } else if let Some(cache_dir) = dirs::cache_dir() {
        let mut p = cache_dir;
        p.push("clickup-tui");
        p.push("app.log");
        p
    } else {
        let mut p = env::temp_dir();
        p.push("clickup-tui");
        p.push("app.log");
        p
    }
}
