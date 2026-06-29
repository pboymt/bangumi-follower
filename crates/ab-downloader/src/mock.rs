use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::client::{DownloaderClient, TorrentInfo};
use crate::error::DownloaderError;
use crate::path::FileInfo;

fn mock_hash(input: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub struct MockDownloader {
    torrents: Mutex<HashMap<String, Value>>,
    rules: Mutex<HashMap<String, Value>>,
    feeds: Mutex<HashMap<String, Value>>,
    categories: Mutex<HashSet<String>>,
    prefs: Mutex<HashMap<String, Value>>,
}

impl MockDownloader {
    pub fn new() -> Self {
        Self {
            torrents: Mutex::new(HashMap::new()),
            rules: Mutex::new(HashMap::new()),
            feeds: Mutex::new(HashMap::new()),
            categories: Mutex::new(HashSet::new()),
            prefs: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_mock_torrent(
        &self,
        name: &str,
        hash: Option<&str>,
        category: &str,
        state: &str,
        save_path: &str,
        files: Option<Vec<FileInfo>>,
    ) -> String {
        let hash = hash.map(|h| h.to_string()).unwrap_or_else(|| mock_hash(name));
        let entry = json!({
            "hash": hash,
            "name": name,
            "save_path": save_path,
            "category": category,
            "state": state,
            "progress": if state == "completed" { 1.0 } else { 0.0 },
            "tags": "ab:mock",
            "files": files,
        });
        self.torrents.lock().unwrap().insert(hash.clone(), entry);
        hash
    }

    pub fn get_state(&self) -> Value {
        let torrents = self.torrents.lock().unwrap();
        let rules = self.rules.lock().unwrap();
        let feeds = self.feeds.lock().unwrap();
        let categories = self.categories.lock().unwrap();
        let prefs = self.prefs.lock().unwrap();
        json!({
            "torrents": *torrents,
            "rules": *rules,
            "feeds": *feeds,
            "categories": categories.iter().collect::<Vec<_>>(),
            "prefs": *prefs,
        })
    }
}

impl Default for MockDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DownloaderClient for MockDownloader {
    async fn auth(&mut self) -> Result<bool, DownloaderError> {
        Ok(true)
    }

    async fn logout(&mut self) -> Result<(), DownloaderError> {
        Ok(())
    }

    async fn check_host(&self) -> Result<bool, DownloaderError> {
        Ok(true)
    }

    async fn add_torrents(
        &self,
        torrent_urls: Option<&[String]>,
        _torrent_files: Option<&[Vec<u8>]>,
        save_path: &str,
        category: &str,
        tags: Option<&str>,
    ) -> Result<bool, DownloaderError> {
        if let Some(urls) = torrent_urls {
            let mut map = self.torrents.lock().unwrap();
            for url in urls {
                let hash = mock_hash(url);
                let entry = json!({
                    "hash": hash,
                    "name": url.clone(),
                    "save_path": save_path,
                    "category": category,
                    "state": "downloading",
                    "progress": 0.0,
                    "tags": tags.unwrap_or(""),
                });
                map.insert(hash, entry);
            }
        }
        Ok(true)
    }

    async fn torrents_info(
        &self,
        status_filter: Option<&str>,
        category: Option<&str>,
        tag: Option<&str>,
    ) -> Result<Vec<TorrentInfo>, DownloaderError> {
        let map = self.torrents.lock().unwrap();
        Ok(map
            .values()
            .filter(|t| {
                if let Some(cat) = category {
                    if t["category"] != cat {
                        return false;
                    }
                }
                if let Some(tag_val) = tag {
                    if !t["tags"].as_str().unwrap_or("").contains(tag_val) {
                        return false;
                    }
                }
                if let Some(filter) = status_filter {
                    if filter == "completed" && t["state"] != "completed" {
                        return false;
                    }
                }
                true
            })
            .map(|t| TorrentInfo {
                hash: t["hash"].as_str().unwrap_or("").to_string(),
                name: t["name"].as_str().unwrap_or("").to_string(),
                save_path: t["save_path"].as_str().unwrap_or("").to_string(),
                category: t["category"].as_str().unwrap_or("").to_string(),
                state: t["state"].as_str().unwrap_or("").to_string(),
                progress: t["progress"].as_f64().unwrap_or(0.0),
                tags: t["tags"].as_str().unwrap_or("").to_string(),
            })
            .collect())
    }

    async fn torrents_files(&self, hash: &str) -> Result<Vec<FileInfo>, DownloaderError> {
        let map = self.torrents.lock().unwrap();
        Ok(map
            .get(hash)
            .and_then(|t| t["files"].as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| {
                        Some(FileInfo {
                            name: f["name"].as_str()?.to_string(),
                            size: f["size"].as_i64()?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn torrents_delete(&self, hash: &str, _delete_files: bool) -> Result<(), DownloaderError> {
        self.torrents.lock().unwrap().remove(hash);
        Ok(())
    }

    async fn torrents_pause(&self, _hash: &str) -> Result<(), DownloaderError> {
        Ok(())
    }

    async fn torrents_resume(&self, _hash: &str) -> Result<(), DownloaderError> {
        Ok(())
    }

    async fn torrents_rename_file(
        &self,
        _hash: &str,
        _old_path: &str,
        _new_path: &str,
        _verify: bool,
    ) -> Result<bool, DownloaderError> {
        Ok(true)
    }

    async fn move_torrent(&self, _hash: &str, _new_location: &str) -> Result<(), DownloaderError> {
        Ok(())
    }

    async fn get_torrent_path(&self, hash: &str) -> Result<String, DownloaderError> {
        let map = self.torrents.lock().unwrap();
        map.get(hash)
            .and_then(|t| t["save_path"].as_str().map(|s| s.to_string()))
            .ok_or_else(|| DownloaderError::Other("torrent not found".to_string()))
    }

    async fn set_category(&self, hash: &str, category: &str) -> Result<(), DownloaderError> {
        let mut map = self.torrents.lock().unwrap();
        if let Some(entry) = map.get_mut(hash) {
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("category".to_string(), Value::String(category.to_string()));
            }
        }
        Ok(())
    }

    async fn add_tag(&self, hash: &str, tag: &str) -> Result<(), DownloaderError> {
        let mut map = self.torrents.lock().unwrap();
        if let Some(entry) = map.get_mut(hash) {
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("tags".to_string(), Value::String(tag.to_string()));
            }
        }
        Ok(())
    }

    async fn rss_add_feed(
        &self,
        url: &str,
        item_path: Option<&str>,
    ) -> Result<(), DownloaderError> {
        let path = item_path.unwrap_or("Mikan_RSS").to_string();
        self.feeds
            .lock()
            .unwrap()
            .insert(path, Value::String(url.to_string()));
        Ok(())
    }

    async fn rss_remove_item(&self, item_path: &str) -> Result<(), DownloaderError> {
        self.feeds.lock().unwrap().remove(item_path);
        Ok(())
    }

    async fn rss_get_feeds(&self) -> Result<Value, DownloaderError> {
        let feeds = self.feeds.lock().unwrap();
        Ok(json!(&*feeds))
    }

    async fn rss_set_rule(&self, rule_name: &str, rule_def: Value) -> Result<(), DownloaderError> {
        self.rules
            .lock()
            .unwrap()
            .insert(rule_name.to_string(), rule_def);
        Ok(())
    }

    async fn get_download_rule(&self) -> Result<Value, DownloaderError> {
        let rules = self.rules.lock().unwrap();
        Ok(json!(&*rules))
    }

    async fn remove_rule(&self, rule_name: &str) -> Result<(), DownloaderError> {
        self.rules.lock().unwrap().remove(rule_name);
        Ok(())
    }

    async fn prefs_init(
        &self,
        prefs: HashMap<String, Value>,
    ) -> Result<(), DownloaderError> {
        *self.prefs.lock().unwrap() = prefs;
        Ok(())
    }

    async fn get_app_prefs(&self) -> Result<Value, DownloaderError> {
        let prefs = self.prefs.lock().unwrap();
        Ok(json!(&*prefs))
    }

    async fn add_category(&self, category: &str) -> Result<(), DownloaderError> {
        self.categories.lock().unwrap().insert(category.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn auth_returns_true() {
        let mut mock = MockDownloader::new();
        assert!(mock.auth().await.unwrap());
    }

    #[tokio::test]
    async fn logout_returns_ok() {
        let mut mock = MockDownloader::new();
        assert!(mock.logout().await.is_ok());
    }

    #[tokio::test]
    async fn add_torrents_with_url_stores_torrent() {
        let mock = MockDownloader::new();
        let urls = vec!["magnet:?xt=urn:btih:abc".to_string()];
        let ok = mock
            .add_torrents(Some(&urls), None, "/save", "anime", Some("tag1"))
            .await
            .unwrap();
        assert!(ok);

        let info = mock.torrents_info(None, None, None).await.unwrap();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "magnet:?xt=urn:btih:abc");
    }

    #[tokio::test]
    async fn torrents_info_filters_by_category() {
        let mock = MockDownloader::new();
        mock.add_mock_torrent("Anime A", None, "anime", "completed", "/save", None);
        mock.add_mock_torrent("Movie B", None, "movie", "downloading", "/save", None);

        let anime = mock.torrents_info(None, Some("anime"), None).await.unwrap();
        assert_eq!(anime.len(), 1);
        assert_eq!(anime[0].name, "Anime A");
    }

    #[tokio::test]
    async fn torrents_info_filters_by_tag() {
        let mock = MockDownloader::new();
        mock.add_mock_torrent("Anime A", None, "anime", "completed", "/save", None);

        // add_torrents with tag
        let urls = vec!["magnet:tag-test".to_string()];
        mock.add_torrents(Some(&urls), None, "/save", "anime", Some("custom_tag"))
            .await
            .unwrap();

        let tagged = mock.torrents_info(None, None, Some("custom_tag")).await.unwrap();
        assert_eq!(tagged.len(), 1);
    }

    #[tokio::test]
    async fn torrents_delete_removes_torrent() {
        let mock = MockDownloader::new();
        let hash = mock.add_mock_torrent("To Delete", None, "anime", "completed", "/save", None);

        let before = mock.torrents_info(None, None, None).await.unwrap();
        assert_eq!(before.len(), 1);

        mock.torrents_delete(&hash, false).await.unwrap();

        let after = mock.torrents_info(None, None, None).await.unwrap();
        assert_eq!(after.len(), 0);
    }

    #[tokio::test]
    async fn torrents_rename_file_returns_true() {
        let mock = MockDownloader::new();
        let ok = mock
            .torrents_rename_file("hash", "old", "new", false)
            .await
            .unwrap();
        assert!(ok);
    }

    #[tokio::test]
    async fn rss_set_rule_stores_rule() {
        let mock = MockDownloader::new();
        let rule = json!({"enable": true, "mustContain": "test"});
        mock.rss_set_rule("test_rule", rule.clone()).await.unwrap();

        let rules = mock.get_download_rule().await.unwrap();
        assert_eq!(rules["test_rule"], rule);
    }

    #[tokio::test]
    async fn add_mock_torrent_populates_state() {
        let mock = MockDownloader::new();
        let hash = mock.add_mock_torrent(
            "Test Torrent",
            Some("custom_hash_123"),
            "anime",
            "completed",
            "/downloads/path",
            Some(vec![
                FileInfo { name: "ep01.mkv".to_string(), size: 100 },
            ]),
        );
        assert_eq!(hash, "custom_hash_123");

        let info = mock.torrents_info(None, Some("anime"), None).await.unwrap();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "Test Torrent");
        assert_eq!(info[0].hash, "custom_hash_123");
        assert_eq!(info[0].state, "completed");
        assert_eq!(info[0].progress, 1.0);

        let files = mock.torrents_files(&hash).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "ep01.mkv");
    }
}
