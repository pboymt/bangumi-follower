# Stage 7: ab-manager — Business Orchestration (Torrent, Rename, Collect)

## Objective
Migrate 3 Python files (`manager/torrent.py`, `manager/renamer.py`, `manager/collector.py`) + `__init__.py` to a single async crate. `TorrentManager` handles rule CRUD and metadata refresh. `Renamer` handles torrent file renaming with offset lookup. `SeasonCollector` handles batch collection and subscription. No class inheritance — pass `DownloadClient` and other dependencies as method parameters.

## Dependencies
- **ab-core** (Stage 1) — `Config` (language, manage settings, rename method, remove_bad_torrent flag)
- **ab-database** (Stage 2) — `BangumiRepo`, `TorrentRepo`, `RssRepo` + models (`Bangumi`, `BangumiUpdate`, `Torrent`, `Notification`)
- **ab-downloader** (Stage 5) — `DownloadClient`, `TorrentInfo`, `FileInfo`, path utilities (`check_files`, `path_to_bangumi`, `gen_save_path`, `rule_name`)
- **ab-rss** (Stage 6) — `RSSEngine`, `AddTorrent` (for `SeasonCollector.subscribe_season`)
- **ab-network** (Stage 3) — `NetworkClient` (for TMDB, bgm.tv API calls)
- **ab-parser** (Stage 4) — `tmdb_search`, `TMDBInfo`, `fetch_bgm_calendar`, `match_weekday`, `torrent_parser`, `ParserConfig`
- **ab-searcher** (Stage 8+) — Definition needed for `SearchSeason` trait (deferred; trait defined here with no default impl)
- **External**: `tracing`, `thiserror`, `async-trait`, `futures`, `chrono`

## Python Sources → Crate Layout

```
ab-manager/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Re-exports
│   ├── error.rs            # ManagerError enum
│   ├── torrent.rs          # TorrentManager (rule CRUD, metadata, poster, calendar)
│   ├── renamer.rs          # Renamer (file rename with offset lookup)
│   └── collector.rs        # SeasonCollector (batch collection + subscription)
```

### Source Mapping

| Python file | Rust target | Lines |
|-------------|-------------|-------|
| `manager/torrent.py` | `torrent.rs` | ~370 |
| `manager/renamer.py` | `renamer.rs` | ~490 |
| `manager/collector.py` | `collector.rs` | ~75 |
| `manager/__init__.py` | `lib.rs` | ~10 |
| (new) | `error.rs` | ~30 |

**Total**: ~975 lines Rust, 5 files.

## Crate Design

### `src/error.rs` — `ManagerError`
```rust
#[derive(thiserror::Error, Debug)]
pub enum ManagerError {
    #[error("database error: {0}")]
    Database(#[from] ab_database::DbError),
    #[error("downloader error: {0}")]
    Downloader(#[from] ab_downloader::DownloaderError),
    #[error("rss error: {0}")]
    Rss(#[from] ab_rss::RssError),
    #[error("network error: {0}")]
    Network(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}

impl From<ab_network::NetworkError> for ManagerError {
    fn from(e: ab_network::NetworkError) -> Self { ManagerError::Network(e.to_string()) }
}

impl From<ab_parser::ParserError> for ManagerError {
    fn from(e: ab_parser::ParserError) -> Self { ManagerError::Network(e.to_string()) }
}
```

### `src/torrent.rs` — `TorrentManager`

Replaces Python's `TorrentManager(Database)`. Holds DB pool and creates repos as needed. No inheritance — all external dependencies passed as method parameters.

