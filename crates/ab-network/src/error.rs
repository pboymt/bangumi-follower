#[derive(thiserror::Error, Debug)]
pub enum NetworkError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("XML parse error: {0}")]
    XmlParse(String),
    #[error("request failed after {0} retries: {1}")]
    RetryExhausted(u32, String),
    #[error("connection check failed")]
    ConnectionFailed,
}
