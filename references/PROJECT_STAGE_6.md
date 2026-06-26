# Stage 6: ab-rss — RSS Engine + Feed Analysis

## Objective
Migrate 2 Python files (`rss/engine.py`, `rss/analyser.py`) + `__init__.py` to a single async crate. `RSSEngine` handles RSS feed polling, torrent matching, and download dispatch. `RSSAnalyser` handles RSS torrent → Bangumi conversion with title enrichment (Mikan/TMDB). No class inheritance — use composition with injected repo and network dependencies.

## Dependencies
- **ab-core** (Stage 1) — `Config` (language, filter, TMDB key subset via `ParserConfig`)
- **ab-database** (Stage 2) — `RssRepo`, `TorrentRepo`, `BangumiRepo` + models (`Bangumi`, `RSSItem`, `Torrent`, `RSSUpdate`)
- **ab-network** (Stage 3) — `NetworkClient` (get_torrents, get_rss_title)
- **ab-parser** (Stage 4) — `raw_parser`, `Episode`, `mikan_parse`, `tmdb_search`, `TMDBInfo`, `ParserConfig`, `ImageSaver`
- **External**: `regex`, `serde`, `tracing`, `thiserror`, `async-trait`

## Python Sources → Crate Layout

```
ab-rss/
├── Cargo.toml
├── src/
│   ├── lib.rs                      # Re-exports
│   ├── error.rs                    # RssError enum
│   ├── analyser.rs                 # RSSAnalyser (RSS → Bangumi conversion + enrichment)
│   └── engine.rs                   # RSSEngine (polling, matching, dispatch)
```

### Source Mapping

| Python file | Rust target | Lines |
|-------------|-------------|-------|
| `rss/engine.py` | `engine.rs` | ~200 |
| `rss/analyser.py` | `analyser.rs` | ~100 |
| `rss/__init__.py` | `lib.rs` | ~10 |
| (new) | `error.rs` | ~20 |
| (new) | `analyser.rs` (filter cache) | ~30 |

**Total**: ~360 lines Rust, 4 files.

## Crate Design

### `src/error.rs` — `RssError`
```rust
#[derive(thiserror::Error, Debug)]
pub enum RssError {
    #[error("database error: {0}")]
    Database(#[from] ab_database::DbError),
    #[error("network error: {0}")]
    Network(#[from] ab_network::NetworkError),
    #[error("parser error: {0}")]
    Parser(#[from] ab_parser::ParserError),
    #[error("add torrent failed: {0}")]
    AddTorrentFailed(String),
    #[error("{0}")]
    Other(String),
}
```

### `src/engine.rs` — `RSSEngine`

Replaces Python's `RSSEngine(Database)`. Uses composition instead of inheritance — holds three repo references and a filter cache.

```rust
pub struct RSSEngine {
    rss_repo: RssRepo,
    torrent_repo: TorrentRepo,
    bangumi_repo: BangumiRepo,
    filter_cache: RefCell<HashMap<String, Regex>>,
}

impl RSSEngine {
    pub fn new(pool: Pool<Sqlite>) -> Self;

    // RSS management
    pub async fn add_rss(
        &self,
        network: &NetworkClient,
        rss_link: &str,
        name: Option<&str>,
        aggregate: bool,
        parser: &str,
    ) -> Result<RSSItem, RssError>;
    pub fn disable_list(&self, ids: &[i32]) -> Result<(), RssError>;
    pub fn enable_list(&self, ids: &[i32]) -> Result<(), RssError>;
    pub fn delete_list(&self, ids: &[i32]) -> Result<(), RssError>;

    // Torrent fetching + matching
    pub async fn pull_rss(
        &self,
        rss_item: &RSSItem,
        network: &NetworkClient,
    ) -> Result<Vec<Torrent>, RssError>;
    pub fn match_torrent(&self, torrent: &Torrent) -> Result<Option<Bangumi>, RssError>;
    pub fn get_rss_torrents(&self, rss_id: i32) -> Result<Vec<Torrent>, RssError>;

    // Main orchestration
    pub async fn refresh_rss(
        &self,
        rss_id: Option<i32>,
        network: &NetworkClient,
        downloader: &dyn AddTorrent,
    ) -> Result<(), RssError>;

    // Single-bangumi download
    pub async fn download_bangumi(
        &self,
        bangumi: &Bangumi,
        network: &NetworkClient,
        downloader: &dyn AddTorrent,
    ) -> Result<bool, RssError>;
}
```