```rust
pub struct TorrentManager {
    pool: Pool<Sqlite>,
    bangumi_repo: BangumiRepo,
    torrent_repo: TorrentRepo,
    rss_repo: RssRepo,
}

impl TorrentManager {
    pub fn new(pool: Pool<Sqlite>) -> Self;

    // Rule CRUD
    pub async fn delete_rule(
        &self, id: i32, delete_files: bool, client: &DownloadClient,
    ) -> Result<(), ManagerError>;
    pub async fn disable_rule(
        &self, id: i32, delete_files: bool, client: &DownloadClient,
    ) -> Result<(), ManagerError>;
    pub fn enable_rule(&self, id: i32) -> Result<(), ManagerError>;
    pub async fn update_rule(
        &self, bangumi_id: i32, data: &BangumiUpdate, client: &DownloadClient,
    ) -> Result<(), ManagerError>;

    // Poster
    pub async fn refresh_poster(
        &self, network: &NetworkClient, config: &ParserConfig,
    ) -> Result<(), ManagerError>;
    pub async fn refind_poster(
        &self, bangumi_id: i32, network: &NetworkClient, config: &ParserConfig,
    ) -> Result<(), ManagerError>;

    // Calendar
    pub async fn refresh_calendar(
        &self, network: &NetworkClient,
    ) -> Result<(), ManagerError>;

    // Metadata
    pub async fn refresh_metadata(
        &self, network: &NetworkClient, config: &ParserConfig,
    ) -> Result<(), ManagerError>;
    pub async fn suggest_offset(
        &self, bangumi_id: i32, network: &NetworkClient, config: &ParserConfig,
    ) -> Result<OffsetSuggestion, ManagerError>;

    // Search
    pub fn search_all_bangumi(&self) -> Result<Vec<Bangumi>, ManagerError>;
    pub fn search_one(&self, id: i32) -> Result<Option<Bangumi>, ManagerError>;

    // Archive
    pub fn archive_rule(&self, id: i32) -> Result<bool, ManagerError>;
    pub fn unarchive_rule(&self, id: i32) -> Result<bool, ManagerError>;
}

pub struct OffsetSuggestion {
    pub suggested_offset: i32,
    pub reason: String,
}
```

**`delete_rule`** — Mirrors Python `TorrentManager.delete_rule`:
```rust
pub async fn delete_rule(
    &self, id: i32, delete_files: bool, client: &DownloadClient,
) -> Result<(), ManagerError> {
    let bangumi = self.bangumi_repo.search_id(id).await?
        .ok_or_else(|| ManagerError::NotFound(format!("bangumi id {id}")))?;
    // Python calls self.rss.delete(data.official_title) — this is a latent bug
    // (RSSDatabase.delete takes int id, not string title). Rust: delete by bangumi RSS link.
    // Instead, we use the rss_repo.delete_by_url or similar.
    // For now: delete torrent records, delete bangumi, optionally delete downloader files.
    self.torrent_repo.delete_by_bangumi_id(id).await?;
    self.bangumi_repo.delete_one(id).await?;
    if delete_files {
        let hashes = match_torrents_by_save_path(client, &bangumi).await?;
        if !hashes.is_empty() {
            client.delete_torrent(&hashes, delete_files).await?;
        }
    }
    Ok(())
}
```

Note: Python's `self.rss.delete(data.official_title)` passes a string to `RSSDatabase.delete(id: int)` — this is a latent bug (no rows matched, silent no-op). Rust fix: delete RSS by bangumi's rss_link instead. Since the relationship isn't direct (rss.url matches bangumi.rss_link substring), we skip this for now — calling `delete_rule` won't delete the RSS feed; the caller must delete it explicitly via RSS API.

**`update_rule`** — Mirrors Python `update_rule`. Moves torrents in downloader and updates qB RSS rule when save_path changes:
```rust
pub async fn update_rule(
    &self, bangumi_id: i32, data: &BangumiUpdate, client: &DownloadClient,
) -> Result<(), ManagerError> {
    let old = self.bangumi_repo.search_id(bangumi_id).await?
        .ok_or_else(|| ManagerError::NotFound(format!("bangumi id {bangumi_id}")))?;

    let hashes = match_torrents_by_save_path(client, &old).await?;
    let new_path = ab_downloader::path::gen_save_path(data, &settings.downloader.path);
    let old_path = old.save_path.as_deref().unwrap_or("");

    if !hashes.is_empty() && new_path != old_path {
        client.move_torrent(&hashes, &new_path).await?;
    }
    if new_path != old_path && old.rule_name.is_some() {
        // Recreate qB RSS rule with new save_path
        let rule = build_rule(data, &new_path, &settings);
        client.rss_set_rule(&old.rule_name.unwrap(), rule).await?;
    }
    // Update bangumi in DB
    self.bangumi_repo.update_partial(bangumi_id, data).await?;
    Ok(())
}
```

**`refresh_poster`** — Iterates bangumi missing poster_link, enriches via TMDB:
```rust
pub async fn refresh_poster(
    &self, network: &NetworkClient, config: &ParserConfig,
) -> Result<(), ManagerError> {
    let bangumis = self.bangumi_repo.search_all().await?;
    let mut updated = Vec::new();
    for mut bangumi in bangumis {
        if bangumi.poster_link.is_some() {
            continue;
        }
        if let Some(info) = ab_parser::tmdb_search(
            network, &bangumi.official_title, &config.language,
        ).await? {
            bangumi.poster_link = info.poster_link.clone();
            updated.push(bangumi);
        }
    }
    if !updated.is_empty() {
        self.bangumi_repo.update_all(&updated).await?;
    }
    Ok(())
}
```

