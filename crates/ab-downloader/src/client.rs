use std::collections::HashMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use ab_core::config::model::{self, DownloaderType};
use ab_database::models::{bangumi::Bangumi, torrent::Torrent};
use ab_network::NetworkClient;

use crate::error::DownloaderError;
use crate::path::{gen_save_path, rule_name, FileInfo};

#[async_trait]
pub trait DownloaderClient: Send + Sync {
    async fn auth(&mut self) -> Result<bool, DownloaderError>;
    async fn logout(&mut self) -> Result<(), DownloaderError>;
    async fn check_host(&self) -> Result<bool, DownloaderError>;

    async fn add_torrents(
        &self,
        torrent_urls: Option<&[String]>,
        torrent_files: Option<&[Vec<u8>]>,
        save_path: &str,
        category: &str,
        tags: Option<&str>,
    ) -> Result<bool, DownloaderError>;

    async fn torrents_info(
        &self,
        status_filter: Option<&str>,
        category: Option<&str>,
        tag: Option<&str>,
    ) -> Result<Vec<TorrentInfo>, DownloaderError>;

    async fn torrents_files(&self, hash: &str) -> Result<Vec<FileInfo>, DownloaderError>;
    async fn torrents_delete(&self, hash: &str, delete_files: bool) -> Result<(), DownloaderError>;
    async fn torrents_pause(&self, hash: &str) -> Result<(), DownloaderError>;
    async fn torrents_resume(&self, hash: &str) -> Result<(), DownloaderError>;

    async fn torrents_rename_file(
        &self,
        hash: &str,
        old_path: &str,
        new_path: &str,
        verify: bool,
    ) -> Result<bool, DownloaderError>;

    async fn move_torrent(&self, hash: &str, new_location: &str) -> Result<(), DownloaderError>;
    async fn get_torrent_path(&self, hash: &str) -> Result<String, DownloaderError>;
    async fn set_category(&self, hash: &str, category: &str) -> Result<(), DownloaderError>;
    async fn add_tag(&self, hash: &str, tag: &str) -> Result<(), DownloaderError>;

    async fn rss_add_feed(&self, url: &str, item_path: Option<&str>) -> Result<(), DownloaderError>;
    async fn rss_remove_item(&self, item_path: &str) -> Result<(), DownloaderError>;
    async fn rss_get_feeds(&self) -> Result<Value, DownloaderError>;
    async fn rss_set_rule(&self, rule_name: &str, rule_def: Value) -> Result<(), DownloaderError>;
    async fn get_download_rule(&self) -> Result<Value, DownloaderError>;
    async fn remove_rule(&self, rule_name: &str) -> Result<(), DownloaderError>;

    async fn prefs_init(&self, prefs: HashMap<String, Value>) -> Result<(), DownloaderError>;
    async fn get_app_prefs(&self) -> Result<Value, DownloaderError>;
    async fn add_category(&self, category: &str) -> Result<(), DownloaderError>;
}

#[derive(Debug, Clone, Deserialize)]
pub struct TorrentInfo {
    pub hash: String,
    pub name: String,
    pub save_path: String,
    pub category: String,
    pub state: String,
    pub progress: f64,
    pub tags: String,
}

pub async fn init_downloader(client: &dyn DownloaderClient) -> Result<(), DownloaderError> {
    client.add_category("BangumiCollection").await?;
    let mut prefs = HashMap::new();
    prefs.insert(
        "rss_download_repack_mode".to_string(),
        Value::String("true".to_string()),
    );
    prefs.insert(
        "rss_max_articles_per_feed".to_string(),
        Value::String("100".to_string()),
    );
    prefs.insert(
        "rss_processing_enabled".to_string(),
        Value::String("true".to_string()),
    );
    prefs.insert(
        "rss_auto_downloading_enabled".to_string(),
        Value::String("true".to_string()),
    );
    client.prefs_init(prefs).await
}

pub fn create_client(
    config: &model::Downloader,
) -> Result<Box<dyn DownloaderClient>, DownloaderError> {
    match config.r#type {
        DownloaderType::Qbittorrent => Ok(Box::new(crate::qb::QbDownloader::new(
            &config.host(),
            &config.username(),
            &config.password(),
            config.ssl,
        )?)),
        DownloaderType::Aria2 => Ok(Box::new(crate::aria2::Aria2Downloader::new(
            &config.host(),
            &config.password(),
        )?)),
        DownloaderType::Mock => Ok(Box::new(crate::mock::MockDownloader::new())),
        DownloaderType::Transmission => {
            Err(DownloaderError::UnsupportedType("transmission".to_string()))
        }
    }
}

pub struct DownloadClient {
    client: Box<dyn DownloaderClient>,
    downloader_path: String,
    group_tag: bool,
    authed: bool,
}

impl DownloadClient {
    pub fn new(config: &model::Downloader) -> Result<Self, DownloaderError> {
        let client = create_client(config)?;
        Ok(Self {
            client,
            downloader_path: config.path.clone(),
            group_tag: false,
            authed: false,
        })
    }

    pub fn with_group_tag(mut self, group_tag: bool) -> Self {
        self.group_tag = group_tag;
        self
    }

    pub async fn auth(&mut self) -> Result<bool, DownloaderError> {
        let ok = self.client.auth().await?;
        self.authed = ok;
        Ok(ok)
    }

    pub async fn logout(&mut self) -> Result<(), DownloaderError> {
        self.client.logout().await
    }

