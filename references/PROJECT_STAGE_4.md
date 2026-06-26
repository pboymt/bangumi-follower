# Stage 4: ab-parser — Title Analysis + Metadata Enrichment

## Objective
Migrate 10 Python files (title regex parsing, TMDB/Mikan/BGM API enrichment, offset detection) to a single async crate. Keep pure-logic parsers (regex-based) separate from API-dependent enrichers (TMDB, Mikan, etc.). Defer the OpenAI parser to a later dedicated stage.

## Dependencies
- **ab-core** (Stage 1) — `Config` (TMDB API key, filter, experimental flags)
- **ab-network** (Stage 3) — `NetworkClient` for HTTP calls to TMDB, bgm.tv, Mikan
- **External**: `regex`, `serde` + `serde_json`, `quick-xml` (bgm.tv API), `tracing`, `thiserror`

## Python Sources → Crate Layout

```
ab-parser/
├── Cargo.toml
├── src/
│   ├── lib.rs                                    # Re-exports: TitleParser facade
│   ├── error.rs                                  # ParserError enum
│   ├── torrent_parser.rs                         # Regex-based torrent filename → EpisodeFile/SubtitleFile
│   ├── raw_parser.rs                             # Fallback parse (unstructured title → Episode)
│   ├── offset_detector.rs                        # TMDB-vs-parsed season/episode offset mismatch
│   ├── enricher/
│   │   ├── mod.rs
│   │   ├── tmdb.rs                               # TMDB API search + season/episode data
│   │   ├── mikan.rs                              # Mikanani homepage scraper
│   │   └── bgm.rs                                # bgm.tv calendar + search
│   └── title_parser.rs                           # High-level orchestration (was TitleParser class)
```

### Source Mapping

| Python file (parser/) | Rust target | Lines | I/O? |
|-----------------------|-------------|-------|------|
| `analyser/torrent_parser.py` | `torrent_parser.rs` | ~130 | No |
| `analyser/raw_parser.py` | `raw_parser.rs` | ~180 | No |
| `analyser/offset_detector.py` | `offset_detector.rs` | ~120 | No |
| `analyser/tmdb_parser.py` | `enricher/tmdb.rs` | ~300 | Yes (NetworkClient) |
| `analyser/mikan_parser.py` | `enricher/mikan.rs` | ~50 | Yes (NetworkClient) |
| `analyser/bgm_parser.py` | `enricher/bgm.rs` | ~80 | Yes (NetworkClient) |
| `analyser/bgm_calendar.py` | included in `enricher/bgm.rs` | — | Yes (NetworkClient) |
| `title_parser.py` | `title_parser.rs` | ~100 | Mixed |
| (new) | `enricher/mod.rs` | ~5 | — |
| (new) | `error.rs` | ~30 | — |
| (new) | `lib.rs` | ~15 | — |

**Deferred to dedicated stage**: `analyser/openai.py` (OpenAI/GPT-based parsing). Depends on `async-openai` crate and has no downstream urgency.

**Total**: ~1,020 lines Rust, 11 files (+ 1 deferred).

## Crate Design

### `src/error.rs` — `ParserError`
```rust
#[derive(thiserror::Error, Debug)]
pub enum ParserError {
    #[error("parse failed: {0}")]
    ParseFailed(String),
    #[error("network error: {0}")]
    Network(#[from] ab_network::NetworkError),
    #[error("no match found")]
    NoMatch,
}
```

### `src/torrent_parser.rs` — Regex Torrent Name Parser

Mirrors `torrent_parser.py` exactly: same 5 regex rules, same LRU cache, same group/season/subtitle extraction.

Python returns `EpisodeFile` (media) or `SubtitleFile` (with `language`) — Rust mirrors this split:

```rust
/// Media file (video) — mirrors Python's EpisodeFile
pub struct ParsedFile {
    pub media_path: String,
    pub group: Option<String>,
    pub title: String,
    pub season: i32,
    pub episode: String,       // "1", "48.5", etc. — Python 存储为 int|float (Pydantic 从 regex 强制转换)
                               // Rust 保存原始字符串, 调用方按需 parse::<f64>()
    pub suffix: String,
}

/// Subtitle file — mirrors Python's SubtitleFile with language field
pub struct ParsedSubtitle {
    pub media_path: String,
    pub group: Option<String>,
    pub title: String,
    pub season: i32,
    pub episode: String,       // 同上, Python 实际类型 int|float
    pub suffix: String,
    pub language: Option<String>,  // "zh" or "zh-tw"
}

pub enum FileType { Media, Subtitle }

/// Parse a torrent file path into structured file info.
/// Checks cache first; falls back to regex matching.
pub fn torrent_parser(
    torrent_path: &str,
    torrent_name: Option<&str>,
    season: Option<i32>,
    file_type: FileType,
) -> Option<ParsedFile>;

/// Parse a torrent file path as a subtitle file, detecting language.
pub fn subtitle_parser(
    torrent_path: &str,
    torrent_name: Option<&str>,
    season: Option<i32>,
) -> Option<ParsedSubtitle>;
```

