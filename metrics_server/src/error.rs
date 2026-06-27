#[derive(thiserror::Error, Debug)]
pub enum MetricsServerError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("collector error: {0}")]
    Collector(String),

    #[error("rpc error: {0}")]
    Rpc(String),

    #[error("http error: {0}")]
    Http(String),
}