    pub async fn check_host(&self) -> Result<bool, DownloaderError> {
        self.client.check_host().await
    }

    pub async fn init_downloader(&self) -> Result<(), DownloaderError> {
        init_downloader(self.client.as_ref()).await
    }

    pub async fn set_rule(&self, data: &Bangumi) -> Result<(), DownloaderError> {
        let name = rule_name(data, self.group_tag);
        let save_path = gen_save_path(data, &self.downloader_path);
        let rule = serde_json::json!({
            "enable": true,
            "mustContain": data.title_raw,
            "mustNotContain": data.filter,
            "useRegex": true,
            "episodeFilter": "",
            "smartFilter": false,
            "previouslyMatchedEpisodes": [],
            "affectedFeeds": data.rss_link,
            "ignoreDays": 0,
            "lastMatch": "",
            "addPaused": false,
            "assignedCategory": "Bangumi",
            "savePath": save_path,
        });
        self.client.rss_set_rule(&name, rule).await
    }

    pub async fn set_rules(&self, bangumi_info: &[Bangumi]) -> Result<(), DownloaderError> {
        for data in bangumi_info {
            self.set_rule(data).await?;
        }
        Ok(())
    }

    pub async fn get_torrent_info(
        &self,
        category: &str,
        status_filter: &str,
        tag: Option<&str>,
    ) -> Result<Vec<TorrentInfo>, DownloaderError> {
        self.client
            .torrents_info(Some(status_filter), Some(category), tag)
            .await
    }

    pub async fn get_torrent_files(&self, hash: &str) -> Result<Vec<FileInfo>, DownloaderError> {
        self.client.torrents_files(hash).await
    }

    pub async fn rename_torrent_file(
        &self,
        hash: &str,
        old_path: &str,
        new_path: &str,
        verify: bool,
    ) -> Result<bool, DownloaderError> {
        self.client
            .torrents_rename_file(hash, old_path, new_path, verify)
            .await
    }

    pub async fn delete_torrent(&self, hash: &str, delete_files: bool) -> Result<(), DownloaderError> {
        self.client.torrents_delete(hash, delete_files).await
    }

    pub async fn pause_torrent(&self, hash: &str) -> Result<(), DownloaderError> {
        self.client.torrents_pause(hash).await
    }

    pub async fn resume_torrent(&self, hash: &str) -> Result<(), DownloaderError> {
        self.client.torrents_resume(hash).await
    }

    pub async fn add_torrent(
        &self,
        torrent: &Torrent,
        bangumi: &Bangumi,
        network: &NetworkClient,
    ) -> Result<bool, DownloaderError> {
        let tags = format!("ab:{}", bangumi.id);
        let save_path = gen_save_path(bangumi, &self.downloader_path);

        if torrent.url.starts_with("magnet:") {
            self.client
                .add_torrents(
                    Some(&[torrent.url.clone()]),
                    None,
                    &save_path,
                    "BangumiCollection",
                    Some(&tags),
                )
                .await
        } else {
            let content = network
                .get_content(&torrent.url)
                .await
                .map_err(DownloaderError::Network)?;
            self.client
                .add_torrents(None, Some(&[content]), &save_path, "BangumiCollection", Some(&tags))
                .await
        }
    }

    pub async fn add_torrents(
        &self,
        torrents: &[Torrent],
        bangumi: &Bangumi,
        network: &NetworkClient,
    ) -> Result<bool, DownloaderError> {
        let tags = format!("ab:{}", bangumi.id);
        let save_path = gen_save_path(bangumi, &self.downloader_path);

        let urls: Vec<String> = torrents
            .iter()
            .filter(|t| t.url.starts_with("magnet:"))
            .map(|t| t.url.clone())
            .collect();
        let mut files = Vec::new();
        for t in torrents.iter().filter(|t| !t.url.starts_with("magnet:")) {
            let content = network
                .get_content(&t.url)
                .await
                .map_err(DownloaderError::Network)?;
            files.push(content);
        }

        self.client
            .add_torrents(
                if urls.is_empty() { None } else { Some(&urls) },
                if files.is_empty() { None } else { Some(&files) },
                &save_path,
                "BangumiCollection",
                Some(&tags),
            )
            .await
    }

    pub async fn move_torrent(&self, hash: &str, location: &str) -> Result<(), DownloaderError> {
        self.client.move_torrent(hash, location).await
    }

    pub async fn add_rss_feed(
        &self,
        rss_link: &str,
        item_path: Option<&str>,
    ) -> Result<(), DownloaderError> {
        self.client.rss_add_feed(rss_link, item_path).await
    }

    pub async fn remove_rss_feed(&self, item_path: &str) -> Result<(), DownloaderError> {
        self.client.rss_remove_item(item_path).await
    }

    pub async fn get_rss_feed(&self) -> Result<Value, DownloaderError> {
        self.client.rss_get_feeds().await
    }

    pub async fn get_download_rules(&self) -> Result<Value, DownloaderError> {
        self.client.get_download_rule().await
    }

    pub async fn remove_rule(&self, rule_name: &str) -> Result<(), DownloaderError> {
        self.client.remove_rule(rule_name).await
    }

    pub async fn get_torrents_by_tag(&self, tag: &str) -> Result<Vec<TorrentInfo>, DownloaderError> {
        self.client.torrents_info(None, None, Some(tag)).await
    }

    pub async fn add_tag(&self, hash: &str, tag: &str) -> Result<(), DownloaderError> {
        self.client.add_tag(hash, tag).await
    }
}