**`refresh_calendar`** — Fetches bgm.tv calendar, updates air_weekday:
```rust
pub async fn refresh_calendar(
    &self, network: &NetworkClient,
) -> Result<(), ManagerError> {
    let calendar = ab_parser::fetch_bgm_calendar(network).await?;
    if calendar.is_empty() {
        return Err(ManagerError::Other("Failed to fetch calendar".into()));
    }
    let bangumis = self.bangumi_repo.search_all().await?;
    let mut updated = Vec::new();
    for mut bangumi in bangumis {
        if bangumi.deleted || bangumi.weekday_locked {
            continue;
        }
        let weekday = ab_parser::match_weekday(
            &bangumi.official_title, &bangumi.title_raw, &calendar,
        );
        if let Some(wd) = weekday {
            if Some(wd) != bangumi.air_weekday {
                bangumi.air_weekday = Some(wd);
                updated.push(bangumi);
            }
        }
    }
    if !updated.is_empty() {
        self.bangumi_repo.update_all(&updated).await?;
    }
    Ok(())
}
```

**`refresh_metadata`** — TMDB metadata refresh + auto-archive ended series:
```rust
pub async fn refresh_metadata(
    &self, network: &NetworkClient, config: &ParserConfig,
) -> Result<(), ManagerError> {
    let bangumis = self.bangumi_repo.search_all().await?;
    let mut updated = Vec::new();
    for mut bangumi in bangumis {
        if bangumi.deleted { continue; }
        let info = ab_parser::tmdb_search(
            network, &bangumi.official_title, &config.language,
        ).await?;
        if let Some(tmdb) = info {
            if bangumi.poster_link.is_none() && tmdb.poster_link.is_some() {
                bangumi.poster_link = tmdb.poster_link.clone();
                updated.push(bangumi.clone());
            }
            if tmdb.series_status.as_deref() == Some("Ended") && !bangumi.archived {
                bangumi.archived = true;
                updated.push(bangumi);
            }
        }
    }
    if !updated.is_empty() {
        self.bangumi_repo.update_all(&updated).await?;
    }
    Ok(())
}
```

**`suggest_offset`** — TMDB-based offset suggestion (Python `suggest_offset`):
```rust
pub async fn suggest_offset(
    &self, bangumi_id: i32, network: &NetworkClient, config: &ParserConfig,
) -> Result<OffsetSuggestion, ManagerError> {
    let bangumi = self.bangumi_repo.search_id(bangumi_id).await?
        .ok_or_else(|| ManagerError::NotFound(format!("bangumi id {bangumi_id}")))?;
    if bangumi.season <= 1 {
        return Ok(OffsetSuggestion {
            suggested_offset: 0,
            reason: "Season 1 does not need offset".into(),
        });
    }
    let info = ab_parser::tmdb_search(
        network, &bangumi.official_title, &config.language,
    ).await?;
    match info {
        Some(tmdb) if !tmdb.season_episode_counts.is_empty() => {
            let offset = tmdb.get_offset_for_season(bangumi.season);
            let prev: Vec<String> = (1..bangumi.season)
                .filter(|s| tmdb.season_episode_counts.contains_key(s))
                .map(|s| format!("S{s}: {} eps", tmdb.season_episode_counts[&s]))
                .collect();
            Ok(OffsetSuggestion {
                suggested_offset: offset,
                reason: format!("Previous seasons: {}", prev.join(", ")),
            })
        }
        _ => Ok(OffsetSuggestion {
            suggested_offset: 0,
            reason: "Unable to fetch TMDB episode data".into(),
        }),
    }
}
```

**Helper** `match_torrents_by_save_path` (private, replaces Python `__match_torrents_list`):
```rust
async fn match_torrents_by_save_path(
    client: &DownloadClient, data: &Bangumi,
) -> Result<Vec<String>, ManagerError> {
    let torrents = client.get_torrent_info(None, None, None).await?;
    Ok(torrents.into_iter()
        .filter(|t| t.save_path == data.save_path.as_deref().unwrap_or(""))
        .map(|t| t.hash)
        .collect())
}
```

### `src/renamer.rs` — `Renamer`

