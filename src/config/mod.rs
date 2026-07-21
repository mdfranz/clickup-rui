pub mod paths;

use crate::util::errors::{AppError, Result};
use paths::{get_config_path, get_legacy_config_path};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FolderConfig {
    pub id: String,
    pub name: String,
}

fn default_ai_provider() -> String {
    "gemini".to_string()
}

fn default_ai_model() -> String {
    "gemini-3.5-flash".to_string()
}

fn default_ollama_url() -> Option<String> {
    Some("http://localhost:11434".to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub workspace_id: String,
    pub workspace_name: String,
    pub space_id: String,
    pub space_name: String,
    pub folders: Vec<FolderConfig>,

    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,

    #[serde(default = "default_ai_model")]
    pub ai_model: String,

    #[serde(
        default = "default_ollama_url",
        skip_serializing_if = "Option::is_none"
    )]
    pub ollama_url: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let primary_path = get_config_path();
        if primary_path.exists() {
            let content = fs::read_to_string(&primary_path)?;
            let config: Config = toml::from_str(&content)
                .map_err(|e| AppError::Other(format!("Failed to parse config: {}", e)))?;
            return Ok(config);
        }

        if let Some(legacy_path) = get_legacy_config_path() {
            if legacy_path.exists() {
                let content = fs::read_to_string(&legacy_path)?;
                let config: Config = toml::from_str(&content).map_err(|e| {
                    AppError::Other(format!("Failed to parse legacy config: {}", e))
                })?;
                return Ok(config);
            }
        }

        Err(AppError::ConfigMissing)
    }

    pub fn save(&self) -> Result<()> {
        let path = get_config_path();

        // 1. Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = fs::metadata(parent) {
                    let mut perms = metadata.permissions();
                    perms.set_mode(0o755);
                    let _ = fs::set_permissions(parent, perms);
                }
            }
        }

        // 2. Serialize to TOML
        let content = toml::to_string_pretty(self)
            .map_err(|e| AppError::Other(format!("Failed to serialize config: {}", e)))?;

        // 3. Write file
        fs::write(&path, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o644);
                let _ = fs::set_permissions(&path, perms);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_config_save_and_load() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let tmp_dir = TempDir::new().unwrap();
        let original_xdg = std::env::var("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", tmp_dir.path());

        let cfg = Config {
            workspace_id: "w123".to_string(),
            workspace_name: "My Workspace".to_string(),
            space_id: "s456".to_string(),
            space_name: "My Space".to_string(),
            folders: vec![FolderConfig {
                id: "f789".to_string(),
                name: "My Folder".to_string(),
            }],
            ai_provider: default_ai_provider(),
            ai_model: default_ai_model(),
            ollama_url: default_ollama_url(),
        };

        // Save
        cfg.save().unwrap();

        // Load
        let loaded = Config::load().unwrap();
        assert_eq!(loaded.workspace_id, "w123");
        assert_eq!(loaded.workspace_name, "My Workspace");
        assert_eq!(loaded.space_id, "s456");
        assert_eq!(loaded.space_name, "My Space");
        assert_eq!(loaded.folders.len(), 1);
        assert_eq!(loaded.folders[0].id, "f789");
        assert_eq!(loaded.folders[0].name, "My Folder");
        assert_eq!(loaded.ai_provider, "gemini");
        assert_eq!(loaded.ai_model, "gemini-3.5-flash");
        assert_eq!(
            loaded.ollama_url,
            Some("http://localhost:11434".to_string())
        );

        // Clean up env
        match original_xdg {
            Ok(val) => std::env::set_var("XDG_CONFIG_HOME", val),
            Err(_) => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn test_config_legacy_fallback() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let tmp_dir = TempDir::new().unwrap();
        let original_xdg = std::env::var("XDG_CONFIG_HOME");
        let original_home = std::env::var("HOME");

        // Set XDG to empty or pointing to nonexistent, so primary config load fails
        std::env::set_var("XDG_CONFIG_HOME", tmp_dir.path().join("nonexistent_xdg"));
        // Override HOME so legacy path points into our tmp_dir
        std::env::set_var("HOME", tmp_dir.path());

        // Create legacy config file manually in legacy location: HOME/.local/clickup-tui.toml
        let legacy_dir = tmp_dir.path().join(".local");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let legacy_file = legacy_dir.join("clickup-tui.toml");

        let cfg = Config {
            workspace_id: "legacy_w".to_string(),
            workspace_name: "Legacy W".to_string(),
            space_id: "legacy_s".to_string(),
            space_name: "Legacy S".to_string(),
            folders: vec![],
            ai_provider: default_ai_provider(),
            ai_model: default_ai_model(),
            ollama_url: default_ollama_url(),
        };

        let content = toml::to_string_pretty(&cfg).unwrap();
        std::fs::write(&legacy_file, content).unwrap();

        // Load - should fallback to legacy config!
        let loaded = Config::load().unwrap();
        assert_eq!(loaded.workspace_id, "legacy_w");
        assert_eq!(loaded.workspace_name, "Legacy W");

        // Restore env
        match original_xdg {
            Ok(val) => std::env::set_var("XDG_CONFIG_HOME", val),
            Err(_) => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        match original_home {
            Ok(val) => std::env::set_var("HOME", val),
            Err(_) => std::env::remove_var("HOME"),
        }
    }

    #[test]
    fn test_legacy_gemini_key_is_not_serialized() {
        let config: Config = toml::from_str(
            r#"
workspace_id = "w123"
workspace_name = "My Workspace"
space_id = "s456"
space_name = "My Space"
gemini_api_key = "obsolete-secret"

[[folders]]
id = "f789"
name = "My Folder"
"#,
        )
        .unwrap();

        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(!serialized.contains("gemini_api_key"));
        assert!(!serialized.contains("obsolete-secret"));
    }
}
