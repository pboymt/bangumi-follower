use std::sync::Mutex;
use lru::LruCache;
use once_cell::sync::Lazy;
use ab_network::NetworkClient;
use crate::error::ParserError;

static MIKAN_CACHE: Lazy<Mutex<LruCache<String, (String, String)>>> = Lazy::new(|| {
    Mutex::new(LruCache::new(512.try_into().unwrap()))
});

pub trait ImageSaver: Send + Sync {
    fn save(&self, data: &[u8], suffix: &str) -> String;
}

pub async fn mikan_parse(
    client: &NetworkClient,
    homepage: &str,
    image_saver: &dyn ImageSaver,
) -> Result<(String, String), ParserError> {
    {
        let mut cache = MIKAN_CACHE.lock().unwrap();
        if let Some(result) = cache.get(homepage) {
            return Ok(result.clone());
        }
    }

    let html = client.get_html(homepage).await.map_err(ParserError::Network)?;

    let mut poster_link = String::new();
    let mut official_title = String::new();

    for line in html.lines() {
        if line.contains("bangumi-poster") {
            if let Some(start) = line.find("url(") {
                let url_start = start + 4;
                if let Some(end) = line[url_start..].find(')') {
                    poster_link = line[url_start..url_start + end].trim_matches('\'').trim_matches('"').to_string();
                }
            }
        }
        if line.contains("bangumi-title") && official_title.is_empty() {
            if let Some(href_start) = line.find("/Home/Bangumi/") {
                let after_href = &line[href_start..];
                if let Some(gt) = after_href.find('>') {
                    let after_gt = &after_href[gt+1..];
                    if let Some(lt) = after_gt.find('<') {
                        official_title = after_gt[..lt].trim().to_string();
                    }
                }
            }
        }
    }

    // Strip season suffix
    if let Some(pos) = official_title.find("第") {
        if official_title[pos..].contains("季") {
            official_title = official_title[..pos].trim().to_string();
        }
    }

    // Download poster
    if !poster_link.is_empty() {
        if let Ok(data) = client.get_content(&poster_link).await {
            let suffix = std::path::Path::new(&poster_link)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpg");
            image_saver.save(&data, suffix);
        }
    }

    let result = (poster_link, official_title.clone());
    let mut cache = MIKAN_CACHE.lock().unwrap();
    cache.put(homepage.to_string(), result.clone());
    Ok(result)
}