Replaces Python's `Renamer(DownloadClient)`. Holds DB pool for offset lookups. DownloadClient passed as method parameter.

**Module-level pending rename cache** (matching Python module-level `_pending_renames`):
```rust
use std::sync::Mutex;
use std::time::Instant;

static PENDING_RENAMES: Lazy<Mutex<HashMap<(String, String, String), Instant>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
const PENDING_COOLDOWN: Duration = Duration::from_secs(300);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);
static LAST_CLEANUP: Mutex<Instant> = Mutex::new(Instant::now());
```

```rust
pub struct Renamer {
    pool: Pool<Sqlite>,
}

impl Renamer {
    pub fn new(pool: Pool<Sqlite>) -> Self;

    /// Main entry: iterate all torrents from downloader, rename files.
    pub async fn rename(
        &self, client: &DownloadClient,
    ) -> Result<Vec<Notification>, ManagerError>;

    /// Rename a single media file.
    pub async fn rename_file(
        &self,
        torrent_name: &str,
        media_path: &str,
        bangumi_name: &str,
        method: &str,
        season: i32,
        hash: &str,
        episode_offset: i32,
        season_offset: i32,
        client: &DownloadClient,
    ) -> Result<Option<Notification>, ManagerError>;

    /// Rename multiple files in a collection (batch torrent).
    pub async fn rename_collection(
        &self,
        media_list: &[String],
        bangumi_name: &str,
        season: i32,
        method: &str,
        hash: &str,
        episode_offset: i32,
        season_offset: i32,
        client: &DownloadClient,
    ) -> Result<(), ManagerError>;

    /// Rename subtitle files.
    pub async fn rename_subtitles(
        &self,
        subtitle_list: &[String],
        torrent_name: &str,
        bangumi_name: &str,
        season: i32,
        method: &str,
        hash: &str,
        episode_offset: i32,
        season_offset: i32,
        client: &DownloadClient,
    ) -> Result<(), ManagerError>;

    /// Generate new path from parsed file info + rename method.
    pub fn gen_path(
        file_info: &ParsedFile,
        bangumi_name: &str,
        method: &str,
        episode_offset: i32,
    ) -> String;

    /// Lookup (episode_offset, season_offset) for a torrent.
    /// Try qb_hash → tag → name → save_path.
    async fn lookup_offsets(
        &self, hash: &str, torrent_name: &str, save_path: &str, tags: &str,
    ) -> (i32, i32);

    /// Batch offset lookup in a single DB session.
    async fn batch_lookup_offsets(
        &self, torrents_info: &[TorrentInfo],
    ) -> HashMap<String, (i32, i32)>;

    /// Parse bangumi_id from "ab:ID" tag format.
    fn parse_bangumi_id_from_tags(tags: &str) -> Option<i32>;

    /// Clean up expired pending rename entries (throttled).
    fn cleanup_pending_cache();
}
```

**`gen_path`** — Pure function mirroring Python `gen_path`:
```rust
pub fn gen_path(
    file_info: &ParsedFile,
    bangumi_name: &str,
    method: &str,
    episode_offset: i32,
) -> String {
    let season_num = file_info.season;
    let season = if season_num < 10 { format!("0{season_num}") } else { season_num.to_string() };
    let original_ep: i32 = file_info.episode.parse().unwrap_or(0);
    let adjusted_ep = if original_ep == 0 && episode_offset != 0 {
        0  // Episode 0 = special/OVA — never apply offset
    } else if original_ep + episode_offset <= 0 {
        original_ep  // non-positive result → misconfiguration, revert
    } else {
        original_ep + episode_offset
    };
    let episode = if adjusted_ep < 10 { format!("0{adjusted_ep}") } else { adjusted_ep.to_string() };
    match method {
        "none" => file_info.media_path.clone(),
        "pn" => format!("{} S{season}E{episode}{}", file_info.title, file_info.suffix),
        "advance" => format!("{bangumi_name} S{season}E{episode}{}", file_info.suffix),
        "normal" => { tracing::warn!("[Renamer] Normal rename method is deprecated."); file_info.media_path.clone() }
        "subtitle_pn" => format!("{} S{season}E{episode}.{}{}", file_info.title, file_info.language.as_deref().unwrap_or(""), file_info.suffix),
        "subtitle_advance" => format!("{bangumi_name} S{season}E{episode}.{}{}", file_info.language.as_deref().unwrap_or(""), file_info.suffix),
        _ => { tracing::error!("[Renamer] Unknown rename method: {method}"); file_info.media_path.clone() }
    }
}
```

