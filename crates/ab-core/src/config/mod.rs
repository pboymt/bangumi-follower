pub mod model;
pub mod parser;
pub mod consts;
pub mod logging;

use std::path::PathBuf;
use std::sync::RwLock;
use once_cell::sync::Lazy;
use model::AppConfig;
use crate::config::parser::{parse_config, serialize_config};

pub struct Settings {
    inner: RwLock<AppConfig>,
    config_path: PathBuf,
}

fn default_config_path() -> PathBuf {
    let path = if cfg!(debug_assertions) {
        "config/config_dev.json"
    } else {
        "config/config.json"
    };
    PathBuf::from(path)
}

fn env_override(config: &mut AppConfig) {
    if let Ok(val) = std::env::var("AB_INTERVAL_TIME") {
        if let Ok(v) = val.parse::<u64>() {
            config.program.rss_time = v;
        }
    }
    if let Ok(val) = std::env::var("AB_RENAME_FREQ") {
        if let Ok(v) = val.parse::<u64>() {
            config.program.rename_time = v;
        }
    }
    if let Ok(val) = std::env::var("AB_WEBUI_PORT") {
        if let Ok(v) = val.parse::<u16>() {
            config.program.webui_port = v;
        }
    }
    if let Ok(val) = std::env::var("AB_DOWNLOADER_HOST") {
        config.downloader.host_ = val;
    }
    if let Ok(val) = std::env::var("AB_DOWNLOADER_USERNAME") {
        config.downloader.username_ = val;
    }
    if let Ok(val) = std::env::var("AB_DOWNLOADER_PASSWORD") {
        config.downloader.password_ = val;
    }
    if let Ok(val) = std::env::var("AB_DOWNLOAD_PATH") {
        config.downloader.path = val;
    }
    if let Ok(val) = std::env::var("AB_RSS_COLLECTOR") {
        config.rss_parser.enable = val == "true" || val == "1";
    }
    if let Ok(val) = std::env::var("AB_NOT_CONTAIN") {
        config.rss_parser.filter = val.split('|').map(|s| s.to_string()).collect();
    }
    if let Ok(val) = std::env::var("AB_LANGUAGE") {
        config.rss_parser.language = val;
    }
    if let Ok(val) = std::env::var("AB_RENAME") {
        config.bangumi_manage.enable = val == "true" || val == "1";
    }
    if let Ok(val) = std::env::var("AB_METHOD") {
        config.bangumi_manage.rename_method = val.to_lowercase();
    }
    if let Ok(val) = std::env::var("AB_GROUP_TAG") {
        config.bangumi_manage.group_tag = val == "true" || val == "1";
    }
    if let Ok(val) = std::env::var("AB_EP_COMPLETE") {
        config.bangumi_manage.eps_complete = val == "true" || val == "1";
    }
    if let Ok(val) = std::env::var("AB_REMOVE_BAD_BT") {
        config.bangumi_manage.remove_bad_torrent = val == "true" || val == "1";
    }
    if let Ok(val) = std::env::var("AB_DEBUG_MODE") {
        config.log.debug_enable = val == "true" || val == "1";
    }
    if let Ok(val) = std::env::var("AB_TMDB_API_KEY") {
        config.tmdb_api_key = val;
    }
    if let Ok(val) = std::env::var("AB_HTTP_PROXY") {
        if !val.is_empty() {
            config.proxy.enable = true;
            config.proxy.r#type = model::ProxyType::Http;
            if let Some(colon) = val.rfind(':') {
                let host = &val[..colon];
                let port_str = &val[colon + 1..];
                config.proxy.host = host.to_string();
                if let Ok(p) = port_str.parse::<u16>() {
                    config.proxy.port = p;
                }
            }
        }
    }
}

impl Settings {
    pub fn load() -> Result<Self, String> {
        let config_path = default_config_path();
        let mut config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .map_err(|e| format!("read config failed: {e}"))?;
            parse_config(&content)?
        } else {
            let config = AppConfig::default();
            let json = serialize_config(&config)?;
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("create config dir failed: {e}"))?;
            }
            std::fs::write(&config_path, &json)
                .map_err(|e| format!("write default config failed: {e}"))?;
            config
        };

        env_override(&mut config);

        Ok(Self {
            inner: RwLock::new(config),
            config_path,
        })
    }

    pub fn save(&self) -> Result<(), String> {
        let config = self.inner.read().map_err(|e| format!("lock error: {e}"))?;
        let json = serialize_config(&config)?;
        std::fs::write(&self.config_path, &json)
            .map_err(|e| format!("write config failed: {e}"))
    }

    pub fn get(&self) -> std::sync::RwLockReadGuard<'_, AppConfig> {
        self.inner.read().expect("config lock poisoned")
    }

    pub fn get_ref(&self) -> std::sync::RwLockReadGuard<'_, AppConfig> {
        self.inner.read().expect("config lock poisoned")
    }

    pub fn set(&self, config: AppConfig) -> Result<(), String> {
        {
            let mut w = self.inner.write().map_err(|e| format!("lock error: {e}"))?;
            *w = config;
        }
        self.save()
    }

    pub fn program(&self) -> model::Program {
        self.get().program.clone()
    }
    pub fn downloader(&self) -> model::Downloader {
        self.get().downloader.clone()
    }
    pub fn rss_parser(&self) -> model::RssParser {
        self.get().rss_parser.clone()
    }
    pub fn bangumi_manage(&self) -> model::BangumiManage {
        self.get().bangumi_manage.clone()
    }
    pub fn log(&self) -> model::Log {
        self.get().log.clone()
    }
    pub fn proxy(&self) -> model::Proxy {
        self.get().proxy.clone()
    }
    pub fn notification(&self) -> model::Notification {
        self.get().notification.clone()
    }
    pub fn security(&self) -> model::Security {
        self.get().security.clone()
    }
    pub fn tmdb_api_key(&self) -> String {
        self.get().tmdb_api_key.clone()
    }
    pub fn resolve_tmdb_api_key(&self) -> String {
        let from_config = self.get().tmdb_api_key.clone();
        if !from_config.is_empty() {
            return from_config;
        }
        std::env::var("AB_TMDB_API_KEY")
            .unwrap_or_else(|_| crate::config::consts::TMDB_API.to_string())
    }
}

pub static SETTINGS: Lazy<Settings> = Lazy::new(|| {
    dotenvy::dotenv().ok();
    Settings::load().expect("Failed to load config")
});
