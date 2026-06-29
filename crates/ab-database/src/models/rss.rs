use sqlx::FromRow;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RSSItem {
    pub id: i32,
    pub name: Option<String>,
    pub url: String,
    pub aggregate: bool,
    pub parser: String,
    pub enabled: bool,
    pub connection_status: Option<String>,
    pub last_checked_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RSSUpdate {
    pub name: Option<String>,
    pub url: Option<String>,
    pub aggregate: Option<bool>,
    pub parser: Option<String>,
    pub enabled: Option<bool>,
    pub connection_status: Option<String>,
    pub last_checked_at: Option<String>,
    pub last_error: Option<String>,
}
