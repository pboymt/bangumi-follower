use thiserror::Error;

#[derive(Error, Debug)]
pub enum DownloaderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("network error: {0}")]
    Network(#[from] ab_network::NetworkError),
    #[error("authentication failed")]
    AuthFailed,
    #[error("unsupported downloader type: {0}")]
    UnsupportedType(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("operation not supported: {0}")]
    NotSupported(String),
    #[error("{0}")]
    Other(String),
}
