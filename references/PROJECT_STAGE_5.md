# Stage 5: ab-downloader — Downloader Client Abstraction + Path Utilities

## Objective
Migrate 7 Python files (path utilities, downloader client abstraction, 3 client implementations + mock) to a single async crate. Define a `DownloaderClient` trait with qBittorrent, Aria2, and Mock implementations. Keep path utilities (`TorrentPath`) as a standalone module reused by `ab-manager`.

## Dependencies
- **ab-core** (Stage 1) — `Config` (`DownloaderConfig`, `BangumiManageConfig`, `PLATFORM`)
- **ab-database** (Stage 2) — `Bangumi`, `Torrent` model types (for method signatures)
- **ab-network** (Stage 3) — `NetworkClient` for torrent file fetching
- **External**: `reqwest` (with `json` feature — qB/aria2 API calls), `tracing`, `thiserror`

## Python Sources → Crate Layout

```
ab-downloader/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Re-exports: DownloadClient, TorrentPath
│   ├── error.rs                  # DownloaderError + ConflictError
│   ├── path.rs                   # TorrentPath (path generation, file classification)
│   ├── client.rs                 # DownloaderClient trait + DownloadClient orchestration
│   ├── qb.rs                     # QbDownloader (qBittorrent Web API v2)
│   ├── aria2.rs                  # Aria2Downloader (JSON-RPC)
│   └── mock.rs                   # MockDownloader (in-memory for testing)
```

### Source Mapping

| Python file | Rust target | Lines |
|-------------|-------------|-------|
| `downloader/path.py` | `path.rs` | ~80 |
| `downloader/download_client.py` | `client.rs` (orchestration) + `lib.rs` | ~200 |
| `downloader/exceptions.py` | `error.rs` | ~10 |
| `downloader/client/qb_downloader.py` | `qb.rs` | ~300 |
| `downloader/client/aria2_downloader.py` | `aria2.rs` | ~120 |
| `downloader/client/mock_downloader.py` | `mock.rs` | ~200 |
| `downloader/client/tr_downloader.py` | (omit — empty file) | — |
| (new) | `lib.rs` | ~15 |
| (new) | `client.rs` (trait definition) | ~20 |

**Deferred**: `downloader/client/tr_downloader.py` is an empty stub — not migrated.

**Total**: ~945 lines Rust, 7 files.

## Crate Design

### `src/error.rs` — `DownloaderError`
```rust
#[derive(thiserror::Error, Debug)]
pub enum DownloaderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("network error: {0}")]
    Network(#[from] ab_network::NetworkError),
    #[error("authentication failed")]
    AuthFailed,
    #[error("unsupported downloader type: {0}")]
    UnsupportedType(String),
    #[error("conflict: {0}")]                   // HTTP 409 from qBittorrent
    Conflict(String),
    #[error("operation not supported: {0}")]
    NotSupported(String),
    #[error("{0}")]
    Other(String),
}
```

### `src/path.rs` — `TorrentPath` (pure logic, no I/O)

Mirrors `path.py` exactly. Free functions instead of a struct (since all methods are static in Python).

```rust
/// Media file suffixes (lowercase)
const MEDIA_SUFFIXES: &[&str] = &[".mp4", ".mkv"];
/// Subtitle file suffixes (lowercase)
const SUBTITLE_SUFFIXES: &[&str] = &[".ass", ".srt"];

/// Classify a file list into media and subtitle groups by suffix.
pub fn check_files(files: &[FileInfo]) -> (Vec<String>, Vec<String>);

/// Extract bangumi name and season from a save path.
pub fn path_to_bangumi(save_path: &str, torrent_name: &str) -> (String, i32);

/// Compute the file depth for a torrent file path.
pub fn file_depth(file_path: &str) -> usize;

/// Returns true if the file path is <= 2 levels deep (i.e., an episode file).
pub fn is_ep(file_path: &str) -> bool;

/// Generate the save path for a bangumi entry.
/// Uses adjusted season number (season + season_offset).
pub fn gen_save_path(data: &Bangumi, downloader_path: &str) -> String;

/// Generate the qBittorrent RSS rule name for a bangumi.
pub fn rule_name(data: &Bangumi, group_tag: bool) -> String;

/// Join path components using the platform separator.
pub fn join_path(parts: &[&str]) -> String;
```

