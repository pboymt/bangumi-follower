#[derive(thiserror::Error, Debug)]
pub enum ParserError {
    #[error("parse failed: {0}")]
    ParseFailed(String),
    #[error("network error: {0}")]
    Network(#[from] ab_network::NetworkError),
    #[error("no match found: {0}")]
    NoMatch(String),
}