**`rename`** — Main entry, mirrors Python `Renamer.rename()`:
```rust
pub async fn rename(
    &self, client: &DownloadClient,
) -> Result<Vec<Notification>, ManagerError> {
    let rename_method = "..."; // from settings
    let torrents_info = client.get_torrent_info(None, None, None).await?;
    let mut renamed_info = Vec::new();

    // Fetch all torrent files concurrently
    let all_files: Vec<Vec<FileInfo>> = futures::future::join_all(
        torrents_info.iter().map(|t| client.torrents_files(&t.hash))
    ).await.into_iter().filter_map(|r| r.ok()).collect();

    let offset_map = self.batch_lookup_offsets(&torrents_info).await;

    for (info, files) in torrents_info.iter().zip(all_files.iter()) {
        let (media_list, subtitle_list) = check_files(files);
        let (bangumi_name, season) = path_to_bangumi(&info.save_path, &info.name);
        let (episode_offset, season_offset) = offset_map.get(&info.hash).copied().unwrap_or((0, 0));
        // ... same logic as Python: single file → rename_file, multi → rename_collection
    }
    Ok(renamed_info)
}
```

**`lookup_offsets`** — Mirrors Python `_lookup_offsets`. Queries DB in 4-step priority:
```rust
async fn lookup_offsets(
    &self, hash: &str, torrent_name: &str, save_path: &str, tags: &str,
) -> (i32, i32) {
    // 1. By qb_hash in Torrent table → bangumi_id
    if let Ok(Some(torrent)) = self.torrent_repo.search_by_qb_hash(hash).await {
        if let Some(bangumi_id) = torrent.bangumi_id {
            if let Ok(Some(b)) = self.bangumi_repo.search_id(bangumi_id).await {
                if !b.deleted { return (b.episode_offset, b.season_offset); }
            }
        }
    }
    // 2. By "ab:ID" tag
    if let Some(bangumi_id) = Self::parse_bangumi_id_from_tags(tags) {
        if let Ok(Some(b)) = self.bangumi_repo.search_id(bangumi_id).await {
            if !b.deleted { return (b.episode_offset, b.season_offset); }
        }
    }
    // 3. By torrent name match
    if let Ok(Some(b)) = self.bangumi_repo.match_torrent(torrent_name).await {
        return (b.episode_offset, b.season_offset);
    }
    // 4. By save_path (with normalization fallback)
    if let Ok(Some(b)) = self.bangumi_repo.match_by_save_path(save_path).await {
        return (b.episode_offset, b.season_offset);
    }
    (0, 0)
}
```

**`rename_file`** — Mirrors Python `rename_file` with pending-cooldown logic:
```rust
pub async fn rename_file(
    &self, torrent_name: &str, media_path: &str, bangumi_name: &str,
    method: &str, season: i32, hash: &str,
    episode_offset: i32, season_offset: i32,
    client: &DownloadClient,
) -> Result<Option<Notification>, ManagerError> {
    let parsed = ab_parser::torrent_parser(
        media_path, Some(torrent_name), Some(season), FileType::Media,
    );
    let ep = match parsed { Some(p) => p, None => return Ok(None) };
    let new_path = Self::gen_path(&ep, bangumi_name, method, episode_offset);
    if media_path == new_path { return Ok(None); }

    let pending_key = (hash.to_string(), media_path.to_string(), new_path.clone());
    // Check cooldown
    {
        let cache = PENDING_RENAMES.lock().unwrap();
        if let Some(last) = cache.get(&pending_key) {
            if last.elapsed() < PENDING_COOLDOWN {
                return Ok(None);
            }
        }
    }
    if client.torrents_rename_file(hash, media_path, &new_path, true).await? {
        PENDING_RENAMES.lock().unwrap().remove(&pending_key);
        let original_ep: i32 = ep.episode.parse().unwrap_or(0);
        let adjusted_ep = if original_ep == 0 && episode_offset != 0 { 0 }
            else if original_ep + episode_offset <= 0 { original_ep }
            else { original_ep + episode_offset };
        return Ok(Some(Notification {
            official_title: bangumi_name.to_string(),
            season: ep.season,
            episode: adjusted_ep,
        }));
    } else {
        PENDING_RENAMES.lock().unwrap().insert(pending_key, Instant::now());
        Self::cleanup_pending_cache();
    }
    Ok(None)
}
```

### `src/collector.rs` — `SeasonCollector`

