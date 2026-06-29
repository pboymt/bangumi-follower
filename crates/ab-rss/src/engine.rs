use std::cell::RefCell;
use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use regex::Regex;

use ab_database::models::bangumi::Bangumi;
use ab_database::models::rss::{RSSItem, RSSUpdate};
use ab_database::models::torrent::Torrent;
use ab_database::repo::bangumi::BangumiRepo;
use ab_database::repo::rss::RssRepo;
use ab_database::repo::torrent::TorrentRepo;
use ab_network::NetworkClient;

use crate::error::RssError;

#[async_trait]
pub trait AddTorrent: Send + Sync {
    async fn add_torrent(
        &self,
        torrent: &Torrent,
        bangumi: &Bangumi,
        network: &NetworkClient,
    ) -> Result<bool, RssError>;
}

pub struct RSSEngine {
    pub(crate) rss_repo: RssRepo,
    pub(crate) torrent_repo: TorrentRepo,
    pub(crate) bangumi_repo: BangumiRepo,
    filter_cache: RefCell<HashMap<String, Regex>>,
}

impl RSSEngine {
    pub fn new(
        rss_repo: RssRepo,
        torrent_repo: TorrentRepo,
        bangumi_repo: BangumiRepo,
    ) -> Self {
        Self {
            rss_repo,
            torrent_repo,
            bangumi_repo,
            filter_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn rss_repo(&self) -> &RssRepo {
        &self.rss_repo
    }

    pub fn torrent_repo(&self) -> &TorrentRepo {
        &self.torrent_repo
    }

    pub fn bangumi_repo(&self) -> &BangumiRepo {
        &self.bangumi_repo
    }

    fn get_filter_pattern(&self, filter_str: &str) -> Result<Regex, RssError> {
        let mut cache = self.filter_cache.borrow_mut();
        if let Some(pattern) = cache.get(filter_str) {
            return Ok(pattern.clone());
        }
        let pattern = compile_filter_pattern(filter_str)?;
        cache.insert(filter_str.to_string(), pattern.clone());
        Ok(pattern)
    }

    fn rss_entry_to_torrents(
        entries: Vec<ab_network::RssEntry>,
        rss_id: i32,
    ) -> Vec<Torrent> {
        entries
            .into_iter()
            .map(|e| Torrent {
                id: 0,
                bangumi_id: None,
                rss_id: Some(rss_id),
                name: Some(e.title),
                url: e.url,
                homepage: Some(e.homepage),
                downloaded: false,
                qb_hash: None,
            })
            .collect()
    }

    pub async fn add_rss(
        &self,
        network: &NetworkClient,
        rss_link: &str,
        name: Option<&str>,
        aggregate: bool,
        parser: &str,
    ) -> Result<RSSItem, RssError> {
        let name = match name {
            Some(n) => n.to_string(),
            None => network.get_rss_title(rss_link).await?,
        };
        let item = RSSItem {
            id: 0,
            name: Some(name),
            url: rss_link.to_string(),
            aggregate,
            parser: parser.to_string(),
            enabled: true,
            connection_status: None,
            last_checked_at: None,
            last_error: None,
        };
        match self.rss_repo.add(&item).await? {
            true => {
                let inserted = self
                    .rss_repo
                    .search_all()
                    .await?
                    .into_iter()
                    .find(|i| i.url == rss_link);
                Ok(inserted.unwrap_or(item))
            }
            false => Err(RssError::Other("RSS already exists".to_string())),
        }
    }

    pub fn disable_list(&self, ids: &[i32]) -> Result<(), RssError> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async { Ok(self.rss_repo.disable_batch(ids).await?) })
    }

