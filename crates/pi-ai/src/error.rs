use thiserror::Error;

#[derive(Debug, Error)]
pub enum AIError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("provider error: {0}")]
    Provider(String),

    #[error("unsupported features: {0}")]
    Unsupported(String),
}