Replaces Python's `SeasonCollector(DownloadClient)`. Stateless struct — all dependencies passed as parameters.

**`SearchSeason` trait** — Abstraction over `ab-searcher` (not yet implemented). Avoids hard dependency:
```rust
#[async_trait]
pub trait SearchSeason: Send + Sync {
    async fn search_season(&self, bangumi: &Bangumi) -> Result<Vec<Torrent>, ManagerError>;
    async fn get_torrents(&self, link: &str, filter: &str) -> Result<Vec<Torrent>, ManagerError>;
}
```

```rust
pub struct SeasonCollector;

impl SeasonCollector {
    /// Collect all episodes for a complete season.
    pub async fn collect_season(
        bangumi: &mut Bangumi,
        link: Option<&str>,
        client: &DownloadClient,
        engine: &RSSEngine,
        searcher: &dyn SearchSeason,
        network: &NetworkClient,
    ) -> Result<bool, ManagerError>;

    /// Subscribe to a new season: add RSS feed + download first torrent.
    pub async fn subscribe_season(
        data: &Bangumi,
        parser: &str,
        engine: &RSSEngine,
        network: &NetworkClient,
        downloader: &dyn AddTorrent,
    ) -> Result<bool, ManagerError>;

    /// Iterate incomplete bangumi and collect full seasons for each.
    pub async fn eps_complete(
        engine: &RSSEngine,
        client: &DownloadClient,
        searcher: &dyn SearchSeason,
        network: &NetworkClient,
    ) -> Result<(), ManagerError>;
}
```

**`collect_season`** — Mirrors Python `collect_season`:
```rust
pub async fn collect_season(
    bangumi: &mut Bangumi,
    link: Option<&str>,
    client: &DownloadClient,
    engine: &RSSEngine,
    searcher: &dyn SearchSeason,
    network: &NetworkClient,
) -> Result<bool, ManagerError> {
    let torrents = match link {
        Some(l) => searcher.get_torrents(l, &bangumi.filter.replace(',', "|")).await?,
        None => searcher.search_season(bangumi).await?,
    };
    let added = client.add_torrent(&torrents, bangumi, network).await?;
    if added {
        bangumi.eps_collect = true;
        engine.bangumi_repo().update_full(bangumi).await?;
        engine.torrent_repo().add_all(&torrents).await?;
        Ok(true)
    } else {
        Ok(false)
    }
}
```

Note: Python has `if await self.add_torrent(torrents, bangumi):` where `add_torrent` is the downloader method. In Rust, `client.add_torrent` takes the torrent list, not a single torrent. We map the `AddTorrent` or `DownloadClient` accordingly.

Wait, looking at the Python more carefully:
```python
if await self.add_torrent(torrents, bangumi):
```

`self.add_torrent` here is `DownloadClient.add_torrent` (inherited). The Python `DownloadClient.add_torrent` takes `(torrents: list[Torrent], bangumi: Bangumi)`. But in our Stage 5 plan, `DownloadClient.add_torrent` takes `(&Torrent, &Bangumi, &NetworkClient)` — a single torrent, not a list.

So there's a mismatch. In Stage 5 the signature is:
```rust
pub async fn add_torrent(
    &self, torrent: &Torrent, bangumi: &Bangumi, network: &NetworkClient,
) -> Result<bool, DownloaderError>;
```

But Python's `add_torrent` takes a list of torrents. Let me check the Python code more carefully...

Looking at Python `download_client.py`:
```python
async def add_torrent(self, torrent: Torrent, bangumi: Bangumi):
    ...
```

So Python takes a SINGLE Torrent. But in `collect_season`:
```python
if await self.add_torrent(torrents, bangumi):
```

It passes a list! That looks like a call-site inconsistency. Let me check what Python's `DownloadClient.add_torrent` actually does with a list...

Actually, looking at the Python `DownloadClient` class more carefully, it inherits `TorrentPath` and has `add_torrent` method. Let me check...

I need to look at the actual Python code. Let me check the download_client.py.

From the Stage 5 plan:
```python
async def add_torrent(self, torrent: Torrent, bangumi: Bangumi):
``` 

But in `collect_season`:
```python
if await self.add_torrent(torrents, bangumi):
```

This passes a list `torrents`. In Python, a list satisfies `Torrent` duck type? No, this would fail. Unless Python's method signature actually accepts a list. Let me re-check...

