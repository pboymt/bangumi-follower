use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DownloaderType {
    Qbittorrent,
    Aria2,
    Mock,
    Transmission,
}

impl Default for DownloaderType {
    fn default() -> Self {
        Self::Qbittorrent
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    Http,
    Socks,
}

impl Default for ProxyType {
    fn default() -> Self {
        Self::Http
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OpenAiType {
    Openai,
    Azure,
}

impl Default for OpenAiType {
    fn default() -> Self {
        Self::Openai
    }
}

fn default_rss_time() -> u64 {
    900
}
fn default_rename_time() -> u64 {
    60
}
fn default_webui_port() -> u16 {
    7892
}
fn default_true() -> bool {
    true
}
fn default_language() -> String {
    "zh".to_string()
}
fn default_rename_method() -> String {
    "pn".to_string()
}
fn default_openai_base() -> String {
    "https://api.openai.com/v1".to_string()
}
fn default_openai_version() -> String {
    "2024-01-01".to_string()
}
fn default_openai_model() -> String {
    "gpt-4o-mini".to_string()
}
fn default_mcp_whitelist() -> Vec<String> {
    vec![
        "192.168.0.0/16".into(),
        "10.0.0.0/8".into(),
        "172.16.0.0/12".into(),
        "127.0.0.1/32".into(),
    ]
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Program {
    #[serde(default = "default_rss_time")]
    pub rss_time: u64,
    #[serde(default = "default_rename_time")]
    pub rename_time: u64,
    #[serde(default = "default_webui_port")]
    pub webui_port: u16,
}

impl Default for Program {
    fn default() -> Self {
        Self {
            rss_time: default_rss_time(),
            rename_time: default_rename_time(),
            webui_port: default_webui_port(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Downloader {
    #[serde(default)]
    pub r#type: DownloaderType,
    #[serde(alias = "host", default)]
    pub host_: String,
    #[serde(alias = "username", default)]
    pub username_: String,
    #[serde(alias = "password", default)]
    pub password_: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub ssl: bool,
}

impl Default for Downloader {
    fn default() -> Self {
        Self {
            r#type: DownloaderType::default(),
            host_: String::new(),
            username_: String::new(),
            password_: String::new(),
            path: String::new(),
            ssl: false,
        }
    }
}

impl Downloader {
    pub fn host(&self) -> String {
        expand(&self.host_)
    }
    pub fn username(&self) -> String {
        expand(&self.username_)
    }
    pub fn password(&self) -> String {
        expand(&self.password_)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RssParser {
    #[serde(default = "default_true")]
    pub enable: bool,
    #[serde(default)]
    pub filter: Vec<String>,
    #[serde(default = "default_language")]
    pub language: String,
}

impl Default for RssParser {
    fn default() -> Self {
        Self {
            enable: true,
            filter: vec!["720".to_string(), "\\d+-\\d+".to_string()],
            language: "zh".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BangumiManage {
    #[serde(default = "default_true")]
    pub enable: bool,
    #[serde(default)]
    pub eps_complete: bool,
    #[serde(default = "default_rename_method")]
    pub rename_method: String,
    #[serde(default)]
    pub group_tag: bool,
    #[serde(default)]
    pub remove_bad_torrent: bool,
}

impl Default for BangumiManage {
    fn default() -> Self {
        Self {
            enable: true,
            eps_complete: false,
            rename_method: "pn".to_string(),
            group_tag: false,
            remove_bad_torrent: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Log {
    #[serde(default)]
    pub debug_enable: bool,
}

impl Default for Log {
    fn default() -> Self {
        Self {
            debug_enable: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Proxy {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub r#type: ProxyType,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(alias = "username", default)]
    pub username_: String,
    #[serde(alias = "password", default)]
    pub password_: String,
}

impl Default for Proxy {
    fn default() -> Self {
        Self {
            enable: false,
            r#type: ProxyType::Http,
            host: String::new(),
            port: 0,
            username_: String::new(),
            password_: String::new(),
        }
    }
}

impl Proxy {
    pub fn username(&self) -> String {
        expand(&self.username_)
    }
    pub fn password(&self) -> String {
        expand(&self.password_)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotificationProvider {
    #[serde(default)]
    pub r#type: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(alias = "token", default)]
    pub token_: Option<String>,
    #[serde(alias = "chat_id", default)]
    pub chat_id_: Option<String>,
    #[serde(alias = "webhook_url", default)]
    pub webhook_url_: Option<String>,
    #[serde(alias = "server_url", default)]
    pub server_url_: Option<String>,
    #[serde(alias = "device_key", default)]
    pub device_key_: Option<String>,
    #[serde(alias = "user_key", default)]
    pub user_key_: Option<String>,
    #[serde(alias = "api_token", default)]
    pub api_token_: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(alias = "url", default)]
    pub url_: Option<String>,
}

impl NotificationProvider {
    pub fn token(&self) -> Option<String> {
        self.token_.as_ref().map(|s| expand(s))
    }
    pub fn chat_id(&self) -> Option<String> {
        self.chat_id_.as_ref().map(|s| expand(s))
    }
    pub fn webhook_url(&self) -> Option<String> {
        self.webhook_url_.as_ref().map(|s| expand(s))
    }
    pub fn server_url(&self) -> Option<String> {
        self.server_url_.as_ref().map(|s| expand(s))
    }
    pub fn device_key(&self) -> Option<String> {
        self.device_key_.as_ref().map(|s| expand(s))
    }
    pub fn user_key(&self) -> Option<String> {
        self.user_key_.as_ref().map(|s| expand(s))
    }
    pub fn api_token(&self) -> Option<String> {
        self.api_token_.as_ref().map(|s| expand(s))
    }
    pub fn url(&self) -> Option<String> {
        self.url_.as_ref().map(|s| expand(s))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Notification {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub providers: Vec<NotificationProvider>,
}

impl Default for Notification {
    fn default() -> Self {
        Self {
            enable: false,
            providers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperimentalOpenAi {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_openai_base")]
    pub api_base: String,
    #[serde(default)]
    pub api_type: OpenAiType,
    #[serde(default = "default_openai_version")]
    pub api_version: String,
    #[serde(default = "default_openai_model")]
    pub model: String,
    #[serde(default)]
    pub deployment_id: String,
}

impl Default for ExperimentalOpenAi {
    fn default() -> Self {
        Self {
            enable: false,
            api_key: String::new(),
            api_base: default_openai_base(),
            api_type: OpenAiType::Openai,
            api_version: default_openai_version(),
            model: default_openai_model(),
            deployment_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Security {
    #[serde(default)]
    pub login_whitelist: Vec<String>,
    #[serde(default)]
    pub login_tokens: Vec<String>,
    #[serde(default = "default_mcp_whitelist")]
    pub mcp_whitelist: Vec<String>,
    #[serde(default)]
    pub mcp_tokens: Vec<String>,
}

impl Default for Security {
    fn default() -> Self {
        Self {
            login_whitelist: Vec::new(),
            login_tokens: Vec::new(),
            mcp_whitelist: default_mcp_whitelist(),
            mcp_tokens: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    #[serde(default)]
    pub program: Program,
    #[serde(default)]
    pub downloader: Downloader,
    #[serde(default)]
    pub rss_parser: RssParser,
    #[serde(default)]
    pub bangumi_manage: BangumiManage,
    #[serde(default)]
    pub log: Log,
    #[serde(default)]
    pub proxy: Proxy,
    #[serde(default)]
    pub notification: Notification,
    #[serde(default)]
    pub experimental_openai: ExperimentalOpenAi,
    #[serde(default)]
    pub security: Security,
    #[serde(default)]
    pub tmdb_api_key: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            program: Program::default(),
            downloader: Downloader::default(),
            rss_parser: RssParser::default(),
            bangumi_manage: BangumiManage::default(),
            log: Log::default(),
            proxy: Proxy::default(),
            notification: Notification::default(),
            experimental_openai: ExperimentalOpenAi::default(),
            security: Security::default(),
            tmdb_api_key: String::new(),
        }
    }
}

pub fn expand(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            let mut var_name = String::new();
            if chars.peek() == Some(&'{') {
                chars.next();
                while let Some(&ch) = chars.peek() {
                    if ch == '}' {
                        chars.next();
                        break;
                    }
                    var_name.push(ch);
                    chars.next();
                }
            } else {
                while let Some(&ch) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '_' {
                        var_name.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            let val = std::env::var(&var_name).unwrap_or_default();
            result.push_str(&val);
        } else {
            result.push(c);
        }
    }
    result
}