**`FileInfo`** — simplified file info struct (what qBittorrent returns per file):
```rust
pub struct FileInfo {
    pub name: String,
    pub size: i64,
}
```

**`_gen_save_path` logic** (from `path.py:62-81`):
```
Folder = "{official_title} ({year})" if year else official_title
Adjusted season = data.season + data.season_offset
# Python (path.py:73-74): 如果 adjusted_season < 1, 回退到原始 data.season, 非 clamp 到 1!
if adjusted_season < 1: adjusted_season = data.season
SavePath = {downloader_path} / {Folder} / "Season {adjusted_season}"
```

**`_rule_name` logic** (from `path.py:84-90`):
```
If group_tag enabled: "[{group_name}] {official_title} S{season}"
Else: "{official_title} S{season}"
```

### `src/client.rs` — `DownloaderClient` Trait + Factory

```rust
/// Common interface for all downloader backends.
#[async_trait]
pub trait DownloaderClient: Send + Sync {
    async fn auth(&mut self) -> Result<bool, DownloaderError>;
    async fn logout(&mut self) -> Result<(), DownloaderError>;
    async fn check_host(&self) -> Result<bool, DownloaderError>;

    // Torrent operations
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

    // RSS operations
    /// Python qb_downloader.py:220-227: 省略 item_path 时默认 "Mikan_RSS"
    async fn rss_add_feed(&self, url: &str, item_path: Option<&str>) -> Result<(), DownloaderError>;
    async fn rss_remove_item(&self, item_path: &str) -> Result<(), DownloaderError>;
    async fn rss_get_feeds(&self) -> Result<Value, DownloaderError>;
    async fn rss_set_rule(&self, rule_name: &str, rule_def: Value) -> Result<(), DownloaderError>;
    async fn get_download_rule(&self) -> Result<Value, DownloaderError>;
    async fn remove_rule(&self, rule_name: &str) -> Result<(), DownloaderError>;

    // Preferences
    async fn prefs_init(&self, prefs: HashMap<String, Value>) -> Result<(), DownloaderError>;
    async fn get_app_prefs(&self) -> Result<Value, DownloaderError>;
    async fn add_category(&self, category: &str) -> Result<(), DownloaderError>;
}

/// Convenience struct for torrent info (maps qBittorrent's JSON response).
#[derive(Debug, Clone, Deserialize)]
pub struct TorrentInfo {
    pub hash: String,
    pub name: String,
    pub save_path: String,
    pub category: String,
    pub state: String,
    pub progress: f64,
    pub tags: String,
    // ... other fields as needed
}
```

