use chrono::{DateTime, Local};

pub fn wrap_text_by_chars(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    for line in text.split('\n') {
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            result.push(String::new());
        } else {
            for chunk in chars.chunks(width) {
                result.push(chunk.iter().collect::<String>());
            }
        }
    }
    result
}

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