    pub fn enable_list(&self, ids: &[i32]) -> Result<(), RssError> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async { Ok(self.rss_repo.enable_batch(ids).await?) })
    }

    pub fn delete_list(&self, ids: &[i32]) -> Result<(), RssError> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            for id in ids {
                self.rss_repo.delete(*id).await?;
            }
            Ok(())
        })
    }

    pub async fn pull_rss(
        &self,
        rss_item: &RSSItem,
        network: &NetworkClient,
    ) -> Result<Vec<Torrent>, RssError> {
        let entries = network.get_torrents(&rss_item.url, None, None).await?;
        let mut torrents = Self::rss_entry_to_torrents(entries, rss_item.id);
        for t in &mut torrents {
            t.rss_id = Some(rss_item.id);
        }
        let new_torrents = self.torrent_repo.check_new(&torrents).await?;
        Ok(new_torrents)
    }

    pub fn match_torrent(&self, torrent: &Torrent) -> Result<Option<Bangumi>, RssError> {
        let name = match torrent.name.as_deref() {
            Some(n) => n,
            None => return Ok(None),
        };
        let rt = tokio::runtime::Handle::current();
        let matched = rt.block_on(async { self.bangumi_repo.match_torrent(name).await })?;
        match matched {
            Some(bangumi) => {
                if bangumi.filter.is_empty() {
                    return Ok(Some(bangumi));
                }
                let pattern = self.get_filter_pattern(&bangumi.filter)?;
                if !pattern.is_match(name) {
                    return Ok(Some(bangumi));
                }
                Ok(None)
            }
            None => Ok(None),
        }
    }

    pub fn get_rss_torrents(&self, rss_id: i32) -> Result<Vec<Torrent>, RssError> {
        let rt = tokio::runtime::Handle::current();
        Ok(rt.block_on(async { self.torrent_repo.search_rss(rss_id).await })?)
    }

    pub async fn refresh_rss(
        &self,
        rss_id: Option<i32>,
        network: &NetworkClient,
        downloader: &dyn AddTorrent,
    ) -> Result<(), RssError> {
        let rss_items = match rss_id {
            Some(id) => vec![self
                .rss_repo
                .search_id(id)
                .await?
                .ok_or_else(|| RssError::Other("RSS not found".to_string()))?],
            None => self.rss_repo.search_active().await?,
        };

        let results: Vec<(RSSItem, Result<Vec<Torrent>, String>)> =
            futures::future::join_all(rss_items.into_iter().map(|item| {
                let engine = self;
                async move {
                    let result = engine.pull_rss(&item, network).await;
                    (item, result.map_err(|e| e.to_string()))
                }
            }))
            .await;

        let now = Utc::now().to_rfc3339();
        for (mut rss_item, fetch_result) in results {
            let (new_torrents, error): (Vec<Torrent>, Option<String>) = match fetch_result {
                Ok(t) => (t, None),
                Err(e) => (vec![], Some(e)),
            };

            rss_item.connection_status = error
                .as_deref()
                .map(|_| "error".to_string())
                .or_else(|| Some("healthy".to_string()));
            rss_item.last_checked_at = Some(now.clone());
            rss_item.last_error = error.clone();
            let update = build_rss_update(&rss_item);
            self.rss_repo
                .update(rss_item.id, &update)
                .await?;

            for mut torrent in new_torrents {
                if let Some(bangumi) = self.match_torrent(&torrent)? {
                    match downloader.add_torrent(&torrent, &bangumi, network).await {
                        Ok(true) => {
                            tracing::debug!("[Engine] Added torrent {} to client", torrent.name.as_deref().unwrap_or("unknown"));
                            torrent.downloaded = true;
                        }
                        _ => {}
                    }
                }
                self.torrent_repo.add(&torrent).await?;
            }
        }
        Ok(())
    }

    pub async fn download_bangumi(
        &self,
        bangumi: &Bangumi,
        network: &NetworkClient,
        downloader: &dyn AddTorrent,
    ) -> Result<bool, RssError> {
        let filter = bangumi.filter.replace(',', "|");
        let torrents = network
            .get_torrents(
                &bangumi.rss_link,
                if filter.is_empty() { None } else { Some(&filter) },
                None,
            )
            .await?;
        if torrents.is_empty() {
            return Ok(false);
        }
        let mut success = false;
        let mut db_torrents = Vec::new();
        for entry in torrents {
            let torrent = Torrent {
                id: 0,
                bangumi_id: Some(bangumi.id),
                rss_id: None,
                name: Some(entry.title),
                url: entry.url,
                homepage: Some(entry.homepage),
                downloaded: false,
                qb_hash: None,
            };
            if downloader.add_torrent(&torrent, bangumi, network).await? {
                success = true;
            }
            db_torrents.push(torrent);
        }
        self.torrent_repo.add_all(&db_torrents).await?;
        Ok(success)
    }
}

fn build_rss_update(item: &RSSItem) -> RSSUpdate {
    RSSUpdate {
        name: None,
        url: None,
        aggregate: None,
        parser: None,
        enabled: None,
        connection_status: item.connection_status.clone(),
        last_checked_at: item.last_checked_at.clone(),
        last_error: item.last_error.clone(),
    }
}