**Factory function** (replacing Python's `__getClient`):
```rust
pub fn create_client(config: &DownloaderConfig) -> Result<Box<dyn DownloaderClient>, DownloaderError> {
    match config.r#type.as_str() {
        "qbittorrent" => Ok(Box::new(QbDownloader::new(
            &config.host, &config.username, &config.password, config.ssl,
        )?)),
        "aria2" => Ok(Box::new(Aria2Downloader::new(
            &config.host, &config.password,
        )?)),
        "mock" => Ok(Box::new(MockDownloader::new())),
        other => Err(DownloaderError::UnsupportedType(other.to_string())),
    }
}
```

### `src/lib.rs` — `DownloadClient` (Orchestration) + Re-exports

Replaces Python's `DownloadClient(TorrentPath)`. Uses composition (owns a `TorrentPath` via the path module + a `Box<dyn DownloaderClient>`), not inheritance.

```rust
pub struct DownloadClient {
    client: Box<dyn DownloaderClient>,
    authed: bool,
}

impl DownloadClient {
    pub fn new(config: &DownloaderConfig) -> Result<Self, DownloaderError>;
    pub async fn auth(&mut self) -> Result<bool, DownloaderError>;
    pub async fn logout(&mut self) -> Result<(), DownloaderError>;
    pub async fn check_host(&self) -> Result<bool, DownloaderError>;

    // Lifecycle
    /// Python download_client.py:245-248: 创建 "BangumiCollection" 分类 + 设置 RSS 下载偏好
    /// 内部调用: client.add_category("BangumiCollection") + prefs_init(rss_download_prefs)
    pub async fn init_downloader(&self) -> Result<(), DownloaderError>;
    pub async fn set_rule(&self, data: &Bangumi) -> Result<(), DownloaderError>;
    pub async fn set_rules(&self, bangumi_info: &[Bangumi]) -> Result<(), DownloaderError>;

    // Torrent operations
    pub async fn get_torrent_info(
        &self, category: &str, status_filter: &str, tag: Option<&str>,
    ) -> Result<Vec<TorrentInfo>, DownloaderError>;
    pub async fn get_torrent_files(&self, hash: &str) -> Result<Vec<FileInfo>, DownloaderError>;
    pub async fn rename_torrent_file(
        &self, hash: &str, old_path: &str, new_path: &str, verify: bool,
    ) -> Result<bool, DownloaderError>;
    pub async fn delete_torrent(&self, hash: &str, delete_files: bool) -> Result<(), DownloaderError>;
    pub async fn pause_torrent(&self, hash: &str) -> Result<(), DownloaderError>;
    pub async fn resume_torrent(&self, hash: &str) -> Result<(), DownloaderError>;
    /// Python download_client.py:73-118: 接受 Torrent 或 Vec<Torrent> (collector 错误传入列表)
    /// 默认 tags: f"ab:{bangumi.id}" (Python download_client.py:83)
    pub async fn add_torrent(
        &self, torrent: &Torrent, bangumi: &Bangumi, network: &NetworkClient,
    ) -> Result<bool, DownloaderError>;
    pub async fn add_torrents(
        &self, torrents: &[Torrent], bangumi: &Bangumi, network: &NetworkClient,
    ) -> Result<bool, DownloaderError>;
    pub async fn move_torrent(&self, hash: &str, location: &str) -> Result<(), DownloaderError>;

    // RSS operations
    /// Python qb_downloader.py:220-227: 默认 item_path="Mikan_RSS"
    /// qBittorrent API: /api/v2/rss/addFeed?url={url}&path={path}
    pub async fn add_rss_feed(&self, rss_link: &str, item_path: Option<&str>) -> Result<(), DownloaderError>;
    pub async fn remove_rss_feed(&self, item_path: &str) -> Result<(), DownloaderError>;
    pub async fn get_rss_feed(&self) -> Result<Value, DownloaderError>;
    pub async fn get_download_rules(&self) -> Result<Value, DownloaderError>;
    pub async fn remove_rule(&self, rule_name: &str) -> Result<(), DownloaderError>;
    pub async fn get_torrents_by_tag(&self, tag: &str) -> Result<Vec<TorrentInfo>, DownloaderError>;
    pub async fn add_tag(&self, hash: &str, tag: &str) -> Result<(), DownloaderError>;
}
```

Key differences from Python:
- `add_torrent` takes `&NetworkClient` explicitly (Python uses `async with RequestContent()` internally)
- No async context manager (`__aenter__`/`__aexit__`) — caller manages `auth()`/`logout()` explicitly
- `set_rule` constructs rule JSON using `serde_json::json!` macro instead of dict literal

**`set_rule` logic** (from `download_client.py:95-118`):
```rust
pub async fn set_rule(&self, data: &Bangumi) -> Result<(), DownloaderError> {
    let rule_name = rule_name(data, settings.bangumi_manage.group_tag);
    let save_path = gen_save_path(data, &settings.downloader.path);
    let rule = json!({
        "enable": true,
        "mustContain": data.title_raw,
        "mustNotContain": data.filter.join("|"),
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
    self.client.rss_set_rule(&rule_name, rule).await?;
    Ok(())
}
```

### `src/qb.rs` — `QbDownloader`

Full qBittorrent Web API v2 client. Uses `reqwest::Client` directly (not `ab-network::NetworkClient`) because:
- Requires `danger_accept_invalid_certs(true)` for self-signed NAS certs
- Connects to a different host (qb server) than the application's HTTP targets

```rust
pub struct QbDownloader {
    host: String,
    username: String,
    password: String,
    client: reqwest::Client,
}

impl QbDownloader {
    pub fn new(host: &str, username: &str, password: &str, ssl: bool) -> Result<Self, DownloaderError>;
    fn url(&self, endpoint: &str) -> String;
}
```

Methods mirror Python `qb_downloader.py`:
- `auth()` — POST `/api/v2/auth/login` with `{username, password}` form data; retry up to 3 times
- `logout()` — POST `/api/v2/auth/logout`; ignore errors
- `check_host()` — GET `/api/v2/app/version` → status 200
- `prefs_init(prefs)` — POST `/api/v2/app/setPreferences` with JSON body
- `get_app_prefs()` — GET `/api/v2/app/preferences`
- `add_category(category)` — POST `/api/v2/torrents/createCategory`
- `torrents_info(filter, category, tag)` — GET `/api/v2/torrents/info`
- `torrents_files(hash)` — GET `/api/v2/torrents/files`
- `add_torrents(urls, files, save_path, category, tags)` — POST `/api/v2/torrents/add` (multipart); `contentLayout: "NoSubfolder"` (Python qb_downloader.py:144)
- `torrents_delete(hash, delete_files)` — POST `/api/v2/torrents/delete`
- `torrents_pause(hash)` / `torrents_resume(hash)` — POST `/api/v2/torrents/pause` / `resume`
- `torrents_rename_file(hash, old, new, verify)` — POST `/api/v2/torrents/renameFile` + verify loop
- `rss_add_feed(url, path)` — POST `/api/v2/rss/addFeed`; path 默认 `"Mikan_RSS"` (Python qb_downloader.py:220-227)
- `rss_remove_item(path)` — POST `/api/v2/rss/removeItem`
- `rss_get_feeds()` — GET `/api/v2/rss/items`
- `rss_set_rule(name, def)` — POST `/api/v2/rss/setRule`
- `rss_get_rules()` — GET `/api/v2/rss/rules`
- `remove_rule(name)` — POST `/api/v2/rss/removeRule`
- `move_torrent(hash, location)` — POST `/api/v2/torrents/setLocation`
- `get_torrent_path(hash)` — GET `/api/v2/torrents/info?hashes={hash}` → extract save_path
- `set_category(hash, category)` — POST `/api/v2/torrents/setCategory`
  - Python qb_downloader.py:251-255: 处理 `409 Conflict` → 先创建分类再重试
- `add_tag(hash, tag)` — POST `/api/v2/torrents/addTags`
- `get_torrents_by_tag(tag)` — GET `/api/v2/torrents/info?tag={tag}`
- `check_connection()` — GET `/api/v2/app/version` → text

All methods wrap `reqwest` calls in retry logic matching Python's `@qb_connect_failed_wait` decorator (retry on `reqwest::Error` up to 3 times, 2s delay).

Client construction (SSL handling matching Python `verify=False`):
```rust
pub fn new(host: &str, username: &str, password: &str, ssl: bool) -> Result<Self, DownloaderError> {
    let host = if !host.contains("://") {
        let scheme = if ssl { "https" } else { "http" };
        format!("{}://{}", scheme, host)
    } else {
        host.to_string()
    };
    let client = Client::builder()
        .danger_accept_invalid_certs(true)  // Python: verify=False
        .timeout(Duration::from_secs(30))
        .build()?;
    Ok(Self { host, username: username.to_string(), password: password.to_string(), client })
}
```

### `src/aria2.rs` — `Aria2Downloader`

JSON-RPC client for Aria2.

```rust
pub struct Aria2Downloader {
    rpc_url: String,
    secret: String,
    client: reqwest::Client,
    id: u64,
}
```

Methods:
- `auth()` — calls `getVersion` RPC method; retry 3x with 5s delay
- `logout()` — close client
- `add_torrents(urls, files, save_path, category, tags)` — `aria2.addUri` / `aria2.addTorrent` RPC
- Most other methods → return `Err(DownloaderError::NotSupported("..."))`

**RPC call helper**:
```rust
async fn call(&mut self, method: &str, params: Vec<Value>) -> Result<Value, DownloaderError> {
    self.id += 1;
    let payload = json!({
        "jsonrpc": "2.0",
        "id": self.id,
        "method": format!("aria2.{}", method),
        "params": [format!("token:{}", self.secret), params],
    });
    let resp = self.client.post(&self.rpc_url).json(&payload).send().await?;
    // Handle error response
}
```

### `src/mock.rs` — `MockDownloader`

In-memory mock with the same interface as QbDownloader. Mirrors `mock_downloader.py`.

```rust
pub struct MockDownloader {
    torrents: HashMap<String, Value>,
    rules: HashMap<String, Value>,
    feeds: HashMap<String, Value>,
    categories: HashSet<String>,
    prefs: HashMap<String, Value>,
}
```

All methods return success values matching Python:
- `auth()` → `true`
- `add_torrents()` → generate mock SHA-1 hash, store in-memory
- `torrents_info()` → filter in-memory torrents
- `torrents_rename_file()` → log + return `true`

**Test helper** (matching Python's `add_mock_torrent()`):
```rust
impl MockDownloader {
    pub fn add_mock_torrent(
        &mut self, name: &str, hash: Option<&str>, category: &str, state: &str,
        save_path: &str, files: Option<Vec<FileInfo>>,
    ) -> String;
    pub fn get_state(&self) -> Value;
}
```

### `src/lib.rs` — Re-exports
```rust
pub mod client;
pub mod error;
pub mod mock;
pub mod path;
pub mod qb;
pub mod aria2;

pub use client::{DownloadClient, DownloaderClient, TorrentInfo, create_client, init_downloader};
pub use error::DownloaderError;
pub use path::{
    check_files, path_to_bangumi, file_depth, is_ep,
    gen_save_path, rule_name, join_path, FileInfo,
};
```

## Cargo.toml Dependencies

```toml
[package]
name = "ab-downloader"
version.workspace = true
edition.workspace = true

[dependencies]
ab-core = { path = "../ab-core" }
ab-database = { path = "../ab-database" }
ab-network = { path = "../ab-network" }
reqwest = { workspace = true, features = ["json"] }
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
tokio = { workspace = true, features = ["time"] }  # for sleep in retry
tracing.workspace = true
thiserror.workspace = true
async-trait.workspace = true
```

## File-by-File Build Order (7 files)

| # | File | Lines | Description |
|---|------|-------|-------------|
| 1 | `ab-downloader/Cargo.toml` | 20 | Workspace member |
| 2 | `ab-downloader/src/error.rs` | 30 | DownloaderError enum |
| 3 | `ab-downloader/src/path.rs` | 80 | TorrentPath (path generation, file classification) |
| 4 | `ab-downloader/src/client.rs` | 50 | DownloaderClient trait + TorrentInfo struct |
| 5 | `ab-downloader/src/qb.rs` | 300 | QbDownloader (qBittorrent Web API) |
| 6 | `ab-downloader/src/aria2.rs` | 120 | Aria2Downloader (JSON-RPC) |
| 7 | `ab-downloader/src/mock.rs` | 200 | MockDownloader (in-memory) |
| 8 | `ab-downloader/src/lib.rs` | 55 | DownloadClient orchestration + re-exports (含 init_downloader) |

## Key Design Decisions

1. **Trait-based polymorphism** — `DownloaderClient` trait replaces Python's duck-typed downloader clients. Provides compile-time interface enforcement.

2. **No async context manager** — Python's `async with DownloadClient() as client:` is replaced by explicit `client.auth().await` / `client.logout().await`. This avoids complex lifetime issues and gives callers precise control.

3. **SSL handling** — `QbDownloader` always sets `danger_accept_invalid_certs(true)`, matching Python's `verify=False` which assumes self-signed NAS certificates.

4. **`add_torrent` signature** — Accepts `&NetworkClient` from `ab-network` for fetching torrent file bytes. Python's `RequestContent` is instantiated inline; Rust requires passing the client.

5. **Retry strategy** — All qBittorrent API calls include retry logic (3 attempts, 2s delay) matching Python's `@qb_connect_failed_wait` decorator. Aria2 auth retries 3x with 5s delay.

6. **`init_downloader` lifecycle** — Python `DownloadClient.__init__` calls `init_downloader` internally to set up category + RSS prefs. Rust separates construction from initialization: caller must call `init_downloader()` explicitly after `auth()`.

7. **`tr_downloader.py` omitted** — Empty file in Python; no Transmission implementation exists.

## Test Plan

**Unit tests** (`src/path.rs`):
- `check_files` classifies .mkv/.mp4 as media, .ass/.srt as subtitle, others ignored
- `check_files` handles mixed lists
- `gen_save_path` with year produces correct path
- `gen_save_path` without year omits year
- `gen_save_path` applies season_offset correctly
- `gen_save_path` when adjusted_season < 1, reverts to data.season (Python path.py:73-74)
- `rule_name` with group_tag produces "[Group] Title S1"
- `rule_name` without group_tag produces "Title S1"
- `path_to_bangumi` extracts name and season
- `is_ep` for various depths

**Unit tests** (`src/mock.rs`):
- `auth` returns Ok(true)
- `add_torrents` with URL returns Ok(true) and stores torrent
- `add_torrents` with file bytes returns Ok(true)
- `torrents_info` filters by category
- `torrents_info` filters by tag
- `torrents_delete` removes torrent
- `torrents_rename_file` returns Ok(true)
- `rss_set_rule` stores rule
- Helper `add_mock_torrent` populates state correctly

**Integration tests** (using `wiremock` for qBittorrent/aria2 API responses):
- `QbDownloader::auth` succeeds with valid credentials
- `QbDownloader::auth` fails with 403
- `QbDownloader::add_torrents` sends correct multipart form
- `QbDownloader::torrents_rename_file` verifies rename
- `Aria2Downloader::auth` succeeds with valid token
- `Aria2Downloader::add_torrents` sends correct JSON-RPC payload

## Acceptance Criteria

1. `cargo build -p ab-downloader` compiles without errors
2. `cargo test -p ab-downloader` passes all tests
3. `DownloadClient::new()` creates the correct client type based on `downloader.type` config
4. `init_downloader()` creates "BangumiCollection" category + sets RSS download prefs (Python download_client.py:245-248)
5. `QbDownloader::auth()` matches Python's login flow (retry, 403 handling, verify=False)
6. `QbDownloader::add_torrents()` sends multipart form with URLs and/or file bytes
7. `QbDownloader::torrents_rename_file()` includes rename verification loop (3 attempts, exponential backoff)
8. `Aria2Downloader::add_torrents()` sends valid JSON-RPC payloads (`addUri` / `addTorrent`)
9. Unsupported methods on Aria2 return `Err(DownloaderError::NotSupported)`
10. `MockDownloader` stores all operations in-memory and supports `add_mock_torrent()` test helper
11. `path::gen_save_path()` produces correct paths matching Python's `_gen_save_path()`
12. `add_torrent()` correctly fetches content via `NetworkClient` for non-magnet URLs
13. `tr_downloader.py` is explicitly omitted (no-op file not migrated)
