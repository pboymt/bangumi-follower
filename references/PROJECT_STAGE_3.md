# Stage 3: ab-network — HTTP Client + RSS Parsing

## Objective
Migrate 5 Python files (3 meaningful: `request_url.py`, `request_contents.py`, `site/mikan.py`; plus 2 trivial `__init__.py` re-exports) to a single reqwest-based crate with site-pluggable RSS parsing.

## Dependencies
- **ab-core** (Stage 1) — `ProxyConfig` from settings
- **External**: `reqwest` (rustls-tls, socks), `quick-xml` (or `serde_xml_rs`), `serde` + `serde_json`, `tracing`, `thiserror`

## Python Sources → Crate Layout

```
ab-network/
├── Cargo.toml
├── src/
│   ├── lib.rs                                    # Re-exports
│   ├── error.rs                                  # NetworkError enum
│   ├── client.rs                                 # SharedClient + NetworkClient
│   └── site/
│       ├── mod.rs                                # Re-exports
│       └── mikan.rs                              # RSS parser (Mikanani)
```

### Source Mapping

| Python file | Rust target | Lines |
|-------------|-------------|-------|
| `request_url.py` | `client.rs` (shared client + proxy + get/post/head) | ~120 |
| `request_contents.py` | `client.rs` (content-type convenience methods) + `ab-network` is called from higher crates | ~40 |
| `site/mikan.py` | `site/mikan.rs` | ~30 |
| (new) | `error.rs` | ~30 |
| (new) | `lib.rs` | ~10 |
| (new) | `site/mod.rs` | ~5 |

**Total**: ~235 lines Rust, 5 files.

## Crate Design

### `src/error.rs` — `NetworkError`
```rust
#[derive(thiserror::Error, Debug)]
pub enum NetworkError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("XML parse error: {0}")]
    XmlParse(String),
    #[error("request failed after {0} retries: {1}")]
    RetryExhausted(u32, String),
    #[error("connection check failed")]
    ConnectionFailed,
}
```

### `src/client.rs` — `SharedClient` + `NetworkClient`

Replace Python's module-level `_shared_client` (managed by `get_shared_client()`) with a lazy-initialized singleton pattern. reqwest `Client` internally manages connection pooling.

```rust
use once_cell::sync::OnceCell;
use reqwest::{Client, Proxy, Response};
use std::sync::Arc;
use ab_core::config::net_config::ProxyConfig;

static SHARED_CLIENT: OnceCell<Arc<Client>> = OnceCell::new();

/// Re-initializes the shared client on proxy config change.
/// Call this when proxy settings are updated at runtime.
pub fn reset_shared_client(proxy: Option<&ProxyConfig>) {
    let client = build_client(proxy);
    let _ = SHARED_CLIENT.set(Arc::new(client));
    // If already set, we'd need a different approach (RwLock<Option<Arc<Client>>>)
}

fn build_client(proxy: Option<&ProxyConfig>) -> Client {
    let mut builder = Client::builder()
        .user_agent(DEFAULT_UA)
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(4)
        .connect_timeout(Duration::from_secs(10))   // Python: connect=10s
        .pool_idle_timeout(Duration::from_secs(10)); // Python: pool=10s
    if let Some(p) = proxy {
        if p.enable {
            // Python Proxy 模型支持 $VAR/${VAR} 环境变量展开 (models/config.py:84-91)
            let user = expand_env(&p.username);
            let pass = expand_env(&p.password);
            let proxy_url = format!("{}://{}:{}", p.r#type, p.host, p.port);
            let mut proxy = Proxy::all(&proxy_url).expect("valid proxy URL");
            if !user.is_empty() && !pass.is_empty() {
                proxy = proxy.basic_auth(user, pass);
            }
            builder = builder.proxy(proxy);
        }
    } else {
        // Python (request_url.py:44-45): 当 proxy 未配置或 type 未知时, 创建无 proxy 客户端
        // 不做任何操作, builder 不带 proxy 即为直连
    }
    builder.build().expect("reqwest client build failed")
}
```

