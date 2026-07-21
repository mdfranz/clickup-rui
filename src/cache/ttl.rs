use std::time::{SystemTime, UNIX_EPOCH};

pub const TTL_USER: i64 = 24 * 3600; // 24h
pub const TTL_TEAMS: i64 = 4 * 3600; // 4h
pub const TTL_SPACES: i64 = 4 * 3600; // 4h
pub const TTL_FOLDERS: i64 = 3600; // 1h
pub const TTL_LISTS: i64 = 3600; // 1h
pub const TTL_LIST_DETAIL: i64 = 3600; // 1h
pub const TTL_WS_USERS: i64 = 4 * 3600; // 4h
pub const TTL_TASK_DETAIL: i64 = 10 * 60; // 10m
pub const TTL_COMMENTS: i64 = 5 * 60; // 5m
pub const TTL_TASKS_FULL: i64 = 30 * 60; // 30m

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
