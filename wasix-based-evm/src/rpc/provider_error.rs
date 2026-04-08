use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("not found: {0}")]
    NotFound(&'static str),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("internal error: {0}")]
    Internal(String),
}
