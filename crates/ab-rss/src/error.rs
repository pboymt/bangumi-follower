use thiserror::Error;

#[derive(Error, Debug)]
pub enum RssError {
    #[error("database error: {0}")]
    Database(#[from] ab_database::DbError),
    #[error("network error: {0}")]
    Network(#[from] ab_network::NetworkError),
    #[error("parser error: {0}")]
    Parser(#[from] ab_parser::ParserError),
    #[error("add torrent failed: {0}")]
    AddTorrentFailed(String),
    #[error("{0}")]
    Other(String),
}