**`filter_cache`** — Thread-local via `RefCell` (single-threaded access, like Python's dict):
```rust
fn get_filter_pattern(&self, filter_str: &str) -> Result<Regex, RssError> {
    let mut cache = self.filter_cache.borrow_mut();
    if let Some(pattern) = cache.get(filter_str) {
        return Ok(pattern.clone());
    }
    let raw_pattern = filter_str.replace(',', "|");
    let pattern = match Regex::new(&raw_pattern) {
        Ok(re) => re,
        Err(_) => {
            // Fall back to escaped literal matching
            let terms: Vec<&str> = filter_str.split(',').collect();
            let escaped = terms.iter().map(|t| regex::escape(t)).collect::<Vec<_>>().join("|");
            Regex::new(&escaped)?
        }
    };
    cache.insert(filter_str.to_string(), pattern.clone());
    Ok(pattern)
}
```

**`add_rss`** — Mirrors Python's `RSSEngine.add_rss()`:
```rust
pub async fn add_rss(
    &self,
    network: &NetworkClient,
    rss_link: &str,
    name: Option<&str>,
    aggregate: bool,
    parser: &str,
) -> Result<RSSItem, RssError> {
    let name = match name {
        Some(n) => n.to_string(),
        None => network.get_rss_title(rss_link).await?,  // fallback to fetch via RSS
    };
    let item = RSSItem::new(name, rss_link, aggregate, parser);
    match self.rss_repo.add(&item).await? {
        true => Ok(item),
        false => Err(RssError::Other("RSS already exists".to_string())),
    }
}
```

**`pull_rss`** — Fetch torrents from RSS URL and filter out already-known:
```rust
pub async fn pull_rss(
    &self,
    rss_item: &RSSItem,
    network: &NetworkClient,
) -> Result<Vec<Torrent>, RssError> {
    let mut torrents = network.get_torrents(&rss_item.url, None, None).await?;
    for t in &mut torrents {
        t.rss_id = Some(rss_item.id);
    }
    let new_torrents = self.torrent_repo.check_new(&torrents).await?;
    Ok(new_torrents)
}
```

**`match_torrent`** — Match torrent to bangumi, applying filter (Python `match_torrent`):
```rust
pub fn match_torrent(&self, torrent: &Torrent) -> Result<Option<Bangumi>, RssError> {
    let matched = self.bangumi_repo.match_torrent(&torrent.name).await?;
    match matched {
        Some(mut bangumi) => {
            if bangumi.filter.is_empty() {
                return Ok(Some(bangumi));
            }
            let pattern = self.get_filter_pattern(&bangumi.filter)?;
            if !pattern.is_match(&torrent.name) {
                // Filter doesn't match → torrent passes filter
                return Ok(Some(bangumi));
            }
            // Filter matches → torrent is excluded
            Ok(None)
        }
        None => Ok(None),
    }
}
```

**`refresh_rss`** — The main polling + dispatch loop (Python `refresh_rss`). Accepts a `&dyn AddTorrent` trait so ab-rss doesn't depend on ab-downloader.

Uses `tokio::join!` or `futures::future::join_all` for concurrent RSS fetches, matching Python's `asyncio.gather`.

```rust
pub async fn refresh_rss(
    &self,
    rss_id: Option<i32>,
    network: &NetworkClient,
    downloader: &dyn AddTorrent,
) -> Result<(), RssError> {
    // 1. Get RSS items
    let rss_items = match rss_id {
        Some(id) => vec![self.rss_repo.search_id(id)
            .await?
            .ok_or_else(|| RssError::Other("RSS not found".to_string()))?],
        None => self.rss_repo.search_active().await?,
    };

    // 2. Concurrently fetch torrents
    let results: Vec<(RSSItem, Result<Vec<Torrent>, String>)> = futures::future::join_all(
        rss_items.into_iter().map(|item| {
            let engine = &self;
            async move {
                let result = engine.pull_rss(&item, network).await;
                (item, result.map_err(|e| e.to_string()))
            }
        })
    ).await;

    let now = Utc::now().to_rfc3339();
    for (mut rss_item, fetch_result) in results {
        let (new_torrents, error) = match fetch_result {
            Ok(t) => (t, None),
            Err(e) => (vec![], Some(e)),
        };

        // Update RSS item connection status
        rss_item.connection_status = error.as_deref().map(|_| "error").or(Some("healthy"));
        rss_item.last_checked_at = Some(now.clone());
        rss_item.last_error = error.clone();
        self.rss_repo.update(rss_item.id, &RSSUpdate::from(&rss_item)).await?;

        // Process new torrents
        for mut torrent in new_torrents {
            if let Some(bangumi) = self.match_torrent(&torrent)? {
                match downloader.add_torrent(&torrent, &bangumi, network).await {
                    Ok(true) => {
                        tracing::debug!("[Engine] Added torrent {} to client", torrent.name);
                        torrent.downloaded = true;
                    }
                    _ => {}  // add failed, still save to history
                }
            }
            self.torrent_repo.add(&torrent).await?;
        }
    }
    Ok(())
}
```

Note: Python calls `self.torrent.add_all(new_torrents)` outside the per-rss loop. In Rust we save each torrent inside the loop after processing. Both achieve the same result (commit after all saves).

**`download_bangumi`** — Mirrors Python's `download_bangumi`, combining the network fetch + downloader add into one call:
```rust
pub async fn download_bangumi(
    &self,
    bangumi: &Bangumi,
    network: &NetworkClient,
    downloader: &dyn AddTorrent,
) -> Result<bool, RssError> {
    let filter = bangumi.filter.replace(',', "|");
    let torrents = network.get_torrents(&bangumi.rss_link, Some(&filter), None).await?;
    if torrents.is_empty() {
        return Ok(false);
    }
    let mut success = false;
    for torrent in &torrents {
        if downloader.add_torrent(torrent, bangumi, network).await? {
            success = true;
        }
    }
    self.torrent_repo.add_all(&torrents).await?;
    Ok(success)
}
```

**`AddTorrent` trait** — Minimal interface for the download operation, defined in `engine.rs`:
```rust
#[async_trait]
pub trait AddTorrent: Send + Sync {
    async fn add_torrent(
        &self,
        torrent: &Torrent,
        bangumi: &Bangumi,
        network: &NetworkClient,
    ) -> Result<bool, RssError>;
}
```

The caller (ab-manager or core-thread) provides an impl that wraps `ab_downloader::DownloadClient::add_torrent`.

### `src/analyser.rs` — `RSSAnalyser`

Replaces Python's `RSSAnalyser(TitleParser)`. Stateless struct (all state passed as parameters). Mirrors the inheritance-based design by accepting parser functions as dependencies.

```rust
pub struct RSSAnalyser;

impl RSSAnalyser {
    /// Enrich a Bangumi with official title + poster from Mikan or TMDB.
    pub async fn official_title_parser(
        &self,
        bangumi: &mut Bangumi,
        rss: &RSSItem,
        torrent: &Torrent,
        network: &NetworkClient,
        config: &ParserConfig,
        image_saver: &dyn ImageSaver,
    ) -> Result<(), RssError>;

    /// Fetch RSS torrents from a link.
    pub async fn get_rss_torrents(
        &self,
        rss_link: &str,
        full_parse: bool,
        network: &NetworkClient,
    ) -> Result<Vec<Torrent>, RssError>;

    /// Convert a list of Torrents → new Bangumi entries (deduplicated by title_raw).
    pub async fn torrents_to_data(
        &self,
        torrents: &[Torrent],
        rss: &RSSItem,
        full_parse: bool,
        network: &NetworkClient,
        config: &ParserConfig,
        image_saver: &dyn ImageSaver,
    ) -> Result<Vec<Bangumi>, RssError>;

    /// Convert a single torrent → Bangumi entry.
    pub async fn torrent_to_data(
        &self,
        torrent: &Torrent,
        rss: &RSSItem,
        network: &NetworkClient,
        config: &ParserConfig,
        image_saver: &dyn ImageSaver,
    ) -> Result<Option<Bangumi>, RssError>;

    /// Full RSS pipeline: fetch → match → parse → save.
    /// Calls `engine.bangumi_repo.match_list()` and `engine.bangumi_repo.add_all()`.
    pub async fn rss_to_data(
        &self,
        rss: &RSSItem,
        engine: &RSSEngine,
        full_parse: bool,
        network: &NetworkClient,
        config: &ParserConfig,
        image_saver: &dyn ImageSaver,
    ) -> Result<Vec<Bangumi>, RssError>;

    /// Quick parse: fetch first torrent from a link and convert to Bangumi.
    pub async fn link_to_data(
        &self,
        rss: &RSSItem,
        network: &NetworkClient,
        config: &ParserConfig,
        image_saver: &dyn ImageSaver,
    ) -> Result<Option<Bangumi>, RssError>;
}
```

**`official_title_parser`** — Private method (not trait method):
```rust
async fn official_title_parser(
    bangumi: &mut Bangumi,
    rss: &RSSItem,
    torrent: &Torrent,
    network: &NetworkClient,
    config: &ParserConfig,
    image_saver: &dyn ImageSaver,
) -> Result<(), RssError> {
    match rss.parser.as_str() {
        "mikan" => {
            if let Some(homepage) = &torrent.homepage {
                let (poster, official_title) = ab_parser::enricher::mikan::mikan_parse(
                    network, homepage, image_saver
                ).await?;
                bangumi.poster_link = Some(poster);
                bangumi.official_title = official_title;
            }
        }
        "tmdb" => {
            let tmdb_info = ab_parser::enricher::tmdb::tmdb_search(
                network,
                &bangumi.official_title,
                &config.language,
            ).await?;
            if let Some(info) = tmdb_info {
                bangumi.official_title = info.title.clone();
                bangumi.year = Some(info.year.clone());
                bangumi.season = info.last_season;
                bangumi.poster_link = info.poster_link.clone();
            }
        }
        _ => {}  // "bgm" or others — no enrichment
    }
    // Strip invalid filename chars (Python: re.sub(r"[/:.\\]", " ", title))
    bangumi.official_title = bangumi.official_title
        .chars()
        .map(|c| if "/:.\\".contains(c) { ' ' } else { c })
        .collect();
}
```

**`torrents_to_data`** — Parse torrent names into Bangumi, deduplicate by `title_raw`:
```rust
pub async fn torrents_to_data(
    &self,
    torrents: &[Torrent],
    rss: &RSSItem,
    full_parse: bool,
    network: &NetworkClient,
    config: &ParserConfig,
    image_saver: &dyn ImageSaver,
) -> Result<Vec<Bangumi>, RssError> {
    let mut new_data = Vec::new();
    let mut seen_titles = HashSet::new();
    for torrent in torrents {
        let episode = ab_parser::raw_parser(&torrent.name);
        let episode = match episode {
            Some(ep) => ep,
            None => continue,
        };
        let title_raw = episode.title_en.clone()
            .or_else(|| episode.title_zh.clone())
            .or_else(|| episode.title_jp.clone())
            .unwrap_or_default();
        if title_raw.is_empty() || seen_titles.contains(&title_raw) {
            continue;
        }
        seen_titles.insert(title_raw.clone());

        let mut bangumi = build_bangumi_from_episode(&episode, config);
        Self::official_title_parser(
            &mut bangumi, rss, torrent, network, config, image_saver,
        ).await?;
        if !full_parse {
            return Ok(vec![bangumi]);
        }
        new_data.push(bangumi);
    }
    Ok(new_data)
}
```

**`rss_to_data`** — Full pipeline:
```rust
pub async fn rss_to_data(
    &self,
    rss: &RSSItem,
    engine: &RSSEngine,
    full_parse: bool,
    network: &NetworkClient,
    config: &ParserConfig,
    image_saver: &dyn ImageSaver,
) -> Result<Vec<Bangumi>, RssError> {
    let rss_torrents = self.get_rss_torrents(&rss.url, full_parse, network).await?;
    let unmatched = engine.bangumi_repo.match_list(&rss_torrents, &rss.url).await?;
    if unmatched.is_empty() {
        return Ok(vec![]);
    }
    let new_data = self.torrents_to_data(
        &unmatched, rss, full_parse, network, config, image_saver,
    ).await?;
    if !new_data.is_empty() {
        engine.bangumi_repo.add_all(&new_data).await?;
    }
    Ok(new_data)
}
```

**`link_to_data`** — Single RSS link → first matching Bangumi:
```rust
pub async fn link_to_data(
    &self,
    rss: &RSSItem,
    network: &NetworkClient,
    config: &ParserConfig,
    image_saver: &dyn ImageSaver,
) -> Result<Option<Bangumi>, RssError> {
    let torrents = self.get_rss_torrents(&rss.url, false, network).await?;
    for torrent in &torrents {
        if let Some(bangumi) = self.torrent_to_data(
            torrent, rss, network, config, image_saver,
        ).await? {
            return Ok(Some(bangumi));
        }
    }
    Ok(None)
}
```

**Helper** `build_bangumi_from_episode` (private, mirrors Python `raw_parser` → `Bangumi` construction in `title_parser.py:95-107`):
```rust
fn build_bangumi_from_episode(episode: &Episode, config: &ParserConfig) -> Bangumi {
    let titles = [&episode.title_zh, &episode.title_en, &episode.title_jp];
    let official_title = titles.iter()
        .find_map(|t| t.as_ref())
        .cloned()
        .unwrap_or_default();
    Bangumi {
        official_title,
        title_raw: episode.title_en.clone()
            .or_else(|| episode.title_zh.clone())
            .or_else(|| episode.title_jp.clone())
            .unwrap_or_default(),
        season: episode.season,
        season_raw: Some(episode.season_raw.clone()),
        group_name: episode.group.clone(),
        dpi: episode.resolution.clone(),
        source: episode.source.clone(),
        subtitle: episode.sub.clone(),
        eps_collect: episode.episode <= 1,  // Python: False if >1 else True
        filter: config.filters.join(","),
        ..Default::default()
    }
}
```

### `src/lib.rs` — Re-exports
```rust
pub mod analyser;
pub mod engine;
pub mod error;

pub use analyser::RSSAnalyser;
pub use engine::{RSSEngine, AddTorrent};
pub use error::RssError;
```

## Cargo.toml Dependencies

```toml
[package]
name = "ab-rss"
version.workspace = true
edition.workspace = true

[dependencies]
ab-core = { path = "../ab-core" }
ab-database = { path = "../ab-database" }
ab-network = { path = "../ab-network" }
ab-parser = { path = "../ab-parser" }
regex.workspace = true
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
tokio = { workspace = true, features = ["time"] }
tracing.workspace = true
thiserror.workspace = true
async-trait.workspace = true
chrono = { workspace = true, features = ["serde"] }   # for Utc::now() in refresh_rss
futures.workspace = true                                # for join_all
```

## File-by-File Build Order (4 files)

| # | File | Lines | Description |
|---|------|-------|-------------|
| 1 | `ab-rss/Cargo.toml` | 22 | Workspace member |
| 2 | `ab-rss/src/error.rs` | 20 | RssError enum (wraps Db/Network/Parser errors) |
| 3 | `ab-rss/src/analyser.rs` | 120 | RSSAnalyser (RSS→Bangumi conversion, enrichment) |
| 4 | `ab-rss/src/engine.rs` | 200 | RSSEngine (polling, matching, filter cache, dispatch) |
| 5 | `ab-rss/src/lib.rs` | 10 | Re-exports |

## Key Design Decisions

1. **No class inheritance** — Python's `RSSEngine(Database)` and `RSSAnalyser(TitleParser)` become structs that receive dependencies via constructor/method parameters. `RSSEngine` holds three repos (`RssRepo`, `TorrentRepo`, `BangumiRepo`) directly; `RSSAnalyser` is stateless with all dependencies passed per-method.

2. **`AddTorrent` trait** — `refresh_rss` and `download_bangumi` accept `&dyn AddTorrent` instead of depending on `ab-downloader`. This avoids a circular dependency (ab-rss → ab-downloader → ab-manager → ab-rss). The caller (ab-manager) provides an adapter that wraps `ab_downloader::DownloadClient`.

3. **Filter cache** — Uses `RefCell<HashMap<String, Regex>>` mirroring Python's `dict[str, re.Pattern]`. Thread-local (single consumer in practice). Falls back to escaped literal matching on regex parse errors, matching Python's `re.error` → `re.escape` fallback.

4. **`full_parse` semantics** — When `full_parse=false`, returns immediately after the first parsed torrent (Python `analyser.py:54-56`). Used by `link_to_data` for quick single-result lookups.

5. **`build_bangumi_from_episode`** — Centralizes the `Episode → Bangumi` conversion (moved from Python's `TitleParser.raw_parser()` lines 95-107). `eps_collect` is `true` when episode ≤ 1 (Python: `False if episode > 1 else True`).

6. **Official title sanitization** — Rust equivalent of `re.sub(r"[/:.\\]", " ", title)`: iterate characters, replace `/`, `:`, `.`, `\` with space.

7. **No `ResponseModel` returns** — Python's `add_rss` returns `ResponseModel` (success/failure with bilingual messages). Rust returns `Result<RSSItem, RssError>` — the API layer constructs the response.

8. **Concurrent RSS fetching** — Uses `futures::future::join_all` matching Python's `asyncio.gather`. Each RSS item is fetched independently; failures are captured as status updates (not propagated as errors).

9. **Status tracking** — `connection_status`, `last_checked_at`, `last_error` fields on `RSSItem` are updated after each `pull_rss` call, matching Python's `refresh_rss` behavior.

## Test Plan

**Unit tests** (`src/engine.rs`):
- `get_filter_pattern` compiles valid regex
- `get_filter_pattern` escapes invalid regex chars and falls back to literal matching
- `get_filter_pattern` caches repeated requests
- `match_torrent` returns bangumi when filter is empty
- `match_torrent` filters out torrent matching filter pattern
- `match_torrent` passes torrent not matching filter pattern
- `match_torrent` returns None when no bangumi matched

**Unit tests** (`src/analyser.rs`):
- `build_bangumi_from_episode` selects correct official_title (zh > en > jp)
- `build_bangumi_from_episode` sets `eps_collect = true` when episode ≤ 1
- `build_bangumi_from_episode` sets `eps_collect = false` when episode > 1
- `official_title_parser` strips invalid filename chars
- `torrents_to_data` deduplicates by title_raw
- `torrents_to_data` returns single result when `full_parse = false`

**Integration tests** (with wiremock for RSS endpoint + in-memory SQLite):
- `add_rss` fetches title from RSS when name is None
- `add_rss` returns error for duplicate URL
- `pull_rss` returns only new torrents (dedup by URL)
- `refresh_rss` updates connection_status on success
- `refresh_rss` updates connection_status to "error" on fetch failure
- `refresh_rss` calls `AddTorrent::add_torrent` for matched torrents
- `rss_to_data` full pipeline: fetch → match → parse → save to DB
- `link_to_data` returns first parsed Bangumi

## Acceptance Criteria

1. `cargo build -p ab-rss` compiles without errors
2. `cargo test -p ab-rss` passes all tests
3. `RSSEngine::add_rss()` correctly adds RSS feed with optional title fetch
4. `RSSEngine::pull_rss()` fetches from network and deduplicates against DB
5. `RSSEngine::match_torrent()` correctly applies filter (both regex and escaped-fallback)
6. `RSSEngine::refresh_rss()` concurrently fetches all active RSS feeds, updates status, matches torrents, dispatches to downloader
7. `RSSEngine::download_bangumi()` fetches filtered torrents and dispatches
8. `RSSAnalyser::official_title_parser()` enriches with Mikan/TMDB based on `rss.parser`
9. `RSSAnalyser::torrents_to_data()` deduplicates by `title_raw` and respects `full_parse`
10. `RSSAnalyser::rss_to_data()` completes the full pipeline (fetch → match → parse → save)
11. `AddTorrent` trait allows any downloader impl without ab-rss depending on ab-downloader
12. Filter cache handles both valid and invalid regex patterns gracefully