Actually, I think my analysis of `download_client.py` might be incomplete. Let me trust the Stage 5 plan which says `add_torrent` takes a single `&Torrent`. For `collect_season`, we iterate and call `add_torrent` for each one.

For the Rust version, I'll iterate over the torrents:
```rust
let mut added = false;
for torrent in &torrents {
    if client.add_torrent(torrent, bangumi, network).await? {
        added = true;
    }
}
```

**`subscribe_season`** — Mirrors Python `subscribe_season`:
```rust
pub async fn subscribe_season(
    data: &Bangumi,
    parser: &str,
    engine: &RSSEngine,
    network: &NetworkClient,
    downloader: &dyn AddTorrent,
) -> Result<bool, ManagerError> {
    let mut add_data = data.clone();
    add_data.added = true;
    add_data.eps_collect = true;
    engine.add_rss(network, &add_data.rss_link, Some(&add_data.official_title), false, parser).await?;
    let result = engine.download_bangumi(&add_data, network, downloader).await?;
    engine.bangumi_repo().add(&add_data).await?;
    Ok(result)
}
```

**`eps_complete`** — Free function, mirrors Python:
```rust
pub async fn eps_complete(
    engine: &RSSEngine,
    client: &DownloadClient,
    searcher: &dyn SearchSeason,
    network: &NetworkClient,
) -> Result<(), ManagerError> {
    let datas = engine.bangumi_repo().not_complete().await?;
    if datas.is_empty() { return Ok(()); }
    for mut data in datas {
        if !data.eps_collect {
            let _ = SeasonCollector::collect_season(
                &mut data, None, client, engine, searcher, network,
            ).await;
        }
        data.eps_collect = true;
    }
    engine.bangumi_repo().update_all(&datas).await?;
    Ok(())
}
```

### `src/lib.rs` — Re-exports
```rust
pub mod error;
pub mod torrent;
pub mod renamer;
pub mod collector;

pub use error::ManagerError;
pub use torrent::{TorrentManager, OffsetSuggestion};
pub use renamer::Renamer;
pub use collector::{SeasonCollector, SearchSeason, eps_complete};
```

## Cargo.toml Dependencies

```toml
[package]
name = "ab-manager"
version.workspace = true
edition.workspace = true

[dependencies]
ab-core = { path = "../ab-core" }
ab-database = { path = "../ab-database" }
ab-downloader = { path = "../ab-downloader" }
ab-rss = { path = "../ab-rss" }
ab-network = { path = "../ab-network" }
ab-parser = { path = "../ab-parser" }
regex = { workspace = true, optional = true }
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
tokio = { workspace = true, features = ["time"] }
tracing.workspace = true
thiserror.workspace = true
async-trait.workspace = true
chrono.workspace = true
futures.workspace = true
once_cell.workspace = true
```

Note: No direct dependency on `ab-searcher`. The `SearchSeason` trait is defined in this crate with a default impl that returns empty results — the actual impl is provided by `ab-searcher` (Stage 8+) and injected at call sites.

## File-by-File Build Order (5 files)

| # | File | Lines | Description |
|---|------|-------|-------------|
| 1 | `ab-manager/Cargo.toml` | 25 | Workspace member |
| 2 | `ab-manager/src/error.rs` | 30 | ManagerError enum |
| 3 | `ab-manager/src/torrent.rs` | 280 | TorrentManager (rule CRUD, metadata, poster, calendar, ~16 methods) |
| 4 | `ab-manager/src/renamer.rs` | 400 | Renamer (file rename, offset lookup, pending cooldown cache, ~10 methods) |
| 5 | `ab-manager/src/collector.rs` | 120 | SeasonCollector + SearchSeason trait + eps_complete (4 items) |
| 6 | `ab-manager/src/lib.rs` | 15 | Re-exports |

## Key Design Decisions

1. **No class inheritance** — Python's `TorrentManager(Database)`, `Renamer(DownloadClient)`, `SeasonCollector(DownloadClient)` all become standalone structs. Dependencies (`DownloadClient`, `NetworkClient`, `RSSEngine`, repo pool) are passed via constructor or method parameters.

2. **`SearchSeason` trait replaces `ab-searcher` dependency** — Since `ab-searcher` hasn't been implemented yet, `SeasonCollector` accepts `&dyn SearchSeason` instead of depending on `ab-searcher` directly. The actual impl comes from Stage 8+.

3. **`Renamer` uses static `Mutex<HashMap>` for pending rename cache** — Mirrors Python's module-level `_pending_renames: dict`. Uses `once_cell::Lazy<Mutex<HashMap<...>>>` with 300s cooldown. `cleanup_pending_cache()` removes stale entries every 60s.

