use crate::util::errors::{AppError, Result};
use std::env;

pub fn get_clickup_pat() -> Result<String> {
    env::var("CLICKUP_PAT").map_err(|_| {
        AppError::EnvError("CLICKUP_PAT environment variable is not set. Please set it to your ClickUp Personal Action Token.".to_string())
    })
}

pub fn get_gemini_api_key() -> Result<String> {
    env::var("GEMINI_API_KEY")
        .or_else(|_| env::var("GOOGLE_API_KEY"))
        .map_err(|_| {
            AppError::EnvError("Neither GEMINI_API_KEY nor GOOGLE_API_KEY is set. Gemini AI features require an API key.".to_string())
        })
}

pub fn is_log_local() -> bool {
    env::var("LOG_LOCAL").map(|v| v == "1").unwrap_or(false)
}

pub fn is_log_response_bodies() -> bool {
    env::var("LOG_RESPONSE_BODIES").map(|v| v == "1").unwrap_or(false)
}

pub fn is_log_sensitive_data() -> bool {
    env::var("LOG_SENSITIVE_DATA").map(|v| v == "1").unwrap_or(false)
}