**Rules** (ordered list, first match wins). Python compiles these with `re.I` (`torrent_parser.py:24`); Rust must prefix each rule with `(?i)` for case-insensitive matching:

1. `(?i)(.*) - (\d{1,4}(?:\.\d{1,2})?(?!\d|p))(?:v\d{1,2})?(?: )?(?:END)?(.*)`
2. `(?i)(.*)[\[\ E](\d{1,4}(?:\.\d{1,2})?)(?:v\d{1,2})?(?: )?(?:END)?[\]\ ](.*)`
3. `(?i)(.*)\[(?:第)?(\d{1,4}(?:\.\d{1,2})?)[话集話](?:END)?\](.*)`
4. `(?i)(.*)第?(\d{1,4}(?:\.\d{1,2})?)[话話集](?:END)?(.*)`
5. `(?i)(.*)(?:S\d{2})?EP?(\d{1,4}(?:\.\d{1,2})?)(.*)`

**Cache**: `LruCache<(String, Option<String>, Option<i32>, FileType), Option<ParsedFile>>` — size 512.

**Helper functions** (trait on `&str`):
- `get_group(group_and_title: &str) -> (Option<String>, String)` — splits `[Group] Title`
- `get_season_and_title(season_and_title: &str) -> (String, i32)` — extracts `S1`/`Season 1`
- `get_subtitle_lang(filename: &str) -> Option<&str>` — detects "zh"/"zh-tw" from keywords

**Subtitle language detection**:
```rust
const SUBTITLE_LANG: &[(&str, &[&str])] = &[
    ("zh-tw", &["tc", "cht", "繁", "zh-tw"]),
    ("zh",    &["sc", "chs", "简", "zh"]),
];
```

### `src/offset_detector.rs` — Season/Episode Offset Mismatch Detection

Migrates `analyser/offset_detector.py` (135 lines). Pure logic (no I/O).

**核心结构体** (Python offset_detector.py:12-19):
```rust
#[derive(Debug, Clone)]
pub struct OffsetSuggestion {
    pub season_offset: i32,
    pub episode_offset: Option<i32>,   // None = 无需调整剧集
    pub reason: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}
```

**主检测函数** (Python offset_detector.py:22-135):
```rust
/// 检测番剧的季度/剧集偏移是否与 TMDB 数据不匹配。
/// 逻辑:
/// 1. tmdb_info 或 last_season 为空 → None (无需调整)
/// 2. parsed_season > tmdb_info.last_season → 计算 season_offset (last_season - parsed_season)
/// 3. 查询 virtual_season_starts 找到目标季度的虚拟季度索引
/// 4. 如果虚拟季度索引 > 0 → 计算 episode_offset (vs_starts[index] - 1)
/// 5. 检查调整后的剧集是否超出 season_episode_counts
/// 6. 如果仍在连载中 (series_status) → 置信度降为 medium
/// 7. 累积原因; 无原因 → None
pub fn detect_offset_mismatch(
    parsed_season: i32,
    parsed_episode: i32,
    tmdb_info: &TMDBInfo,
) -> Option<OffsetSuggestion>;
```

**辅助函数**:
- `detect_virtual_seasons(episodes: &[EpisodeAirDate], gap_months: u32) -> Vec<i32>` — 检测空档超过 N 个月的虚拟季度分界点
- `get_aired_episode_count(tv_id: i32, season_number: i32, language: &str) -> i32` — 统计已播出剧集数 (需 NetworkClient)

### `src/raw_parser.rs` — Unstructured Title Parser

Mirrors `raw_parser.py` with the same regex pipeline: preprocess → `TITLE_RE` match → prefix processing → season extraction → name splitting (en/zh/jp) → tag extraction.

```rust
/// Parse an unstructured raw title string into structured episode info.
pub fn raw_parser(raw: &str) -> Option<Episode>;
```