pub(crate) fn compile_filter_pattern(filter_str: &str) -> Result<Regex, RssError> {
    let raw_pattern = filter_str.replace(',', "|");
    match Regex::new(&raw_pattern) {
        Ok(re) => Ok(re),
        Err(_) => {
            let terms: Vec<&str> = filter_str.split(',').collect();
            let escaped = terms
                .iter()
                .map(|t| regex::escape(t))
                .collect::<Vec<_>>()
                .join("|");
            Regex::new(&escaped)
                .map_err(|e| RssError::Other(format!("regex error: {e}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_filter_pattern_valid_regex() {
        let re = compile_filter_pattern("720").unwrap();
        assert!(re.is_match("720p"));
        assert!(!re.is_match("1080p"));
    }

    #[test]
    fn test_compile_filter_pattern_comma_separated() {
        let re = compile_filter_pattern("720,1080").unwrap();
        assert!(re.is_match("720p"));
        assert!(re.is_match("1080p"));
        assert!(!re.is_match("2160p"));
    }

    #[test]
    fn test_compile_filter_pattern_fallback_on_invalid_regex() {
        let re = compile_filter_pattern("(unclosed").unwrap();
        assert!(re.is_match("foo(unclosedbar"));
    }

    #[test]
    fn test_compile_filter_pattern_empty() {
        let re = compile_filter_pattern("").unwrap();
        assert!(re.is_match("anything"));
    }

    #[test]
    fn test_compile_filter_pattern_multi_term_with_special_chars() {
        let re = compile_filter_pattern("720,[abc").unwrap();
        assert!(re.is_match("foo720bar"));
        assert!(re.is_match("foo[abcbar"));
    }

    #[test]
    fn test_rss_entry_to_torrents_conversion() {
        let entries = vec![
            ab_network::RssEntry {
                title: "Test Torrent".into(),
                url: "http://example.com/1.torrent".into(),
                homepage: "http://example.com/bangumi/1".into(),
            },
        ];
        let torrents = RSSEngine::rss_entry_to_torrents(entries, 42);
        assert_eq!(torrents.len(), 1);
        assert_eq!(torrents[0].name.as_deref(), Some("Test Torrent"));
        assert_eq!(torrents[0].url, "http://example.com/1.torrent");
        assert_eq!(torrents[0].rss_id, Some(42));
        assert_eq!(torrents[0].downloaded, false);
    }

    #[test]
    fn test_filter_cache_reuses_pattern() {
        let engine = create_test_engine();
        let re1 = engine.get_filter_pattern("720").unwrap();
        let re2 = engine.get_filter_pattern("720").unwrap();
        assert!(re1.is_match("720p"));
        assert!(re2.is_match("720p"));
    }

    #[test]
    fn test_filter_cache_multiple_patterns() {
        let engine = create_test_engine();
        let re1 = engine.get_filter_pattern("720").unwrap();
        let re2 = engine.get_filter_pattern("1080").unwrap();
        assert!(re1.is_match("720p"));
        assert!(re2.is_match("1080p"));
        assert!(!re2.is_match("720p"));
    }

    fn create_test_engine() -> RSSEngine {
        let pool = create_test_pool();
        RSSEngine::new(
            RssRepo::new(pool.clone()),
            TorrentRepo::new(pool.clone()),
            BangumiRepo::new(pool),
        )
    }

    fn create_test_pool() -> sqlx::SqlitePool {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS rssitem (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT,
                    url TEXT NOT NULL,
                    aggregate INTEGER DEFAULT 0,
                    parser TEXT DEFAULT 'mikan',
                    enabled INTEGER DEFAULT 1,
                    connection_status TEXT,
                    last_checked_at TEXT,
                    last_error TEXT
                )"
            ).execute(&pool).await.unwrap();
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS bangumi (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    official_title TEXT NOT NULL DEFAULT '',
                    year TEXT,
                    title_raw TEXT NOT NULL DEFAULT '',
                    season INTEGER NOT NULL DEFAULT 1,
                    season_raw TEXT,
                    group_name TEXT,
                    dpi TEXT,
                    source TEXT,
                    subtitle TEXT,
                    eps_collect INTEGER NOT NULL DEFAULT 0,
                    episode_offset INTEGER NOT NULL DEFAULT 0,
                    season_offset INTEGER NOT NULL DEFAULT 0,
                    filter TEXT NOT NULL DEFAULT '720,\\d+-\\d+',
                    rss_link TEXT NOT NULL DEFAULT '',
                    poster_link TEXT,
                    added INTEGER NOT NULL DEFAULT 0,
                    rule_name TEXT,
                    save_path TEXT,
                    deleted INTEGER NOT NULL DEFAULT 0,
                    archived INTEGER NOT NULL DEFAULT 0,
                    air_weekday INTEGER,
                    weekday_locked INTEGER NOT NULL DEFAULT 0,
                    needs_review INTEGER NOT NULL DEFAULT 0,
                    needs_review_reason TEXT,
                    suggested_season_offset INTEGER,
                    suggested_episode_offset INTEGER,
                    title_aliases TEXT
                )"
            ).execute(&pool).await.unwrap();
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS torrent (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    refer_id INTEGER,
                    rss_id INTEGER,
                    name TEXT,
                    url TEXT NOT NULL,
                    homepage TEXT,
                    downloaded INTEGER NOT NULL DEFAULT 0,
                    qb_hash TEXT
                )"
            ).execute(&pool).await.unwrap();
            pool
        })
    }
}
