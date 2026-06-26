# Stage 2: ab-database — Database Models + Repository + Schema Migrations

## Objective
Migrate all 5 Python database CRUD classes (Session-based, SQLModel ORM) and 9 schema migrations (SQLite) to a single async sqlx crate. Design repository structs that accept a `sqlx::Pool<Sqlite>` and return `Result<T, DbError>`.

## Dependencies
- **ab-core** (Stage 1) — no runtime deps needed; database uses its own `Pool<Sqlite>`
- **External**: `sqlx` (sqlite, runtime-tokio), `serde` + `serde_json`, `chrono`, `thiserror`, `once_cell`, `tokio`, `tracing`

## Python Source Files & Target Crate Layout

```
ab-database/
├── Cargo.toml
├── migrations/
│   ├── 20240101000001_create_rssitem.sql          # includes air_weekday + connection_status
│   ├── 20240101000002_create_bangumi.sql           # full table: official_title, year, archived, needs_review, etc.
│   ├── 20240101000003_create_torrent.sql           # includes qb_hash
│   ├── 20240101000004_create_user.sql
│   ├── 20240101000005_create_passkey.sql
│   ├── 20240101000006_create_schema_version.sql
│   └── 20240101000007_add_weekday_locked.sql       # ALTER for existing DB compat
├── src/
│   ├── lib.rs                                    # Crate root, re-exports
│   ├── connection.rs                             # Pool creation + migration runner
│   ├── models/
│   │   ├── mod.rs
│   │   ├── bangumi.rs                            # Bangumi, BangumiUpdate, Notification, Episode
│   │   ├── rss.rs                                # RSSItem, RSSUpdate
│   │   ├── torrent.rs                            # Torrent, TorrentUpdate, EpisodeFile, SubtitleFile
│   │   ├── user.rs                               # User, UserUpdate, UserLogin, Token, TokenData
│   │   └── passkey.rs                            # Passkey, PasskeyCreate*, PasskeyList, ...
│   └── repo/
│       ├── mod.rs
│       ├── bangumi.rs                            # BangumiDatabase (~32 methods)
│       ├── rss.rs                                # RSSDatabase (13 methods)
│       ├── torrent.rs                            # TorrentDatabase (14 methods)
│       ├── user.rs                               # UserDatabase (4 methods + password hasher trait)
│       └── passkey.rs                            # PasskeyDatabase (7 async methods)
```

> **Note**: The `models/` subdirectory parallels Python's `module/models/`; the `repo/` subdirectory parallels `module/database/`. `response.py` (`ResponseModel`, `APIResponse`) is an API-layer concern — it is NOT included in ab-database; the `auth_user` method returns a `Result` instead.

## Crate Design

### 7 Migration Files (replacing `combine.py:MIGRATIONS`)

Python's 9 migrations are incremental ALTER TABLE additions over an expanding schema. Since
we are building a fresh database, the initial CREATE TABLE statements embed every column
that exists in the current Python SQLModel definitions, including those originally added
by later migrations. Only one ALTER-TABLE migration is kept (v9: `weekday_locked`) for
documentation of the final step.

**Python migrations history** (from `combine.py:MIGRATIONS`, verifies we have every column):

| v | Python DDL | Status in fresh Rust DB |
|---|------------|------------------------|
| 1 | ALTER bangumi ADD air_weekday | Embedded in `create_bangumi.sql` |
| 2 | ALTER rssitem ADD connection_status, last_checked_at, last_error | Embedded in `create_rssitem.sql` |
| 3 | CREATE passkey | `create_passkey.sql` |
| 4 | ALTER bangumi ADD archived | Embedded in `create_bangumi.sql` |
| 5 | RENAME COLUMN offset→episode_offset, ADD season_offset, needs_review, needs_review_reason | Embedded in `create_bangumi.sql` |
| 6 | ALTER torrent ADD qb_hash, CREATE INDEX | Embedded in `create_torrent.sql` |
| 7 | ALTER bangumi ADD suggested_season_offset, suggested_episode_offset | Embedded in `create_bangumi.sql` |
| 8 | ALTER bangumi ADD title_aliases | Embedded in `create_bangumi.sql` |
| 9 | ALTER bangumi ADD weekday_locked | Kept as standalone migration |

Migration files for the Rust crate:

| File | Content |
|------|---------|
| `...01_create_rssitem.sql` | CREATE TABLE rssitem (id INTEGER PRIMARY KEY, name TEXT, url TEXT NOT NULL, aggregate INTEGER DEFAULT 0, parser TEXT DEFAULT 'mikan', enabled INTEGER DEFAULT 1, connection_status TEXT, last_checked_at TEXT, last_error TEXT) + CREATE INDEX ix_rssitem_url ON rssitem(url) — Python 用 `index=True` 非 `unique=True` (rss.py:9), 允许相同 URL 多条记录 |
| `...02_create_bangumi.sql` | CREATE TABLE bangumi (all 28 fields: id, official_title, year, title_raw, season, season_raw, group_name, dpi, source, subtitle, eps_collect, episode_offset, season_offset, filter, rss_link, poster_link, added, rule_name, save_path, deleted, archived, air_weekday, weekday_locked, needs_review, needs_review_reason, suggested_season_offset, suggested_episode_offset, title_aliases) |
| `...03_create_torrent.sql` | CREATE TABLE torrent (id INTEGER PRIMARY KEY, refer_id INTEGER REFERENCES bangumi(id), rss_id INTEGER REFERENCES rssitem(id), name TEXT, url TEXT NOT NULL, homepage TEXT, downloaded INTEGER DEFAULT 0, qb_hash TEXT) + CREATE INDEX ix_torrent_url ON torrent(url) + CREATE INDEX ix_torrent_qb_hash ON torrent(qb_hash) — Python `Torrent.bangumi_id` 列别名为 `refer_id` (torrent.py:9), url 用 `index=True` 非 `unique=True` (torrent.py:13) |
| `...04_create_user.sql` | CREATE TABLE user (id INTEGER PRIMARY KEY, username TEXT NOT NULL, password TEXT NOT NULL) — Python `User.username` 无 SQL UNIQUE (user.py:9-11), 仅 Pydantic 级别约束 |
| `...05_create_passkey.sql` | CREATE TABLE passkey (id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES user(id), name VARCHAR(64) NOT NULL, credential_id VARCHAR NOT NULL UNIQUE, public_key VARCHAR NOT NULL, sign_count INTEGER DEFAULT 0, aaguid VARCHAR, transports VARCHAR, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, last_used_at TIMESTAMP, backup_eligible INTEGER DEFAULT 0, backup_state INTEGER DEFAULT 0) + CREATE INDEX ix_passkey_user_id + CREATE UNIQUE INDEX ix_passkey_credential_id |
| `...06_create_schema_version.sql` | CREATE TABLE IF NOT EXISTS schema_version (id INTEGER PRIMARY KEY, version INTEGER NOT NULL) |
| `...07_add_weekday_locked.sql` | ALTER TABLE bangumi ADD COLUMN weekday_locked BOOLEAN DEFAULT 0 |

### `src/connection.rs` — Pool + Migration Runner
- `pub async fn create_pool(database_url: &str) -> Result<Pool<Sqlite>, sqlx::Error>`
- Enable `PRAGMA journal_mode=WAL`, `PRAGMA foreign_keys=ON` after connect
- Run sqlx migrations from `./migrations/` directory
- Track schema version via sqlx's built-in `_sqlx_migrations` table (replaces Python's custom `schema_version` table)

```rust
// Pseudocode
pub async fn create_pool(database_url: &str) -> Result<Pool<Sqlite>, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    // Enable WAL + foreign keys
    sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;
    // Run automatic migrations
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
```

- Remove `Database` class that extends Session (Python `combine.py:Database`): no need to compose sub-repos in Rust. Each repo takes a `&Pool<Sqlite>`.
- Remove `_fill_null_with_defaults()`: not needed on fresh DB — models store booleans as `bool` (sqlx maps INTEGER 0/1) and Option<T> for nullable fields.
- Remove `migrate()` (destructive re-insert): obsolete; not needed for greenfield migration.

### `src/models/` — Struct Definitions

Each model mirrors the Python SQLModel but uses Rust-native types:

- **`bangumi.rs`**: `struct Bangumi { id, official_title, year, title_raw, season, season_raw, group_name, dpi, source, subtitle, eps_collect, episode_offset, season_offset, filter, rss_link, poster_link, added, rule_name, save_path, deleted, archived, air_weekday, weekday_locked, needs_review, needs_review_reason, suggested_season_offset, suggested_episode_offset, title_aliases }`
  - Python 实际 28 个字段 (bangumi.py:8-54)
  - `title_aliases: Option<String>` — JSON list stored as TEXT; provide `fn aliases(&self) -> Vec<String>` and `fn set_aliases(&mut self, aliases: Vec<String>)` helpers
  - `BangumiUpdate` struct: Python 中不全是 `Option<T>` — 部分字段有默认值但非 Optional (bangumi.py:57-92):
    - `official_title: String`, `title_raw: String`, `season: i32`, `filter: String`, `rss_link: String`, `deleted: bool`, `archived: bool`, `weekday_locked: bool`, `needs_review: bool` — 均有默认值, 不是 Option
    - `season_offset: i32`, `episode_offset: i32` — 有默认值 0, 不是 Option
    - 其他字段 `Option<T>`
    - 前台通过 `exclude_unset=True` 区分用户输入和默认值 (Rust 中对应 `#[serde(default)]` + 检测输入中哪些字段存在)
  - `Notification` struct (for notification dispatch — also used by Stage 7)
  - `Episode` — `#[derive(Clone, Debug)]` struct
  - Derive `sqlx::FromRow` on `Bangumi`, `RSSItem`, `Torrent`, `User`, `Passkey`

- **`rss.rs`**: `struct RSSItem { id, name, url, aggregate, parser, enabled, connection_status, last_checked_at, last_error }`
  - `RSSUpdate` struct (all fields `Option<T>`)

- **`torrent.rs`**: `struct Torrent { id, bangumi_id, rss_id, name, url, homepage, downloaded, qb_hash }`
  - **注意**: Python `Torrent.bangumi_id` 的 SQL 列别名为 `refer_id` (torrent.py:9) — Rust 中字段名保持 `bangumi_id`，但 SQL CREATE TABLE 使用 `refer_id`，通过 `#[sqlx(rename = "refer_id")]` 或查询时指定列名
  - `TorrentUpdate` struct: Python 只有一个字段 (torrent.py:18-19): `downloaded: bool` (非 Option)
  - `EpisodeFile`, `SubtitleFile` — plain structs (not DB models)

- **`user.rs`**: `struct User { id, username, password }`
  - `UserUpdate` struct
  - `UserLogin`, `Token`, `TokenData` — request/response DTOs (used by API layer)

- **`passkey.rs`**: `struct Passkey { id, user_id, name, credential_id, public_key, sign_count, aaguid, transports, created_at, last_used_at, backup_eligible, backup_state }`
  - DTOs: `PasskeyCreate`, `PasskeyList`, `PasskeyDelete`, `PasskeyAuthStart`, `PasskeyAuthFinish`
  - Note: `aaguid`, `transports`, `last_used_at` are `Option<String>` (TEXT stored)

All fields use sqlx-compatible types:
- `INTEGER` → `i32` or `i64`
- `BOOLEAN` → `bool` (sqlx maps 0/1)
- `TEXT` → `String` or `Option<String>`
- `TIMESTAMP` → `Option<chrono::NaiveDateTime>` (or `String` for simplicity)

### `src/repo/` — Repository Traits + Implementations

#### **`rss.rs`** — `RssRepo` (13 methods)
```
impl RssRepo {
    pub fn new(pool: Pool<Sqlite>) -> Self
    pub async fn add(&self, data: &RSSItem) -> Result<bool, DbError>
    pub async fn add_all(&self, data: &[RSSItem]) -> Result<(), DbError>
    pub async fn update(&self, id: i32, data: &RSSUpdate) -> Result<bool, DbError>
    pub async fn enable(&self, id: i32) -> Result<bool, DbError>
    pub async fn enable_batch(&self, ids: &[i32]) -> Result<(), DbError>
    pub async fn disable(&self, id: i32) -> Result<bool, DbError>
    pub async fn disable_batch(&self, ids: &[i32]) -> Result<(), DbError>
    pub async fn search_id(&self, id: i32) -> Result<Option<RSSItem>, DbError>
    pub async fn search_all(&self) -> Result<Vec<RSSItem>, DbError>
    pub async fn search_active(&self) -> Result<Vec<RSSItem>, DbError>
    pub async fn search_aggregate(&self) -> Result<Vec<RSSItem>, DbError>
    pub async fn delete(&self, id: i32) -> Result<bool, DbError>
    pub async fn delete_all(&self) -> Result<(), DbError>
}
```

