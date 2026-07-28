use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("Ignore error: {0}")]
    Ignore(#[from] ignore::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Invalid preview argument: {0}")]
    InvalidPreview(String),

    #[error("Skim error: {0}")]
    Skim(String),

    #[error("Error: {0}")]
    General(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "FCS-IO",
            Self::Regex(_) => "FCS-REGEX",
            Self::Ignore(_) => "FCS-IGNORE",
            Self::Sqlite(_) => "FCS-SQLITE",
            Self::FileNotFound(_) => "FCS-NOT-FOUND",
            Self::InvalidPreview(_) => "FCS-INVALID-PREVIEW",
            Self::Skim(_) => "FCS-PICKER",
            Self::General(_) => "FCS-GENERAL",
        }
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_errors_have_stable_codes() {
        assert_eq!(
            AppError::InvalidPreview("bad location".to_string()).code(),
            "FCS-INVALID-PREVIEW"
        );
        assert_eq!(AppError::General("failed".to_string()).code(), "FCS-GENERAL");
    }
}