**`get_shared_client()`** (replacing Python's async `get_shared_client()`):
```rust
pub fn get_shared_client() -> Arc<Client> {
    SHARED_CLIENT.get().cloned().unwrap_or_else(|| {
        let client = Arc::new(build_client(None));
        let _ = SHARED_CLIENT.set(client.clone());
        client
    })
}
```

**`NetworkClient`** (replacing `RequestURL` + `RequestContent` inheritance):
```rust
pub struct NetworkClient {
    client: Arc<Client>,
}

impl NetworkClient {
    pub fn new(proxy: Option<&ProxyConfig>) -> Self;
    pub fn from_client(client: Arc<Client>) -> Self;

    // Low-level — maps to RequestURL
    pub async fn get_url(&self, url: &str, retry: u32) -> Result<Response, NetworkError>;
    /// Python 的 post_json (request_contents.py:56-58) 实际调用 post_url(data=dict) 发送 form-encoded!
    /// 方法名有误导性 —— 所有通知 provider 调用 `post_json` 期望的是 form-encoded 数据。
    /// 此 Rust 方法提供真正的 JSON 发送 (reqwest .json())，而 `post_form` 用于通知 provider。
    pub async fn post_json(&self, url: &str, data: &Value, retry: u32) -> Result<Response, NetworkError>;   // Content-Type: application/json (真正的 JSON)
    pub async fn post_form(&self, url: &str, data: &HashMap<String, String>, retry: u32) -> Result<Response, NetworkError>;  // Content-Type: application/x-www-form-urlencoded (Python post_json 的实际行为)
    pub async fn post_multipart(&self, url: &str, data: &HashMap<String, String>, files: ...) -> Result<Response, NetworkError>;
    pub async fn check_url(&self, url: &str) -> Result<bool, NetworkError>;

    // High-level — convenience methods (from RequestContent)
    pub async fn get_xml(&self, url: &str, retry: u32) -> Result<String, NetworkError>;  // returns raw XML text
    pub async fn get_json(&self, url: &str) -> Result<Value, NetworkError>;
    pub async fn get_html(&self, url: &str) -> Result<String, NetworkError>;
    pub async fn get_content(&self, url: &str) -> Result<Vec<u8>, NetworkError>;

    // RSS-specific
    /// 获取 RSS 种子列表。
    /// filter: 正则拒绝列表 —— Python (request_contents.py:29) 用 `re.search(filter, name) is None` 筛选，
    /// 即匹配正则的项目被排除 (deny-list), 不匹配的保留。
    /// 默认值: settings.rss_parser.filter, Python 默认 ["720", "\\d+-\\d+"] (models/config.py:54)
    /// limit: 返回数量上限
    pub async fn get_torrents(
        &self,
        url: &str,
        filter: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<RssEntry>, NetworkError>;
    pub async fn get_rss_title(&self, url: &str) -> Result<String, NetworkError>;
}
```

**`RssEntry`** — parsed RSS item (returned to callers):
```rust
#[derive(Debug, Clone)]
pub struct RssEntry {
    pub title: String,
    pub url: String,
    pub homepage: String,
}
```

**`expand_env`** — 展开 `$VAR` / `${VAR}` 环境变量引用 (Python Proxy 模型 `_expand()`):
```rust
/// 展开字符串中的 $VAR 和 ${VAR} 环境变量引用
fn expand_env(value: &str) -> String {
    // 使用 regex 或逐字符扫描查找 $... 并替换
    // 匹配 Python os.path.expandvars() 行为
}
```

**Header selection** (from `_get_headers`):
- Per-request headers (built for every call):
  - `User-Agent`: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ...
  - `Accept-Language`: `en-US,en;q=0.9`
  - `Accept-Encoding`: `gzip, deflate`
  - `Connection`: `keep-alive`
- `Accept` header varies by URL:
  - Torrent URLs (`.torrent` or `/download/` in path): `application/x-bittorrent, application/octet-stream, */*`
  - Otherwise: `application/xml, text/xml, */*`
- `Content-Type` is set by the specific method:
  - `post_json`: `application/json`
  - `post_form`: `application/x-www-form-urlencoded`
  - `post_multipart`: `multipart/form-data`
- All headers set on the `RequestBuilder` per-call, not on the shared client.

**Timeout granularity** (matching Python `request_url.py:29`):
- 总超时: 30s (reqwest `Client::builder().timeout()`)
- 连接超时: 10s (`.connect_timeout()`) — Python: `connect=10s`
- 读超时: 30s (由总超时覆盖) — Python: `read=30s`
- 写超时: 10s (Python: `write=10s` — reqwest 无独立写超时, 由总超时覆盖)
- 连接池空闲超时: 10s (`.pool_idle_timeout()`) — Python: `pool=10s`

**Retry logic** (from `get_url` / `post_json` / `post_form`):
- On `reqwest::Error` (connectivity): retry up to `retry` times with 5s sleep
- On HTTP status error (4xx/5xx): return error immediately (don't retry by default — Python breaks on `HTTPStatusError`)
- Use `tracing::warn!` for retry attempts, `tracing::error!` for exhaustion

### `src/site/mikan.rs` — RSS Parser

```rust
use quick_xml::Reader;
use quick_xml::events::Event;

pub fn parse_rss(xml: &str) -> Result<Vec<RssEntry>, NetworkError> {
    let mut reader = Reader::from_str(xml);
    // Navigate: /rss/channel/item
    // For each item: extract <title>, <enclosure url="...">, <link>
    // ...
}
```

Alternative: if XML structure is well-known, use `serde_xml_rs` for deserialization to a struct. Must handle both enclosure-based and direct-link items (Python `mikan.py:13-17` fallback when `<enclosure>` is absent):

```rust
#[derive(Debug, Deserialize)]
struct Rss {
    channel: Channel,
}

#[derive(Debug, Deserialize)]
struct Channel {
    title: String,
    item: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Item {
    title: String,
    link: Option<String>,          // <link> — used as url when enclosure is absent
    enclosure: Option<Enclosure>,  // <enclosure url="..."> — preferred source
}

#[derive(Debug, Deserialize)]
struct Enclosure {
    #[serde(rename = "url")]
    url: String,
}
```

Mapping to `RssEntry`:
- `title` → `item.title`
- `url` → `item.enclosure.url` if present, else `item.link` (Python fallback at `mikan.py:14-17`)
- `homepage` → `item.link` if enclosure present, else `""`

**Prefer `quick-xml`** for parsing (lighter, no proc macros) — but `serde_xml_rs` is acceptable for simpler code.

### `src/lib.rs` — Re-exports
```rust
pub mod client;
pub mod error;
pub mod site;

pub use client::{NetworkClient, RssEntry, get_shared_client, reset_shared_client};
pub use error::NetworkError;
pub use site::mikan::parse_rss;
```

### `Cargo.toml` Dependencies
```toml
[package]
name = "ab-network"
version.workspace = true
edition.workspace = true

[dependencies]
ab-core = { path = "../ab-core" }
reqwest = { workspace = true, features = ["json", "socks"] }
quick-xml.workspace = true
serde.workspace = true
serde_json.workspace = true
once_cell.workspace = true
tokio = { workspace = true, features = ["time"] }  # for sleep in retry
tracing.workspace = true
thiserror.workspace = true
```

`[dev-dependencies]` for integration tests: `wiremock` for mocking HTTP endpoints.

## Key Design Decisions

1. **No shared mutable state** — Python's `_shared_client` module-level global with proxy reload is replaced by `OnceCell<Arc<Client>>`. Proxy config changes reset the client. If runtime proxy updates are needed, use `RwLock<Option<Arc<Client>>>` instead.

2. **No class inheritance** — Python's `RequestContent(RequestURL)` is flattened into a single `NetworkClient` struct with both low-level and high-level methods.

3. **Sync vs async** — All network methods are async (reqwest is async-native), matching Python's `httpx.AsyncClient`.

4. **Retry strategy** — Only retry on transient connectivity errors (reqwest `Error::Request` with `is_connect()` or `is_timeout()`). HTTP 4xx/5xx errors are returned immediately without retry. Python's `get_url` also returns `None` on `HTTPStatusError` (no retry).

5. **XML parsing** — Use `quick-xml` (streaming, zero-copy) with serde deserialization via `serde_xml_rs` for simplicity. Prefer `quick-xml` for tighter dependency control.

6. **No async context manager** — Python's `async with RequestURL() as client:` pattern becomes direct construction:
   ```rust
   let client = NetworkClient::new(Some(&proxy_config));
   let entries = client.get_torrents("https://...", None, None).await?;
   ```

7. **`RssEntry`** is a plain struct — not the `Torrent` DB model. The `get_torrents` method returns `Vec<RssEntry>`; the caller (e.g., `ab-rss` or `ab-manager`) converts to the DB model. This keeps `ab-network` free of database dependencies.

## File-by-File Build Order (5 files)

| # | File | Lines | Description |
|---|------|-------|-------------|
| 1 | `ab-network/Cargo.toml` | 18 | Workspace member, deps |
| 2 | `ab-network/src/error.rs` | 30 | NetworkError enum |
| 3 | `ab-network/src/site/mod.rs` | 3 | pub mod mikan |
| 4 | `ab-network/src/site/mikan.rs` | 50 | parse_rss() XML parser |
| 5 | `ab-network/src/client.rs` | 150 | NetworkClient + shared client + get_torrents |
| 6 | `ab-network/src/lib.rs` | 10 | Re-exports |

## Test Plan

**Unit tests** (`src/client.rs`):
- `build_client` with no proxy → basic client
- `build_client` with http proxy → proxy configured
- `build_client` with socks5 proxy → socks proxy configured

**Integration tests** (using `wiremock`):
- `get_url` succeeds after 1 retry → returns response
- `get_url` retries on 503 → retries up to 3 times
- `get_url` fails with 404 → returns error immediately (no retry)
- `get_torrents` parses valid RSS XML → returns parsed entries
- `get_torrents` with filter → filters matching results
- `get_torrents` with limit → returns at most N entries
- `get_xml` on non-XML response → returns XmlParse error

**Test RSS XML** (embedded in test): a minimal valid RSS 2.0 feed with 3 items.

## Acceptance Criteria

1. `cargo build -p ab-network` compiles without errors
2. `cargo test -p ab-network` passes all tests
3. `NetworkClient::new()` builds a reqwest Client respecting proxy config
4. `get_torrents()` correctly fetches, parses, filters, and limits RSS feed items
5. `get_url()` retries on transient errors, returns on HTTP errors
6. `check_url()` returns `true`/`false` based on HEAD request
7. RSS parser handles both enclosure-based and direct-link formats
8. No database or config dependencies beyond `ab-core` types
9. `RssEntry` retains full original data (title, url, homepage) for downstream use
