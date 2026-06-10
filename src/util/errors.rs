use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration missing: Run 'clickup-rui setup' first.")]
    ConfigMissing,

    #[error("Environment error: {0}")]
    EnvError(String),

    #[error("ClickUp API error: status {status} - {message}")]
    ApiError {
        status: u16,
        message: String,
    },

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("AI error: {0}")]
    AiError(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
