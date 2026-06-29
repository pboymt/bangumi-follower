use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::time::sleep;

use crate::client::{DownloaderClient, TorrentInfo};
use crate::error::DownloaderError;
use crate::path::FileInfo;

pub struct Aria2Downloader {
    rpc_url: String,
    secret: String,
    client: Client,
    id: AtomicU64,
}

impl Aria2Downloader {
    pub fn new(host: &str, secret: &str) -> Result<Self, DownloaderError> {
        let rpc_url = if !host.contains("://") {
            format!("http://{}/jsonrpc", host)
        } else {
            format!("{}/jsonrpc", host.trim_end_matches('/'))
        };
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { rpc_url, secret: secret.to_string(), client, id: AtomicU64::new(0) })
    }

    async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value, DownloaderError> {
        let id = self.id.fetch_add(1, Ordering::SeqCst);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": format!("aria2.{}", method),
            "params": [format!("token:{}", self.secret), params],
        });
        let resp = self.client.post(&self.rpc_url).json(&payload).send().await?;
        let body: Value = resp.json().await?;
        if let Some(err) = body.get("error") {
            let msg = err["message"].as_str().unwrap_or("unknown error");
            return Err(DownloaderError::Other(msg.to_string()));
        }
        Ok(body["result"].clone())
    }

    async fn retry_call(
        &self,
        method: &str,
        params: Vec<Value>,
        max_retries: u32,
        delay: Duration,
    ) -> Result<Value, DownloaderError> {
        let mut last_err = None;
        for attempt in 0..max_retries {
            match self.call(method, params.clone()).await {
                Ok(val) => return Ok(val),
                Err(e) => {
                    last_err = Some(e);
                    if attempt < max_retries - 1 {
                        sleep(delay).await;
                    }
                }
            }
        }
        Err(last_err.unwrap())
    }
}

#[async_trait]
impl DownloaderClient for Aria2Downloader {
    async fn auth(&mut self) -> Result<bool, DownloaderError> {
        self.retry_call("getVersion", vec![], 3, Duration::from_secs(5))
            .await?;
        Ok(true)
    }

    async fn logout(&mut self) -> Result<(), DownloaderError> {
        Ok(())
    }

    async fn check_host(&self) -> Result<bool, DownloaderError> {
        self.call("getVersion", vec![]).await?;
        Ok(true)
    }

    async fn add_torrents(
        &self,
        torrent_urls: Option<&[String]>,
        torrent_files: Option<&[Vec<u8>]>,
        save_path: &str,
        _category: &str,
        _tags: Option<&str>,
    ) -> Result<bool, DownloaderError> {
        if let Some(urls) = torrent_urls {
            for url in urls {
                self.call(
                    "addUri",
                    vec![
                        json!([url]),
                        json!({"dir": save_path}),
                    ],
                ).await?;
            }
        }

        if let Some(files) = torrent_files {
            use base64::Engine;
            for data in files {
                let b64 = base64::engine::general_purpose::STANDARD.encode(data);
                self.call(
                    "addTorrent",
                    vec![
                        json!(b64),
                        json!({"dir": save_path}),
                    ],
                ).await?;
            }
        }

        Ok(true)
    }

    async fn torrents_info(
        &self,
        _status_filter: Option<&str>,
        _category: Option<&str>,
        _tag: Option<&str>,
    ) -> Result<Vec<TorrentInfo>, DownloaderError> {
        Err(DownloaderError::NotSupported(
            "torrents_info not supported for aria2".to_string(),
        ))
    }

    async fn torrents_files(&self, _hash: &str) -> Result<Vec<FileInfo>, DownloaderError> {
        Err(DownloaderError::NotSupported(
            "torrents_files not supported for aria2".to_string(),
        ))
    }

    async fn torrents_delete(
        &self,
        _hash: &str,
        _delete_files: bool,
    ) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "torrents_delete not supported for aria2".to_string(),
        ))
    }

    async fn torrents_pause(&self, _hash: &str) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "torrents_pause not supported for aria2".to_string(),
        ))
    }

    async fn torrents_resume(&self, _hash: &str) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "torrents_resume not supported for aria2".to_string(),
        ))
    }

    async fn torrents_rename_file(
        &self,
        _hash: &str,
        _old_path: &str,
        _new_path: &str,
        _verify: bool,
    ) -> Result<bool, DownloaderError> {
        Err(DownloaderError::NotSupported(
            "torrents_rename_file not supported for aria2".to_string(),
        ))
    }

    async fn move_torrent(
        &self,
        _hash: &str,
        _new_location: &str,
    ) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "move_torrent not supported for aria2".to_string(),
        ))
    }

    async fn get_torrent_path(&self, _hash: &str) -> Result<String, DownloaderError> {
        Err(DownloaderError::NotSupported(
            "get_torrent_path not supported for aria2".to_string(),
        ))
    }

    async fn set_category(&self, _hash: &str, _category: &str) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "set_category not supported for aria2".to_string(),
        ))
    }

    async fn add_tag(&self, _hash: &str, _tag: &str) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "add_tag not supported for aria2".to_string(),
        ))
    }

    async fn rss_add_feed(
        &self,
        _url: &str,
        _item_path: Option<&str>,
    ) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "rss_add_feed not supported for aria2".to_string(),
        ))
    }

    async fn rss_remove_item(&self, _item_path: &str) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "rss_remove_item not supported for aria2".to_string(),
        ))
    }

    async fn rss_get_feeds(&self) -> Result<Value, DownloaderError> {
        Err(DownloaderError::NotSupported(
            "rss_get_feeds not supported for aria2".to_string(),
        ))
    }

    async fn rss_set_rule(
        &self,
        _rule_name: &str,
        _rule_def: Value,
    ) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "rss_set_rule not supported for aria2".to_string(),
        ))
    }

    async fn get_download_rule(&self) -> Result<Value, DownloaderError> {
        Err(DownloaderError::NotSupported(
            "get_download_rule not supported for aria2".to_string(),
        ))
    }

    async fn remove_rule(&self, _rule_name: &str) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "remove_rule not supported for aria2".to_string(),
        ))
    }

    async fn prefs_init(
        &self,
        _prefs: HashMap<String, Value>,
    ) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "prefs_init not supported for aria2".to_string(),
        ))
    }

    async fn get_app_prefs(&self) -> Result<Value, DownloaderError> {
        Err(DownloaderError::NotSupported(
            "get_app_prefs not supported for aria2".to_string(),
        ))
    }

    async fn add_category(&self, _category: &str) -> Result<(), DownloaderError> {
        Err(DownloaderError::NotSupported(
            "add_category not supported for aria2".to_string(),
        ))
    }
}
