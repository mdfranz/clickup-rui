use chrono::{DateTime, Local};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TextSegment {
    Plain(String),
    Link { url: String, text: String },
}

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

fn tokenize_line(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current_word = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let len = chars.len();

    while i < len {
        if chars[i] == '\\' && i + 1 < len && (chars[i+1] == ' ' || chars[i+1] == '\t' || chars[i+1] == ' ') {
            current_word.push(chars[i]);
            current_word.push(chars[i+1]);
            i += 2;
        } else if chars[i].is_whitespace() {
            if !current_word.is_empty() {
                words.push(current_word.clone());
                current_word.clear();
            }
            i += 1;
        } else {
            current_word.push(chars[i]);
            i += 1;
        }
    }
    if !current_word.is_empty() {
        words.push(current_word);
    }
    words
}

pub fn wrap_text_by_words(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    for line in text.split('\n') {
        let words = tokenize_line(line);
        if words.is_empty() {
            result.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        for word in words {
            let word_len = word.chars().count();
            if current_line.is_empty() {
                current_line = word;
            } else {
                let current_len = current_line.chars().count();
                if current_len + 1 + word_len <= width {
                    current_line.push(' ');
                    current_line.push_str(&word);
                } else {
                    result.push(current_line);
                    current_line = word;
                }
            }
        }
        if !current_line.is_empty() {
            result.push(current_line);
        }
    }
    result
}

pub fn parse_links(line: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    while i < len {
        // Check for URL prefixes (http:// and https:// only)
        let is_http = (i + 7 <= len && chars[i..i+7].iter().collect::<String>() == "http://")
            || (i + 8 <= len && chars[i..i+8].iter().collect::<String>() == "https://");

        if is_http {
            if i > start {
                segments.push(TextSegment::Plain(chars[start..i].iter().collect()));
            }

            let url_start = i;
            while i < len && !chars[i].is_whitespace() {
                i += 1;
            }
            let url_str: String = chars[url_start..i].iter().collect();

            let prefix_len = if url_str.starts_with("https://") { 8 } else { 7 };

            if url_str.len() > prefix_len {
                segments.push(TextSegment::Link {
                    url: url_str.clone(),
                    text: url_str,
                });
            } else {
                segments.push(TextSegment::Plain(url_str));
            }
            start = i;
        } else {
            i += 1;
        }
    }

    if start < len {
        segments.push(TextSegment::Plain(chars[start..len].iter().collect()));
    }

    segments
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_line() {
        let line = r"Hello /path/with\ space\ here and there";
        let tokens = tokenize_line(line);
        assert_eq!(tokens, vec![
            "Hello",
            r"/path/with\ space\ here",
            "and",
            "there"
        ]);
    }

    #[test]
    fn test_wrap_text_by_words() {
        let text = r"This is a long description with /some/path\ with\ space inside it.";
        let wrapped = wrap_text_by_words(text, 25);
        assert!(wrapped.len() > 1);
        assert!(wrapped.iter().any(|line| line.contains(r"/some/path\ with\ space")));
    }

    #[test]
    fn test_parse_links() {
        let line = r"Check out https://clickup.com and /var/folders/Screenshot\ 1.png but not list/get or https://";
        let segments = parse_links(line);
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0], TextSegment::Plain("Check out ".to_string()));
        assert_eq!(segments[1], TextSegment::Link {
            url: "https://clickup.com".to_string(),
            text: "https://clickup.com".to_string()
        });
        assert_eq!(segments[2], TextSegment::Plain(r" and /var/folders/Screenshot\ 1.png but not list/get or ".to_string()));
        assert_eq!(segments[3], TextSegment::Plain("https://".to_string()));
    }
}
