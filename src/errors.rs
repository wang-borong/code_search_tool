use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("Ignore error: {0}")]
    Ignore(#[from] ignore::Error),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Invalid preview argument: {0}")]
    InvalidPreview(String),

    #[error("Skim error: {0}")]
    Skim(String),

    #[error("Error: {0}")]
    General(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