SQL approach:
- `add`: Python 用 SELECT-then-INSERT 模式做幂等 (rss.py:14-26):
  1. `SELECT id FROM rssitem WHERE url = ?`
  2. 如果存在 → return false (表示重复)
  3. 否则 → `INSERT INTO rssitem ...` (无 RETURNING, sqlite 自动生成 id)
  Rust 可用 `INSERT OR IGNORE INTO rssitem ...` + 再 SELECT 获取 id, 或直接模仿 Python 的两步模式
- `add_all`: `INSERT OR IGNORE ...` loop or batched
- `update`: `UPDATE rssitem SET name = ?1, url = ?2, ... WHERE id = ?3`
- `search_active`: `SELECT * FROM rssitem WHERE enabled = 1`
- `search_aggregate`: `SELECT * FROM rssitem WHERE aggregate = 1 AND enabled = 1`

#### **`torrent.rs`** — `TorrentRepo`
```
impl TorrentRepo {
    pub fn new(pool: Pool<Sqlite>) -> Self
    pub async fn add(&self, data: &Torrent) -> Result<(), DbError>
    pub async fn add_all(&self, datas: &[Torrent]) -> Result<(), DbError>
    pub async fn update(&self, data: &Torrent) -> Result<(), DbError>
    pub async fn update_all(&self, datas: &[Torrent]) -> Result<(), DbError>
    pub async fn update_one_user(&self, data: &Torrent) -> Result<(), DbError>
    pub async fn search(&self, id: i32) -> Result<Option<Torrent>, DbError>
    pub async fn search_all(&self) -> Result<Vec<Torrent>, DbError>
    pub async fn search_rss(&self, rss_id: i32) -> Result<Vec<Torrent>, DbError>
    pub async fn check_new(&self, torrents: &[Torrent]) -> Result<Vec<Torrent>, DbError>
    pub async fn search_by_qb_hash(&self, qb_hash: &str) -> Result<Option<Torrent>, DbError>
    pub async fn search_by_qb_hashes(&self, qb_hashes: &[String]) -> Result<Vec<Torrent>, DbError>
    pub async fn delete_by_bangumi_id(&self, bangumi_id: i32) -> Result<i32, DbError>
    pub async fn search_by_url(&self, url: &str) -> Result<Option<Torrent>, DbError>
    pub async fn update_qb_hash(&self, torrent_id: i32, qb_hash: &str) -> Result<bool, DbError>
}
```
**Total: 14 methods** (matches Python `TorrentDatabase`).

SQL approach:
- `add`: Python 用两步 (torrent.py:14-17): `SELECT url FROM torrent WHERE url = ?` → 不存在则 INSERT. Rust 同模式或用 `INSERT OR IGNORE`.
- `check_new`: For each input torrent, `SELECT url FROM torrent WHERE url = ?` — collect non-matching. The Python version does a single WHERE url IN (...) batch query; replicate that.
- `delete_by_bangumi_id`: `DELETE FROM torrent WHERE bangumi_id = ?1` — 注意 bangumi_id 列的 SQL 名是 `refer_id`，需要用 `refer_id = ?1`.

#### **`user.rs`** — `UserRepo`
- Accepts a `PasswordHasher` trait (or `Box<dyn PasswordHasher>`) for hash/verify operations, keeping ab-database independent of ab-security.

```rust
pub trait PasswordHasher: Send + Sync {
    fn hash_password(&self, password: &str) -> String;
    fn verify_password(&self, password: &str, hash: &str) -> bool;
}

pub struct UserRepo {
    pool: Pool<Sqlite>,
    hasher: Box<dyn PasswordHasher>,
}
```

- `pub fn new(pool: Pool<Sqlite>, hasher: Box<dyn PasswordHasher>) -> Self`
- `pub async fn get_user(&self, username: &str) -> Result<User, DbError>`
- `pub async fn auth_user(&self, user: &User) -> Result<bool, DbError>`
  - Python (user.py:25) 传入 `User` 对象, 内部校验密码后返回 `ResponseModel` (含 status/status_code/msg_en/msg_zh)
  - Rust 仓库层返回 `Result<bool>`, API 层构造 ResponseModel
- `pub async fn update_user(&self, username: &str, update: &UserUpdate) -> Result<User, DbError>`
- `pub async fn add_default_user(&self) -> Result<(), DbError>`

