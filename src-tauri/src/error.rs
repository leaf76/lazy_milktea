use thiserror::Error;

#[derive(Error, Debug)]
pub enum LogcatError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Parse error at line {line}: {message}")]
    Parse { line: usize, message: String },

    #[error("Index corruption: {0}")]
    IndexCorruption(String),

    #[error("Time conversion failed for '{input}': {reason}")]
    TimeConversion { input: String, reason: String },

    #[error("Invalid filter: {0}")]
    InvalidFilter(String),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("Cache not found: {0}")]
    CacheNotFound(String),

    #[error("No bugreport found in archive")]
    NoBugreportFound,

    #[error("State lock poisoned")]
    StatePoisoned,
}

impl From<LogcatError> for String {
    fn from(e: LogcatError) -> Self {
        e.to_string()
    }
}

pub type Result<T> = std::result::Result<T, LogcatError>;