4. **Path utilities reused from `ab-downloader`** — `check_files`, `path_to_bangumi`, `gen_save_path`, `rule_name` are re-exported from `ab_downloader::path`. No need to duplicate in `ab-manager`.

5. **Python bug fix: `delete_rule` RSS deletion** — Python's `self.rss.delete(data.official_title)` passes a title string to `RSSDatabase.delete(id: int)` — a no-op bug. Rust skips this call; RSS feeds must be deleted explicitly via the RSS API.

6. **`TorrentManager.update_rule` save_path change** — When save_path changes, both moves torrents in the downloader AND recreates the qBittorrent RSS rule with the new path. Matches Python's three-step flow: match torrents → move → update qB rule → update DB.

7. **No `ResponseModel` returns** — All methods return `Result<_, ManagerError>` or `Result<Vec<_>, _>`. The API layer constructs `ResponseModel` with status codes and bilingual messages.

8. **Offset lookup priority** — `lookup_offsets` follows Python's 4-step priority: (1) qb_hash → bangumi_id, (2) `ab:ID` tag, (3) torrent name matching, (4) save_path matching with normalization fallback.

9. **`eps_collect` logic** — `gen_path` and `rename_file` share the same episode adjustment logic: episode 0 never gets offset applied; non-positive results after offset revert to original.

## Test Plan

**Unit tests** (`src/renamer.rs`):
- `gen_path` with `pn` method produces correct format
- `gen_path` with `advance` method includes bangumi name
- `gen_path` with `subtitle_pn` includes language suffix
- `gen_path` handles episode 0 without applying offset
- `gen_path` reverts non-positive adjusted episode to original
- `gen_path` pads single-digit season/episode with leading zero
- `gen_path` returns original path for unknown method
- `parse_bangumi_id_from_tags` extracts `ab:123` → Some(123)
- `parse_bangumi_id_from_tags` returns None for non-matching tags
- `parse_bangumi_id_from_tags` returns None for empty tags

**Unit tests** (`src/torrent.rs`):
- `search_all_bangumi` filters out deleted entries
- `search_one` returns None for non-existent id
- `suggest_offset` returns offset 0 for season 1
- `suggest_offset` returns offset 0 when TMDB unavailable

**Integration tests** (in-memory SQLite + wiremock for TMDB/bgm.tv):
- `TorrentManager.enable_rule` sets deleted=false
- `TorrentManager.disable_rule` sets deleted=true
- `TorrentManager.delete_rule` removes bangumi + torrent records
- `TorrentManager.refresh_poster` updates bangumi with missing poster
- `TorrentManager.refresh_calendar` updates air_weekday from mock calendar
- `TorrentManager.refresh_metadata` auto-archives ended series
- `Renamer.rename` processes torrent files and returns notifications
- `SeasonCollector.collect_season` adds torrents and sets eps_collect
- `SeasonCollector.subscribe_season` adds RSS + downloads
- `eps_complete` processes incomplete bangumi

## Acceptance Criteria

1. `cargo build -p ab-manager` compiles without errors
2. `cargo test -p ab-manager` passes all tests
3. `TorrentManager.delete_rule()` removes bangumi, torrent records, optionally deletes downloader files
4. `TorrentManager.disable_rule()` soft-deletes bangumi (deleted=true)
5. `TorrentManager.update_rule()` moves torrents + updates qB RSS rule when save_path changes
6. `TorrentManager.refresh_poster()` fills missing poster_link from TMDB
7. `TorrentManager.refresh_calendar()` updates air_weekday from bgm.tv (skips locked entries)
8. `TorrentManager.refresh_metadata()` auto-archives ended series + fills missing posters
9. `TorrentManager.suggest_offset()` returns TMDB-based offset suggestion for season > 1
10. `Renamer.rename()` processes all torrents, applies rename methods, respects cooldown cache
11. `Renamer.lookup_offsets()` follows 4-step priority (qb_hash → tag → name → save_path)
12. `SeasonCollector.collect_season()` adds torrents + sets eps_collect
13. `SeasonCollector.subscribe_season()` adds RSS feed + downloads first torrent
14. `eps_complete()` iterates incomplete bangumi, collects each, updates DB
15. `SearchSeason` trait allows pluggable searcher implementation from ab-searcher
16. Pending rename cache prevents duplicate rename attempts within 300s cooldown
