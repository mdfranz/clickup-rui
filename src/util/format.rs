use chrono::{DateTime, Local};

pub fn format_task_date(ms_str: &Option<String>) -> String {
    let ms = match ms_str {
        Some(s) if !s.is_empty() => s,
        _ => return String::new(),
    };

    let ms_i64 = match ms.parse::<i64>() {
        Ok(v) => v,
        Err(_) => return String::new(),
    };

    if let Some(dt) = DateTime::from_timestamp_millis(ms_i64) {
        let local_dt: DateTime<Local> = dt.into();
        local_dt.format("%m/%d").to_string()
    } else {
        String::new()
    }
}

pub fn format_comment_date(ms_str: &str) -> String {
    if ms_str.is_empty() {
        return String::new();
    }

    let ms_i64 = match ms_str.parse::<i64>() {
        Ok(v) => v,
        Err(_) => return String::new(),
    };

    if let Some(dt) = DateTime::from_timestamp_millis(ms_i64) {
        let local_dt: DateTime<Local> = dt.into();
        local_dt.format("%m/%d %H:%M").to_string()
    } else {
        String::new()
    }
}
