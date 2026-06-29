use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use tokio::time::sleep;

use crate::client::{DownloaderClient, TorrentInfo};
use crate::error::DownloaderError;
use crate::path::FileInfo;

pub struct QbDownloader {
    host: String,
    username: String,
    password: String,
    client: Client,
}

impl QbDownloader {
    pub fn new(host: &str, username: &str, password: &str, ssl: bool) -> Result<Self, DownloaderError> {
        let host = if !host.contains("://") {
            let scheme = if ssl { "https" } else { "http" };
            format!("{}://{}", scheme, host)
        } else {
            host.to_string()
        };
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { host, username: username.to_string(), password: password.to_string(), client })
    }

    fn url(&self, endpoint: &str) -> String {
        format!("{}/api/v2/{}", self.host, endpoint)
    }
}

#[async_trait]
impl DownloaderClient for QbDownloader {
    async fn auth(&mut self) -> Result<bool, DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .post(self.url("auth/login"))
                .form(&[
                    ("username", self.username.as_str()),
                    ("password", self.password.as_str()),
                ])
                .send()
                .await;
            match resp {
                Ok(r) if r.status().as_u16() == 403 => return Err(DownloaderError::AuthFailed),
                Ok(r) => return Ok(r.status().is_success()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn logout(&mut self) -> Result<(), DownloaderError> {
        let _ = self.client.post(self.url("auth/logout")).send().await;
        Ok(())
    }

    async fn check_host(&self) -> Result<bool, DownloaderError> {
        for attempt in 0..3 {
            let resp = self.client.get(self.url("app/version")).send().await;
            match resp {
                Ok(r) => return Ok(r.status().is_success()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn add_torrents(
        &self,
        torrent_urls: Option<&[String]>,
        torrent_files: Option<&[Vec<u8>]>,
        save_path: &str,
        category: &str,
        tags: Option<&str>,
    ) -> Result<bool, DownloaderError> {
        for attempt in 0..3 {
            let mut form = reqwest::multipart::Form::new()
                .text("savepath", save_path.to_string())
                .text("category", category.to_string())
                .text("contentLayout", "NoSubfolder".to_string());
            if let Some(urls) = torrent_urls {
                form = form.text("urls", urls.join("\n"));
            }
            if let Some(files) = torrent_files {
                for (i, data) in files.iter().enumerate() {
                    form = form.part(
                        format!("fileselect{}", i),
                        reqwest::multipart::Part::bytes(data.clone())
                            .file_name(format!("torrent{}.torrent", i)),
                    );
                }
            }
            if let Some(t) = tags {
                form = form.text("tags", t.to_string());
            }
            let resp = self.client.post(self.url("torrents/add")).multipart(form).send().await;
            match resp {
                Ok(r) => return Ok(r.status().is_success()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn torrents_info(
        &self,
        status_filter: Option<&str>,
        category: Option<&str>,
        tag: Option<&str>,
    ) -> Result<Vec<TorrentInfo>, DownloaderError> {
        for attempt in 0..3 {
            let mut params: Vec<(&str, &str)> = Vec::new();
            if let Some(f) = status_filter {
                params.push(("filter", f));
            }
            if let Some(c) = category {
                params.push(("category", c));
            }
            if let Some(t) = tag {
                params.push(("tag", t));
            }
            let resp = self.client.get(self.url("torrents/info")).query(&params).send().await;
            match resp {
                Ok(r) => {
                    let info: Vec<TorrentInfo> = r.json().await?;
                    return Ok(info);
                }
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn torrents_files(&self, hash: &str) -> Result<Vec<FileInfo>, DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .get(self.url("torrents/files"))
                .query(&[("hash", hash)])
                .send()
                .await;
            match resp {
                Ok(r) => {
                    #[derive(serde::Deserialize)]
                    struct RawFile {
                        name: String,
                        size: i64,
                    }
                    let raw: Vec<RawFile> = r.json().await?;
                    return Ok(raw.into_iter().map(|f| FileInfo { name: f.name, size: f.size }).collect());
                }
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn torrents_delete(&self, hash: &str, delete_files: bool) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let delete_val = if delete_files { "true" } else { "false" };
            let resp = self
                .client
                .post(self.url("torrents/delete"))
                .form(&[("hashes", hash), ("deleteFiles", delete_val)])
                .send()
                .await;
            match resp {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn torrents_pause(&self, hash: &str) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .post(self.url("torrents/pause"))
                .form(&[("hashes", hash)])
                .send()
                .await;
            match resp {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn torrents_resume(&self, hash: &str) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .post(self.url("torrents/resume"))
                .form(&[("hashes", hash)])
                .send()
                .await;
            match resp {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn torrents_rename_file(
        &self,
        hash: &str,
        old_path: &str,
        new_path: &str,
        verify: bool,
    ) -> Result<bool, DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .post(self.url("torrents/renameFile"))
                .form(&[("hash", hash), ("oldPath", old_path), ("newPath", new_path)])
                .send()
                .await;
            match resp {
                Ok(_) => {
                    if !verify {
                        return Ok(true);
                    }
                    for v_attempt in 0..3 {
                        sleep(Duration::from_millis(500 * 2u64.pow(v_attempt))).await;
                        let files = self.torrents_files(hash).await?;
                        if files.iter().any(|f| f.name == new_path) {
                            return Ok(true);
                        }
                    }
                    return Ok(false);
                }
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn move_torrent(&self, hash: &str, new_location: &str) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .post(self.url("torrents/setLocation"))
                .form(&[("hashes", hash), ("location", new_location)])
                .send()
                .await;
            match resp {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn get_torrent_path(&self, hash: &str) -> Result<String, DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .get(self.url("torrents/info"))
                .query(&[("hashes", hash)])
                .send()
                .await;
            match resp {
                Ok(r) => {
                    #[derive(serde::Deserialize)]
                    struct TorrentEntry {
                        save_path: String,
                    }
                    let list: Vec<TorrentEntry> = r.json().await?;
                    return list
                        .into_iter()
                        .next()
                        .map(|e| e.save_path)
                        .ok_or_else(|| DownloaderError::Other("torrent not found".to_string()));
                }
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn set_category(&self, hash: &str, category: &str) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .post(self.url("torrents/setCategory"))
                .form(&[("hashes", hash), ("category", category)])
                .send()
                .await;
            match resp {
                Ok(r) if r.status().as_u16() == 409 => {
                    self.client
                        .post(self.url("torrents/createCategory"))
                        .form(&[("category", category)])
                        .send()
                        .await?;
                    self.client
                        .post(self.url("torrents/setCategory"))
                        .form(&[("hashes", hash), ("category", category)])
                        .send()
                        .await?;
                    return Ok(());
                }
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn add_tag(&self, hash: &str, tag: &str) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .post(self.url("torrents/addTags"))
                .form(&[("hashes", hash), ("tags", tag)])
                .send()
                .await;
            match resp {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn rss_add_feed(&self, url: &str, item_path: Option<&str>) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let path = item_path.unwrap_or("Mikan_RSS");
            let resp = self
                .client
                .post(self.url("rss/addFeed"))
                .form(&[("url", url), ("path", path)])
                .send()
                .await;
            match resp {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn rss_remove_item(&self, item_path: &str) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .post(self.url("rss/removeItem"))
                .form(&[("path", item_path)])
                .send()
                .await;
            match resp {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn rss_get_feeds(&self) -> Result<Value, DownloaderError> {
        for attempt in 0..3 {
            let resp = self.client.get(self.url("rss/items")).send().await;
            match resp {
                Ok(r) => {
                    let val: Value = r.json().await?;
                    return Ok(val);
                }
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn rss_set_rule(&self, rule_name: &str, rule_def: Value) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .post(self.url("rss/setRule"))
                .form(&[("ruleName", rule_name), ("ruleDef", &rule_def.to_string())])
                .send()
                .await;
            match resp {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn get_download_rule(&self) -> Result<Value, DownloaderError> {
        for attempt in 0..3 {
            let resp = self.client.get(self.url("rss/rules")).send().await;
            match resp {
                Ok(r) => {
                    let val: Value = r.json().await?;
                    return Ok(val);
                }
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn remove_rule(&self, rule_name: &str) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .post(self.url("rss/removeRule"))
                .form(&[("ruleName", rule_name)])
                .send()
                .await;
            match resp {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn prefs_init(&self, prefs: HashMap<String, Value>) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let json = serde_json::to_value(&prefs)
                .map_err(|e| DownloaderError::Other(e.to_string()))?;
            let resp = self
                .client
                .post(self.url("app/setPreferences"))
                .form(&[("json", &json.to_string())])
                .send()
                .await;
            match resp {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn get_app_prefs(&self) -> Result<Value, DownloaderError> {
        for attempt in 0..3 {
            let resp = self.client.get(self.url("app/preferences")).send().await;
            match resp {
                Ok(r) => {
                    let val: Value = r.json().await?;
                    return Ok(val);
                }
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn add_category(&self, category: &str) -> Result<(), DownloaderError> {
        for attempt in 0..3 {
            let resp = self
                .client
                .post(self.url("torrents/createCategory"))
                .form(&[("category", category)])
                .send()
                .await;
            match resp {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < 2 {
                        sleep(Duration::from_secs(2)).await;
                    } else {
                        return Err(DownloaderError::Http(e));
                    }
                }
            }
        }
        unreachable!()
    }
}
