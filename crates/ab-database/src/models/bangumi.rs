use sqlx::FromRow;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Bangumi {
    pub id: i32,
    pub official_title: String,
    pub year: Option<String>,
    pub title_raw: String,
    pub season: i32,
    pub season_raw: Option<String>,
    pub group_name: Option<String>,
    pub dpi: Option<String>,
    pub source: Option<String>,
    pub subtitle: Option<String>,
    pub eps_collect: bool,
    pub episode_offset: i32,
    pub season_offset: i32,
    pub filter: String,
    pub rss_link: String,
    pub poster_link: Option<String>,
    pub added: bool,
    pub rule_name: Option<String>,
    pub save_path: Option<String>,
    pub deleted: bool,
    pub archived: bool,
    pub air_weekday: Option<i32>,
    pub weekday_locked: bool,
    pub needs_review: bool,
    pub needs_review_reason: Option<String>,
    pub suggested_season_offset: Option<i32>,
    pub suggested_episode_offset: Option<i32>,
    pub title_aliases: Option<String>,
}

impl Bangumi {
    pub fn aliases(&self) -> Vec<String> {
        self.title_aliases.as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    pub fn set_aliases(&mut self, aliases: Vec<String>) {
        self.title_aliases = Some(serde_json::to_string(&aliases).unwrap_or_default());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BangumiUpdate {
    pub official_title: Option<String>,
    pub year: Option<String>,
    pub title_raw: Option<String>,
    pub season: Option<i32>,
    pub season_raw: Option<String>,
    pub group_name: Option<String>,
    pub dpi: Option<String>,
    pub source: Option<String>,
    pub subtitle: Option<String>,
    pub eps_collect: Option<bool>,
    pub episode_offset: Option<i32>,
    pub season_offset: Option<i32>,
    pub filter: Option<String>,
    pub rss_link: Option<String>,
    pub poster_link: Option<String>,
    pub added: Option<bool>,
    pub rule_name: Option<String>,
    pub save_path: Option<String>,
    pub deleted: Option<bool>,
    pub archived: Option<bool>,
    pub air_weekday: Option<i32>,
    pub weekday_locked: Option<bool>,
    pub needs_review: Option<bool>,
    pub needs_review_reason: Option<String>,
    pub suggested_season_offset: Option<i32>,
    pub suggested_episode_offset: Option<i32>,
    pub title_aliases: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub official_title: String,
    pub season: i32,
    pub episode: i32,
    pub poster_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Episode {
    pub title_en: Option<String>,
    pub title_zh: Option<String>,
    pub title_jp: Option<String>,
    pub season: i32,
    pub season_raw: String,
    pub episode: i32,
    pub sub: String,
    pub group: String,
    pub resolution: String,
    pub source: String,
}