**Regex constants** (compiled with `once_cell::sync::Lazy`):
- `TITLE_RE`: `(.*?|\[.*])((?: ?-) ?\d+ |\[\d+]|...)(.*)` — same as Python
- `RESOLUTION_RE`: `1080|720|2160|4K`
- `SOURCE_RE`: `B-Global|[Bb]aha|[Bb]ilibili|AT-X|Web`
- `SUB_RE`: `[简繁日字幕]|CH|BIG5|GB`
- `EPISODE_RE`: `\d+`
- `PREFIX_RE`: `[^\w\s\u4e00-\u9fff\u3040-\u309f\u30a0-\u30ff-]` — 组名前缀特殊字符清理 (raw_parser.py:21)
- `CHINESE_NUMBER_MAP`: 中文数字→阿拉伯数字映射 `{"一":1, "二":2, ..., "十":10}` (raw_parser.py:36-47) — `season_process` 在匹配 `第.[季期]` 后需要转换
- Fallback patterns for edge cases (digits before `[`, `[02(57)]`)

**Pipeline**: `pre_process` → `get_group` → `TITLE_RE.match` (or `_fallback_parse`) → `prefix_process` → `season_process` → `name_process` → `find_tags`

**`Episode` output**:
```rust
pub struct Episode {
    pub title_en: Option<String>,
    pub title_zh: Option<String>,
    pub title_jp: Option<String>,
    pub season: i32,
    pub season_raw: String,
    pub episode: i32,
    pub sub: String,           // Python (bangumi.py:110) 声明为非可选 str
    pub group: String,         // Python (bangumi.py:111) 同上
    pub resolution: String,    // Python (bangumi.py:112) 同上
    pub source: String,        // Python (bangumi.py:113) 同上
}
// 注: Python dataclass 声明这 4 个字段为必填 str 非 Optional,
// 但 raw_parser 的 find_tags 可能返回 None — 使用空字符串兜底
```

### `src/enricher/tmdb.rs` — TMDB API Client

Mirrors `tmdb_parser.py`. Uses `NetworkClient` from `ab-network` for all HTTP calls. No direct `reqwest` usage.

```rust
pub struct TMDBInfo {
    pub id: i32,
    pub title: String,
    pub original_title: String,
    pub seasons: Vec<TMDBSeason>,      // Python 字段名 season (list[dict])
    pub last_season: i32,
    pub year: String,
    pub poster_link: Option<String>,
    pub series_status: Option<String>,
    pub season_episode_counts: HashMap<i32, i32>,
    pub virtual_season_starts: HashMap<i32, Vec<i32>>,
}

impl TMDBInfo {
    /// 获取指定季度的虚拟季度偏移 (Python tmdb_parser.py:36-44)
    /// 返回 (episode_offset, reason, confidence)
    pub fn get_offset_for_season(&self, season_number: i32) -> Option<OffsetSuggestion>;
}

pub struct TMDBSeason {
    pub season: String,
    pub air_date: Option<String>,
    pub poster_path: Option<String>,
}

/// Search TMDB for a TV show by title.
/// Returns None if no animation match found.
pub async fn tmdb_search(
    client: &NetworkClient,
    title: &str,
    language: &str,
    api_key: &str,
    test: bool,              // Python tmdb_parser.py:287-296: true=返回 URL, false=下载并保存图片
) -> Result<Option<TMDBInfo>, ParserError>;
```

Key differences from Python:
- `api_key` is passed explicitly (not from module-level `TMDB_API` import)
- `LruCache` for TMDB results (same 512 size, key = `{title}:{language}`)
- All network calls use the injected `NetworkClient`

**Sub-functions** (all async):
- `is_animation(tv_id, language) -> bool` — check genre `id == 16`
- `get_season_episodes(tv_id, season_number, language) -> Vec<EpisodeAirDate>` — fetch air dates
- `detect_virtual_seasons(episodes, gap_months=6) -> Vec<i32>` — pure logic (no I/O)
- `get_aired_episode_count(tv_id, season_number, language) -> i32` — count aired episodes
- `get_season(tv_id, language) -> (i32, String)` — 找到最近有播出日期季度 (Python tmdb_parser.py:185-199)

**URL construction** (configurable base):
- Search: `{BASE}/3/search/tv?api_key={key}&query={title}&include_adult=false`
- Info: `{BASE}/3/tv/{id}?api_key={key}&language={lang}`
- Season: `{BASE}/3/tv/{id}/season/{n}?api_key={key}&language={lang}`
- Poster: `https://image.tmdb.org/t/p/w780{path}`