Default `PasswordHasher` implementation (usable for default user creation; replaced by ab-security in Stage 9):

> Use `sha2` crate (SHA-256) for a simple hash that avoids the panic but is **not** production-grade.
> Stage 9 (ab-security) will provide an Argon2-based `PasswordHasher` implementation.

```rust
use sha2::{Sha256, Digest};

pub struct DefaultHasher;
impl PasswordHasher for DefaultHasher {
    fn hash_password(&self, password: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        format!("{:x}", hasher.finalize())
    }
    fn verify_password(&self, password: &str, hash: &str) -> bool {
        self.hash_password(password) == hash
    }
}
```

Add `sha2` to `ab-database/Cargo.toml`:
```toml
sha2 = "0.10"
```

#### **`passkey.rs`** — `PasskeyRepo` (all async)
```
impl PasskeyRepo {
    pub fn new(pool: Pool<Sqlite>) -> Self
    pub async fn create_passkey(&self, passkey: &Passkey) -> Result<Passkey, DbError>
    pub async fn get_passkey_by_credential_id(&self, credential_id: &str) -> Result<Option<Passkey>, DbError>
    pub async fn get_passkeys_by_user_id(&self, user_id: i32) -> Result<Vec<Passkey>, DbError>
    pub async fn get_passkey_by_id(&self, passkey_id: i32, user_id: i32) -> Result<Passkey, DbError>
    pub async fn update_passkey_usage(&self, passkey: &Passkey, new_sign_count: i32) -> Result<(), DbError>
    pub async fn delete_passkey(&self, passkey_id: i32, user_id: i32) -> Result<bool, DbError>
}
```

