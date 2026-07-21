use crate::cache::ttl::now_secs;
use crate::clickup::models::*;
use crate::config::paths::get_cache_path;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CacheEntry<T> {
    pub value: T,
    pub expires_at: i64, // Unix timestamp in seconds
}

impl<T> CacheEntry<T> {
    pub fn is_expired(&self) -> bool {
        now_secs() > self.expires_at
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TaskListCacheEntry {
    pub tasks: Vec<Task>,
    pub fetched_at: i64,       // Unix timestamp in seconds when fetched
    pub max_date_updated: i64, // millisecond timestamp high-water mark
    pub includes_closed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CacheStore {
    pub version: u32,
    pub updated_at: String,

    pub user: Option<CacheEntry<User>>,
    pub teams: Option<CacheEntry<Vec<Team>>>,
    pub spaces_by_team: HashMap<String, CacheEntry<Vec<Space>>>,
    pub folders_by_space: HashMap<String, CacheEntry<Vec<Folder>>>,
    pub lists_by_folder: HashMap<String, CacheEntry<Vec<List>>>,
    pub list_detail_by_list: HashMap<String, CacheEntry<List>>,
    pub workspace_users_by_workspace: HashMap<String, CacheEntry<Vec<User>>>,
    pub task_detail_by_task: HashMap<String, CacheEntry<Task>>,
    pub comments_by_task: HashMap<String, CacheEntry<Vec<Comment>>>,

    pub tasks: HashMap<String, TaskListCacheEntry>,

    #[serde(skip)]
    pub dirty: bool,
}

impl CacheStore {
    pub fn new() -> Self {
        Self {
            version: 1,
            updated_at: Local::now().to_rfc3339(),
            user: None,
            teams: None,
            spaces_by_team: HashMap::new(),
            folders_by_space: HashMap::new(),
            lists_by_folder: HashMap::new(),
            list_detail_by_list: HashMap::new(),
            workspace_users_by_workspace: HashMap::new(),
            task_detail_by_task: HashMap::new(),
            comments_by_task: HashMap::new(),
            tasks: HashMap::new(),
            dirty: false,
        }
    }

    pub fn load() -> Self {
        let path = get_cache_path();
        if !path.exists() {
            return Self::new();
        }

        match fs::read_to_string(&path) {
            Ok(content) => {
                match serde_json::from_str::<CacheStore>(&content) {
                    Ok(mut store) => {
                        if store.version == 1 {
                            store.dirty = false;
                            store
                        } else {
                            // Version mismatch -> fresh store
                            tracing::warn!("Cache version mismatch. Starting fresh.");
                            let mut fresh = Self::new();
                            fresh.dirty = true;
                            fresh
                        }
                    }
                    Err(e) => {
                        // Corrupt cache -> fresh store
                        tracing::warn!("Cache corrupt: {}. Starting fresh.", e);
                        let mut fresh = Self::new();
                        fresh.dirty = true;
                        fresh
                    }
                }
            }
            Err(_) => Self::new(),
        }
    }

    pub fn save(&mut self) -> Result<(), std::io::Error> {
        if !self.dirty {
            return Ok(());
        }

        self.updated_at = Local::now().to_rfc3339();
        let path = get_cache_path();

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Serialize
        let content = serde_json::to_string_pretty(self)?;

        // Write to temporary file
        let mut tmp_path = path.clone();
        tmp_path.set_extension("json.tmp");
        fs::write(&tmp_path, content)?;

        // Atomic rename
        fs::rename(&tmp_path, &path)?;

        self.dirty = false;
        Ok(())
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

impl Default for CacheStore {
    fn default() -> Self {
        Self::new()
    }
}