### `src/enricher/mikan.rs` — Mikanani Homepage Scraper

Mirrors `mikan_parser.py`. Parses Mikanani episode homepage HTML for poster URL and official title.

```rust
/// Fetch poster + official title from Mikanani episode homepage.
pub async fn mikan_parse(
    client: &NetworkClient,
    homepage: &str,
    image_saver: &dyn ImageSaver,
) -> Result<(String, String), ParserError>;
```

**Note**: `image_saver` abstracts image storage (Python: `module.utils.save_image`). In `ab-parser`, we define a trait:

```rust
pub trait ImageSaver: Send + Sync {
    fn save(&self, data: &[u8], suffix: &str) -> String;
}
```

The impl is provided by `ab-core` or `ab-manager` (Stage 7). For Stage 4, provide a no-op stub.

**Caching**: `LruCache<String, (String, String)>` (key = homepage URL, value = (poster_link, official_title)).

**Parser logic**:
1. Fetch HTML with `NetworkClient`
2. Extract `div.bangumi-poster` style attribute for poster URL
3. Extract `p.bangumi-title a[href^="/Home/Bangumi/"]` text for official title
4. Strip season suffix from title (`第.*季`)
5. Download poster image
6. Cache result

### `src/enricher/bgm.rs` — Bangumi.tv Calendar + Search

Combines `bgm_parser.py` (search) and `bgm_calendar.py` (calendar).

```rust
pub struct CalendarItem {
    pub name: String,       // Japanese title
    pub name_cn: String,    // Chinese title
    pub air_weekday: i32,   // 0=Mon, ..., 6=Sun
}

/// Fetch the current season calendar from bangumi.tv API.
pub async fn fetch_bgm_calendar(client: &NetworkClient) -> Result<Vec<CalendarItem>, ParserError>;

/// Match a bangumi against calendar items by title (CN → JP → substring).
pub fn match_weekday(
    official_title: &str,
    title_raw: &str,
    calendar: &[CalendarItem],
) -> Option<i32>;

/// Search bangumi.tv API for a subject by title.
pub async fn bgm_search(client: &NetworkClient, title: &str) -> Result<Option<serde_json::Value>, ParserError>;
```

**Matching strategy** (same as Python):
1. Exact match on Chinese title (`name_cn == official_title`)
2. Exact match on Japanese title (`name == title_raw` or `name == official_title`)
3. Substring match on Chinese title (≥4 chars)
4. Substring match on Japanese title (≥4 chars)

### `src/title_parser.rs` — High-Level Orchestration

Replaces the Python `TitleParser` static-methods class with free functions (or a stateless struct for grouping).

```rust
/// Combined parsing: regex-based raw parse + TMDB enrichment + weekday matching.
pub async fn full_parse(
    client: &NetworkClient,
    raw_title: &str,
    config: &ParserConfig,       // TMDB key, filters, language, openai flag
    image_saver: &dyn ImageSaver,
) -> Result<ParsedBangumi, ParserError>;

/// Quick regex-only parse (no network). Falls back to TMDB if disabled.
pub fn quick_parse(raw_title: &str) -> Option<Episode>;
```

**`ParserConfig`** (subset of `Config` needed by parser):
```rust
pub struct ParserConfig {
    pub language: String,          // "zh", "en", "jp"
    pub filters: Vec<String>,      // rss_parser.filter
    pub tmdb_api_key: Option<String>,
    pub openai_enable: bool,       // mirrors settings.experimental_openai.enable
}
```

**`ParsedBangumi`** (output of full parse):
```rust
pub struct ParsedBangumi {
    pub official_title: String,
    pub title_raw: String,
    pub season: i32,
    pub season_raw: String,
    pub group_name: Option<String>,
    pub dpi: Option<String>,
    pub source: Option<String>,
    pub subtitle: Option<String>,
    pub eps_collect: bool,
    pub poster_link: Option<String>,
    pub year: Option<String>,
}
```

### `src/lib.rs` — Re-exports
```rust
pub mod error;
pub mod raw_parser;
pub mod torrent_parser;
pub mod offset_detector;
pub mod enricher;
pub mod title_parser;

pub use error::ParserError;
pub use raw_parser::{raw_parser, Episode};
pub use torrent_parser::{torrent_parser, subtitle_parser, ParsedFile, ParsedSubtitle, FileType};
pub use offset_detector::{detect_offset_mismatch, OffsetSuggestion};
pub use enricher::tmdb::{tmdb_search, TMDBInfo};
pub use enricher::mikan::{mikan_parse, ImageSaver};
pub use enricher::bgm::{fetch_bgm_calendar, match_weekday, bgm_search};
pub use title_parser::{full_parse, quick_parse, ParserConfig, ParsedBangumi};
```

