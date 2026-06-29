pub mod client;
pub mod error;
pub mod site;

pub use client::{NetworkClient, RssEntry, get_shared_client, reset_shared_client};
pub use error::NetworkError;
