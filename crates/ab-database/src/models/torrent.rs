use sqlx::FromRow;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Torrent {
    pub id: i32,
    #[sqlx(rename = "refer_id")]
    pub bangumi_id: Option<i32>,
    pub rss_id: Option<i32>,
    pub name: Option<String>,
    pub url: String,
    pub homepage: Option<String>,
    pub downloaded: bool,
    pub qb_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentUpdate {
    pub downloaded: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct EpisodeFile {
    pub media_path: String,
    pub group: Option<String>,
    pub title: String,
    pub season: i32,
    pub episode: String,
    pub suffix: String,
}

#[derive(Debug, Clone)]
pub struct SubtitleFile {
    pub media_path: String,
    pub group: Option<String>,
    pub title: String,
    pub season: i32,
    pub episode: String,
    pub suffix: String,
    pub language: Option<String>,
}