## Cargo.toml Dependencies

```toml
[package]
name = "ab-parser"
version.workspace = true
edition.workspace = true

[dependencies]
ab-core = { path = "../ab-core" }
ab-network = { path = "../ab-network" }
regex.workspace = true
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
once_cell.workspace = true
lru.workspace = true                     # LRU cache for torrent + TMDB results
quick-xml.workspace = true                # XML parsing for bgm.tv API (if needed)
tokio.workspace = true
tracing.workspace = true
thiserror.workspace = true
```

Add `lru` to workspace dependencies (`Cargo.toml` root):
```toml
lru = "0.12"
```

## File-by-File Build Order (11 files)

| # | File | Lines | Description |
|---|------|-------|-------------|
| 1 | `ab-parser/Cargo.toml` | 20 | Workspace member |
| 2 | `ab-parser/src/error.rs` | 30 | ParserError enum |
| 3 | `ab-parser/src/torrent_parser.rs` | 130 | Regex filename parser + LRU cache |
| 4 | `ab-parser/src/raw_parser.rs` | 180 | Unstructured title → Episode (named entities) |
| 5 | `ab-parser/src/offset_detector.rs` | 120 | Mismatch detection logic |
| 6 | `ab-parser/src/enricher/mod.rs` | 5 | pub mod tmdb, mikan, bgm |
| 7 | `ab-parser/src/enricher/tmdb.rs` | 300 | TMDB API search + season data |
| 8 | `ab-parser/src/enricher/mikan.rs` | 50 | Mikan homepage scraper |
| 9 | `ab-parser/src/enricher/bgm.rs` | 80 | bgm.tv calendar + search |
| 10 | `ab-parser/src/title_parser.rs` | 100 | Orchestration (combines all parsers) |
| 11 | `ab-parser/src/lib.rs` | 15 | Re-exports |

## Test Plan

**Unit tests** (pure logic, no I/O):
- `torrent_parser`: all 5 regex rules match known torrent filenames
- `torrent_parser`: group extraction from `[Group] Title`
- `torrent_parser`: season extraction from `S01`/`Season 1`/`第1季`
- `torrent_parser`: subtitle language detection (zh/zh-tw)
- `torrent_parser`: LRU cache eviction at 512 entries
- `raw_parser`: TITLE_RE match on 10+ real anime titles
- `raw_parser`: name_process splits Chinese/English/Japanese correctly
- `raw_parser`: episode number extraction from various formats
- `offset_detector`: season mismatch detection
- `offset_detector`: virtual season detection from air date gaps
- `offset_detector`: returns None when no mismatch
- `bgm::match_weekday`: exact match → weekday
- `bgm::match_weekday`: substring match → weekday
- `bgm::match_weekday`: no match → None

**Integration tests** (use wiremock for TMDB/bgm.tv/Mikan APIs):
- `tmdb_search` returns TMDBInfo for mocked API response
- `tmdb_search` returns None for non-animation response
- `mikan_parse` extracts poster + title from mocked HTML
- `bgm_search` parses mocked bgm.tv API response
- `full_parse` integrates all steps end-to-end

## Acceptance Criteria

1. `cargo build -p ab-parser` compiles without errors
2. `cargo test -p ab-parser` passes all tests
3. `torrent_parser` replicates Python regex rules exactly for all 5 patterns
4. `raw_parser` parses 10+ known anime title formats without regression
5. `offset_detector` correctly suggests season/episode offsets from TMDB data
6. `tmdb_search` correctly searches, validates animation, and caches results (512 LRU)
7. `mikan_parse` extracts poster + title from Mikan HTML
8. `bgm::fetch_bgm_calendar` returns parsed calendar items
9. `quick_parse` works with zero network I/O
10. OpenAIParser is explicitly deferred (not included in this crate)
11. `ImageSaver` trait allows downstream crates to handle image storage

## Deferred: OpenAIParser

`analyser/openai.py` (151L) is NOT included in Stage 4. Rationale:
- Depends on external LLM API (openai crate or manual reqwest)
- Feature-gated (only used when `settings.experimental_openai.enable`)
- Can be implemented as a standalone crate (`ab-ai-parser`) or integrated into `ab-parser` in a later stage
- Core functionality (regex-based parsing) works without it
