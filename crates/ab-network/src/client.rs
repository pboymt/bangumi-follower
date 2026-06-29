use reqwest::{Client, Proxy, Response};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use serde_json::Value;
use crate::error::NetworkError;

const DEFAULT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

static SHARED_CLIENT: RwLock<Option<Arc<Client>>> = RwLock::new(None);

#[derive(Debug, Clone)]
pub struct RssEntry {
    pub title: String,
    pub url: String,
    pub homepage: String,
}

fn build_client(proxy_url: Option<&str>, proxy_user: Option<&str>, proxy_pass: Option<&str>) -> Client {
    let mut builder = Client::builder()
        .user_agent(DEFAULT_UA)
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(4)
        .connect_timeout(Duration::from_secs(10))
        .pool_idle_timeout(Duration::from_secs(10));
    if let Some(url) = proxy_url {
        if let Ok(mut proxy) = Proxy::all(url) {
            if let (Some(user), Some(pass)) = (proxy_user, proxy_pass) {
                if !user.is_empty() && !pass.is_empty() {
                    proxy = proxy.basic_auth(user, pass);
                }
            }
            builder = builder.proxy(proxy);
        }
    }
    builder.build().expect("reqwest client build failed")
}

pub fn reset_shared_client(proxy_url: Option<&str>, proxy_user: Option<&str>, proxy_pass: Option<&str>) {
    let client = Arc::new(build_client(proxy_url, proxy_user, proxy_pass));
    *SHARED_CLIENT.write().unwrap() = Some(client);
}

pub fn get_shared_client() -> Arc<Client> {
    if let Some(ref client) = *SHARED_CLIENT.read().unwrap() {
        return client.clone();
    }
    let client = Arc::new(build_client(None, None, None));
    *SHARED_CLIENT.write().unwrap() = Some(client.clone());
    client
}

fn build_headers(is_torrent: bool) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9".parse().unwrap());
    headers.insert(reqwest::header::ACCEPT_ENCODING, "gzip, deflate".parse().unwrap());
    headers.insert(reqwest::header::CONNECTION, "keep-alive".parse().unwrap());
    let accept = if is_torrent {
        "application/x-bittorrent, application/octet-stream, */*"
    } else {
        "application/xml, text/xml, */*"
    };
    headers.insert(reqwest::header::ACCEPT, accept.parse().unwrap());
    headers
}

pub struct NetworkClient {
    client: Arc<Client>,
}

impl NetworkClient {
    pub fn new(proxy_url: Option<&str>, proxy_user: Option<&str>, proxy_pass: Option<&str>) -> Self {
        Self {
            client: Arc::new(build_client(proxy_url, proxy_user, proxy_pass)),
        }
    }

    pub fn from_client(client: Arc<Client>) -> Self {
        Self { client }
    }

    pub async fn get_url(&self, url: &str, retry: u32) -> Result<Response, NetworkError> {
        let mut last_err = None;
        for attempt in 0..=retry {
            match self.client.get(url)
                .headers(build_headers(url.contains(".torrent") || url.contains("/download/")))
                .send().await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        return Ok(resp);
                    }
                    if resp.status().is_client_error() {
                        return Err(NetworkError::Http(resp.error_for_status().unwrap_err()));
                    }
                    last_err = Some(NetworkError::Http(resp.error_for_status().unwrap_err()));
                }
                Err(e) => {
                    last_err = Some(NetworkError::Http(e));
                }
            }
            if attempt < retry {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
        Err(last_err.unwrap_or(NetworkError::ConnectionFailed))
    }

    pub async fn post_json(&self, url: &str, data: &Value, retry: u32) -> Result<Response, NetworkError> {
        let mut last_err = None;
        for attempt in 0..=retry {
            match self.client.post(url)
                .json(data)
                .headers(build_headers(false))
                .send().await
            {
                Ok(resp) => return Ok(resp),
                Err(e) => last_err = Some(NetworkError::Http(e)),
            }
            if attempt < retry {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
        Err(last_err.unwrap_or(NetworkError::ConnectionFailed))
    }

    pub async fn post_form(&self, url: &str, data: &std::collections::HashMap<String, String>, retry: u32) -> Result<Response, NetworkError> {
        let mut last_err = None;
        for attempt in 0..=retry {
            match self.client.post(url)
                .form(data)
                .headers(build_headers(false))
                .send().await
            {
                Ok(resp) => return Ok(resp),
                Err(e) => last_err = Some(NetworkError::Http(e)),
            }
            if attempt < retry {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
        Err(last_err.unwrap_or(NetworkError::ConnectionFailed))
    }

    pub async fn post_multipart(&self, url: &str, fields: &[(&str, &str)], files: &[(&str, &[u8], &str)], retry: u32) -> Result<Response, NetworkError> {
        let mut last_err = None;
        for attempt in 0..=retry {
            let mut form = reqwest::multipart::Form::new();
            for (k, v) in fields {
                form = form.text(k.to_string(), v.to_string());
            }
            for (k, data, filename) in files {
                form = form.part(k.to_string(), reqwest::multipart::Part::bytes(data.to_vec()).file_name(filename.to_string()));
            }
            match self.client.post(url)
                .multipart(form)
                .send().await
            {
                Ok(resp) => return Ok(resp),
                Err(e) => last_err = Some(NetworkError::Http(e)),
            }
            if attempt < retry {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
        Err(last_err.unwrap_or(NetworkError::ConnectionFailed))
    }

    pub async fn check_url(&self, url: &str) -> Result<bool, NetworkError> {
        match self.client.head(url)
            .headers(build_headers(false))
            .send().await
        {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    pub async fn get_xml(&self, url: &str, retry: u32) -> Result<String, NetworkError> {
        let resp = self.get_url(url, retry).await?;
        let text = resp.text().await.map_err(NetworkError::Http)?;
        Ok(text)
    }

    pub async fn get_json(&self, url: &str) -> Result<Value, NetworkError> {
        let resp = self.client.get(url)
            .headers(build_headers(false))
            .send().await?;
        let json = resp.json::<Value>().await?;
        Ok(json)
    }

    pub async fn get_html(&self, url: &str) -> Result<String, NetworkError> {
        self.get_xml(url, 1).await
    }

    pub async fn get_content(&self, url: &str) -> Result<Vec<u8>, NetworkError> {
        let resp = self.get_url(url, 1).await?;
        let bytes = resp.bytes().await.map_err(NetworkError::Http)?;
        Ok(bytes.to_vec())
    }

    pub async fn get_torrents(&self, url: &str, filter: Option<&str>, limit: Option<u32>) -> Result<Vec<RssEntry>, NetworkError> {
        let xml = self.get_xml(url, 2).await?;
        let entries = crate::site::mikan::parse_rss(&xml)?;
        let filtered: Vec<RssEntry> = if let Some(f) = filter {
            let re = regex::Regex::new(f).unwrap_or(regex::Regex::new(&regex::escape(f)).unwrap());
            entries.into_iter().filter(|e| !re.is_match(&e.title)).collect()
        } else {
            entries
        };
        Ok(match limit {
            Some(l) => filtered.into_iter().take(l as usize).collect(),
            None => filtered,
        })
    }

    pub async fn get_rss_title(&self, url: &str) -> Result<String, NetworkError> {
        let xml = self.get_xml(url, 2).await?;
        let title = crate::site::mikan::parse_rss_title(&xml)?;
        Ok(title)
    }
}