`get_passkey_by_id` returns `Err(DbError::NotFound(...))` on missing (matching Python's HTTPException(404) — error mapping done by the caller).

#### **`bangumi.rs`** — `BangumiRepo` (largest, ~32 public methods)

```rust
pub enum UpdateData {
    Full(Bangumi),
    Partial(i32, BangumiUpdate),
}

impl BangumiRepo {
    pub fn new(pool: Pool<Sqlite>) -> Self

    // Insert / Update
    pub async fn add(&self, data: &Bangumi) -> Result<bool, DbError>     // SELECT-then-INSERT 幂等
    pub async fn add_all(&self, datas: &[Bangumi]) -> Result<u32, DbError>
    pub async fn update(&self, data: UpdateData) -> Result<bool, DbError>   // Python 单方法 isinstance 派发
    pub async fn update_all(&self, datas: &[Bangumi]) -> Result<(), DbError>
    pub async fn update_rss(&self, title_raw: &str, rss_set: &str) -> Result<(), DbError>
    pub async fn update_poster(&self, title_raw: &str, poster_link: &str) -> Result<(), DbError>

    // Delete
    pub async fn delete_one(&self, id: i32) -> Result<(), DbError>
    pub async fn delete_all(&self) -> Result<(), DbError>
    pub async fn disable_rule(&self, id: i32) -> Result<(), DbError>

    // Search (single)
    pub async fn search_id(&self, id: i32) -> Result<Option<Bangumi>, DbError>
    pub async fn search_official_title(&self, official_title: &str) -> Result<Option<Bangumi>, DbError>
    pub async fn search_ids(&self, ids: &[i32]) -> Result<Vec<Bangumi>, DbError>

    // Search (list)
    pub async fn search_all(&self) -> Result<Vec<Bangumi>, DbError>
    pub async fn not_complete(&self) -> Result<Vec<Bangumi>, DbError>
    pub async fn not_added(&self) -> Result<Vec<Bangumi>, DbError>
    pub async fn search_rss(&self, rss_link: &str) -> Result<Vec<Bangumi>, DbError>
    pub async fn get_needs_review(&self) -> Result<Vec<Bangumi>, DbError>
    pub async fn get_active_for_scan(&self) -> Result<Vec<Bangumi>, DbError>

    // Matching
    pub async fn match_poster(&self, bangumi_name: &str) -> Result<String, DbError>
    pub async fn match_list(&self, torrent_list: &[Torrent], rss_link: &str) -> Result<Vec<Torrent>, DbError>
    pub async fn match_torrent(&self, torrent_name: &str) -> Result<Option<Bangumi>, DbError>
    pub async fn match_by_save_path(&self, save_path: &str) -> Result<Option<Bangumi>, DbError>

    // Duplicate detection
    pub async fn find_semantic_duplicate(&self, data: &Bangumi) -> Result<Option<Bangumi>, DbError>
    pub async fn add_title_alias(&self, bangumi_id: i32, new_title_raw: &str) -> Result<bool, DbError>
    pub fn get_all_title_patterns(&self, bangumi: &Bangumi) -> Vec<String>

    // Archive / Review
    pub async fn archive_one(&self, id: i32) -> Result<bool, DbError>
    pub async fn unarchive_one(&self, id: i32) -> Result<bool, DbError>
    pub async fn set_needs_review(&self, id: i32, reason: &str, ...) -> Result<bool, DbError>
    pub async fn clear_needs_review(&self, id: i32) -> Result<bool, DbError>
    pub async fn set_weekday(&self, id: i32, weekday: Option<i32>) -> Result<bool, DbError>

    // Cache helpers
    fn get_cached_bangumi(&self) -> Option<Vec<Bangumi>>
    fn set_cached_bangumi(&self, bangumi: Vec<Bangumi>)
    fn invalidate_cache(&self)
}
```

**Cache pattern** (replacing Python module-level globals):
- Use `Mutex<Option<(Vec<Bangumi>, Instant)>>` stored as a field on `BangumiRepo` (not module-level).
- `search_all()` checks cache first; if expired or missing, queries DB, stores in cache.
- `_invalidate_bangumi_cache()` → `self.invalidate_cache()`, called on any write operation.
- Cache TTL: 300 seconds (5 minutes — same as Python's `_BANGUMI_CACHE_TTL = 300.0`).

**Title aliases** (replacing `_get_aliases_list` / `_set_aliases_list` helper functions):
- Implement directly on struct:
```rust
impl Bangumi {
    pub fn aliases(&self) -> Vec<String> {
        self.title_aliases.as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
    pub fn set_aliases(&mut self, aliases: Vec<String>) {
        self.title_aliases = Some(serde_json::to_string(&aliases).unwrap_or_default());
    }
}
```

**`match_list` algorithm**: Replicate Python's regex-based title matching:
1. `search_all()` to get all bangumi (from cache)
2. Build `HashMap<String, Bangumi>` index (main title + all aliases)
3. Sort keys by length descending, build compiled regex alternation
4. For each torrent, regex search → if match found, append rss_link + mark `added = false`
5. Different from Python: we don't call `self.session.merge()` because sqlx doesn't have a merge concept; instead use `UPDATE bangumi SET rss_link = ... WHERE id = ?`
6. Return unmatched torrents

**`update` flexibility**: Python's `update` accepts both `Bangumi` and `BangumiUpdate` with optional `_id` via isinstance dispatch (bangumi.py:300-316). Rust 提供单一方法来匹配:

```rust
pub enum UpdateData {
    Full(Bangumi),
    Partial(i32, BangumiUpdate),
}

impl BangumiRepo {
    pub async fn update(&self, data: UpdateData) -> Result<bool, DbError>;
}
```

如果不想用 enum, 也可以用两个分开方法但调用方需在外部做类型判断 — 这与 Python 行为一致 (调用者已知数据类型).

### `src/lib.rs` — Re-exports
```
pub mod connection;
pub mod models;
pub mod repo;

pub use connection::create_pool;
pub use repo::bangumi::BangumiRepo;
pub use repo::rss::RssRepo;
pub use repo::torrent::TorrentRepo;
pub use repo::user::{UserRepo, PasswordHasher};
pub use repo::passkey::PasskeyRepo;
```

### Error Type
```rust
#[derive(thiserror::Error, Debug)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("duplicate entry")]
    Duplicate,
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}
```

## File-by-File Build Order (32 files)

| # | File | Lines (est.) | Description |
|---|------|-------------|-------------|
| 1 | `ab-database/Cargo.toml` | 30 | Workspace member, deps on sqlx, serde, chrono, thiserror, once_cell, ab-core |
| 2 | `ab-database/migrations/20240101000001_create_rssitem.sql` | 18 | CREATE TABLE rssitem (final schema) |
| 3 | `ab-database/migrations/20240101000002_create_bangumi.sql` | 35 | CREATE TABLE bangumi (final schema) |
| 4 | `ab-database/migrations/20240101000003_create_torrent.sql` | 12 | CREATE TABLE torrent (final schema) |
| 5 | `ab-database/migrations/20240101000004_create_user.sql` | 6 | CREATE TABLE user |
| 6 | `ab-database/migrations/20240101000005_create_passkey.sql` | 18 | CREATE TABLE passkey + indexes |
| 7 | `ab-database/migrations/20240101000006_create_schema_version.sql` | 4 | CREATE TABLE schema_version |
| 8 | `ab-database/migrations/20240101000007_add_weekday_locked.sql` | 3 | ALTER TABLE bangumi ADD COLUMN weekday_locked |
| 9 | `ab-database/src/lib.rs` | 20 | Crate root, pub mod + re-exports |
| 10 | `ab-database/src/connection.rs` | 50 | create_pool(), PRAGMA, migration runner |
| 11 | `ab-database/src/models/mod.rs` | 5 | pub mod declarations |
| 12 | `ab-database/src/models/bangumi.rs` | 130 | Bangumi + BangumiUpdate + Notification + helpers (aliases) |
| 13 | `ab-database/src/models/rss.rs` | 30 | RSSItem + RSSUpdate |
| 14 | `ab-database/src/models/torrent.rs` | 45 | Torrent + TorrentUpdate + EpisodeFile + SubtitleFile |
| 15 | `ab-database/src/models/user.rs` | 50 | User + UserUpdate + UserLogin + Token + TokenData |
| 16 | `ab-database/src/models/passkey.rs` | 80 | Passkey struct + 6 DTOs |
| 17 | `ab-database/src/repo/mod.rs` | 5 | pub mod declarations |
| 18 | `ab-database/src/repo/rss.rs` | 120 | RssRepo (13 methods) |
| 19 | `ab-database/src/repo/torrent.rs` | 150 | TorrentRepo (14 methods, incl. update_one_user) |
| 20 | `ab-database/src/repo/user.rs` | 120 | UserRepo + PasswordHasher trait (4 methods) |
| 21 | `ab-database/src/repo/passkey.rs` | 120 | PasskeyRepo (7 async methods) |
| 22 | `ab-database/src/repo/bangumi.rs` | 600 | BangumiRepo (~32 methods, cache, matching) |

**Total**: ~1,700 lines of Rust (22 files, including 7 sql migrations, 14 Rust source files).

## Dependencies in Cargo.toml

```toml
[package]
name = "ab-database"
version.workspace = true
edition.workspace = true

[dependencies]
ab-core = { path = "../ab-core" }
sqlx = { workspace = true, features = ["sqlite", "runtime-tokio-rustls", "chrono", "migrate"] }
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
chrono.workspace = true
thiserror.workspace = true
once_cell.workspace = true
tokio = { workspace = true, features = ["rt"] }
tracing.workspace = true
sha2 = "0.10"                              # Default password hashing (replaced by ab-security Stage 9)
```

## Test Plan

**Integration tests** (`tests/integration/`):
- Create in-memory SQLite pool for each test
- Run migrations against test pool
- Test each repo's main flows (insert, read, update, delete, list)
- Specifically test:
  - Bangumi duplicate detection (`_is_duplicate`, `find_semantic_duplicate`, `add_title_alias`)
  - RSS `add_all` batch dedup
  - Torrent `check_new` with mixed new/existing
  - User `add_default_user` (only when table is empty)
  - User `auth_user` (with mock PasswordHasher)
  - Passkey full CRUD
  - Bangumi `match_list` regex matching
  - Bangumi cache invalidation on write

## Acceptance Criteria

1. `cargo build -p ab-database` compiles without errors
2. `cargo test -p ab-database` passes all tests
3. `create_pool()` correctly runs all 7 migrations on a fresh SQLite file
4. `PRAGMA foreign_keys=ON` is set on pool creation
5. All 5 repository structs support full CRUD matching Python behaviour
6. `BangumiRepo.search_all()` caches and invalidates correctly (TTL 300s)
7. `BangumiRepo.match_list()` replicates Python's regex+alias matching
8. `UserRepo` works with both real and default `PasswordHasher`
9. `BangumiRepo.find_semantic_duplicate()` first filters by `official_title`, then matches on (dpi, subtitle, source, group_name similarity)
10. `BangumiRepo.add()` merges semantic duplicates as aliases (not new entries)
