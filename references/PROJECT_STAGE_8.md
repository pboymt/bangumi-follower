# Stage 8: ab-security + ab-api — JWT/WebAuthn Authentication + Axum REST API

## Objective
Migrate 4 Python security files (`security/jwt.py`, `webauthn.py`, `auth_strategy.py`, `api.py`) and 12 API route files (`api/*.py`) to two async crates. `ab-security` handles JWT tokens, WebAuthn passkey authentication, and auth middleware. `ab-api` provides the Axum REST API layer with 11 route groups under `/v1`.

## Dependencies

### ab-security
- **ab-core** (Stage 1) — `Config` (security settings, login whitelist, login_tokens)
- **ab-database** (Stage 2) — `UserRepo`, `PasskeyRepo` + models (`User`, `Passkey`, passkey DTOs)
- **External**: `jsonwebtoken` (HS256), `argon2` (password hashing), `webauthn-rs` (WebAuthn), `serde` + `serde_json`, `base64`, `tracing`, `thiserror`, `once_cell`

### ab-api
- **ab-core** (Stage 1) — `Config` (all settings), `VERSION`, `LOG_PATH`
- **ab-database** (Stage 2) — `BangumiRepo`, `RssRepo`, `TorrentRepo`, `UserRepo`, `PasskeyRepo`
- **ab-security** (Stage 8) — JWT/WebAuthn auth, `get_current_user` middleware
- **ab-downloader** (Stage 5) — `DownloadClient`, `TorrentInfo`
- **ab-rss** (Stage 6) — `RSSEngine`, `RSSAnalyser`, `AddTorrent`
- **ab-parser** (Stage 4) — `tmdb_search`, `TMDBInfo`, `detect_offset_mismatch`, `OffsetSuggestion`
- **ab-manager** (Stage 7) — `TorrentManager`, `SeasonCollector`, `SearchSeason`
- **ab-searcher** (Stage 8+) — Required for `/api/search` SSE endpoint; trait provided via ab-manager's `SearchSeason`
- **ab-notification** (Stage 8+) — Required for `/api/notification/test`; deferred to trait
- **External**: `axum`, `tokio`, `tower-http` (CORS), `serde`, `serde_json`, `tracing`, `thiserror`, `chrono`

## Python Sources → Crate Layout

```
ab-security/
├── Cargo.toml
└── src/
    ├── lib.rs                    # Re-exports: JwtService, WebAuthnService, AuthStrategy
    ├── error.rs                  # SecurityError enum
    ├── jwt.rs                    # JWT create/decode/verify + password hashing
    ├── webauthn.rs               # WebAuthnService (register/authenticate)
    ├── auth_strategy.rs          # AuthStrategy trait + PasswordStrategy + PasskeyStrategy
    └── middleware.rs             # get_current_user Axum middleware + IP whitelist check

ab-api/
├── Cargo.toml
└── src/
    ├── lib.rs                    # Crate root
    ├── router.rs                 # Compose all sub-routers under /v1 prefix
    ├── error.rs                  # ApiError enum
    ├── response.rs               # ApiResponse helper + u_response equivalent
    └── routes/
        ├── mod.rs                # pub mod + re-exports
        ├── auth.rs               # POST /login, GET /refresh, GET /logout, POST /update
        ├── bangumi.rs            # CRUD + poster/calendar/metadata/offset/weekday
        ├── config.rs             # GET /get, PATCH /update (with sensitive field masking)
        ├── downloader.rs         # GET /torrents, POST pause/resume/delete/tag
        ├── log.rs                # GET /log, GET /log/clear
        ├── notification.rs       # POST /test, POST /test-config
        ├── passkey.rs            # WebAuthn register/auth/list/delete
        ├── program.rs            # GET start/stop/restart/status/shutdown
        ├── rss.rs                # CRUD + refresh + analysis + collect
        ├── search.rs             # GET /bangumi (SSE)
        └── setup.rs              # GET /status, POST test-*/complete
```

### Source Mapping

| Python file | Rust target | Lines | Crate |
|-------------|-------------|-------|-------|
| `security/jwt.py` | `jwt.rs` | ~70 | ab-security |
| `security/webauthn.py` | `webauthn.rs` | ~370 | ab-security |
| `security/auth_strategy.py` | `auth_strategy.rs` | ~140 | ab-security |
| `security/api.py` | `middleware.rs` | ~100 | ab-security |
| (new) | `error.rs` | ~20 | ab-security |
| (new) | `lib.rs` | ~15 | ab-security |
| `api/auth.py` | `routes/auth.rs` | ~90 | ab-api |
| `api/bangumi.py` | `routes/bangumi.rs` | ~380 | ab-api |
| `api/config.py` | `routes/config.rs` | ~90 | ab-api |
| `api/downloader.py` | `routes/downloader.rs` | ~150 | ab-api |
| `api/log.py` | `routes/log.rs` | ~50 | ab-api |
| `api/notification.py` | `routes/notification.rs` | ~130 | ab-api |
| `api/passkey.py` | `routes/passkey.rs` | ~300 | ab-api |
| `api/program.py` | `routes/program.rs` | ~110 | ab-api |
| `api/rss.py` | `routes/rss.rs` | ~200 | ab-api |
| `api/search.py` | `routes/search.rs` | ~60 | ab-api |
| `api/setup.py` | `routes/setup.rs` | ~350 | ab-api |
| `api/response.py` | `response.rs` | ~15 | ab-api |
| `api/__init__.py` | `router.rs` + `lib.rs` | ~30 | ab-api |
| (new) | `error.rs` | ~30 | ab-api |
| (new) | `routes/mod.rs` | ~15 | ab-api |

**Total**: ~2,460 lines Rust, 19 files (6 for ab-security, 13 for ab-api).

## Crate Design — ab-security

### `src/error.rs` — `SecurityError`
```rust
#[derive(thiserror::Error, Debug)]
pub enum SecurityError {
    #[error("jwt error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),
    #[error("authentication failed")]
    AuthFailed,
    #[error("token expired")]
    TokenExpired,
    #[error("password hash error: {0}")]
    PasswordHash(String),
    #[error("webauthn error: {0}")]
    WebAuthn(String),
    #[error("database error: {0}")]
    Database(#[from] ab_database::DbError),
}
```

### `src/jwt.rs` — JWT + Password Hashing

```rust
/// HS256 JWT service.
pub struct JwtService;

impl JwtService {
    /// Load or create the JWT secret from config/.jwt_secret (32-byte hex).
    pub fn load_secret() -> String;

    /// Create a JWT access token.
    pub fn create_access_token(
        data: &HashMap<String, String>, expires_delta: Option<Duration>,
    ) -> Result<String, SecurityError>;

    /// Decode a JWT token (returns payload, ignores expiration).
    pub fn decode_token(token: &str) -> Result<HashMap<String, Value>, SecurityError>;

    /// Verify a JWT token (checks expiration).
    pub fn verify_token(token: &str) -> Result<HashMap<String, Value>, SecurityError>;
}

/// Password hashing (Argon2 — replaces Python's bcrypt).
pub fn hash_password(password: &str) -> Result<String, SecurityError>;
pub fn verify_password(password: &str, hash: &str) -> Result<bool, SecurityError>;
```

Python differences:
- `bcrypt` → `argon2` (per PROJECT.md decision)
- `HS256` same, `jsonwebtoken` crate instead of `python-jose`
- JWT payload as `HashMap<String, Value>` instead of dict
- Secret stored in `config/.jwt_secret` (same path as Python)

**`_load_or_create_secret`** — Mirrors Python exactly:
```rust
const SECRET_PATH: &str = "config/.jwt_secret";

pub fn load_secret() -> String {
    let path = Path::new(SECRET_PATH);
    if path.exists() {
        return std::fs::read_to_string(path).unwrap_or_default().trim().to_string();
    }
    let secret = hex::encode(rand::thread_rng().gen::<[u8; 32]>());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(path, &secret).ok();
    secret
}
```

### `src/webauthn.rs` — WebAuthnService

Uses the `webauthn-rs` crate. Challenge store replaces Python's `_challenges` dict.

```rust
const CHALLENGE_TTL: Duration = Duration::from_secs(300);
const CHALLENGE_MAX: usize = 100;

pub struct WebAuthnService {
    rp_id: String,
    rp_name: String,
    origin: String,
    challenges: Arc<Mutex<HashMap<String, (Vec<u8>, Instant, String)>>>,
}

impl WebAuthnService {
    pub fn new(rp_id: &str, rp_name: &str, origin: &str) -> Self;

    // Registration
    pub fn generate_registration_options(
        &self, username: &str, user_id: i32, existing_passkeys: &[Passkey],
    ) -> Result<Value, SecurityError>;
    pub fn verify_registration(
        &self, username: &str, credential: &Value, device_name: &str,
    ) -> Result<PasskeyCreate, SecurityError>;

    // Authentication
    pub fn generate_authentication_options(
        &self, username: &str, passkeys: &[Passkey],
    ) -> Result<Value, SecurityError>;
    pub fn generate_discoverable_authentication_options(&self) -> Result<Value, SecurityError>;
    pub fn verify_authentication(
        &self, username: &str, credential: &Value, passkey: &Passkey,
    ) -> Result<i32, SecurityError>;
    pub fn verify_discoverable_authentication(
        &self, credential: &Value, passkey: &Passkey,
    ) -> Result<i32, SecurityError>;

    // Helpers
    fn cleanup_expired(&self);
    fn store_challenge(&self, logical_key: &str, challenge: Vec<u8>);
    fn pop_challenge_by_key(&self, logical_key: &str) -> Option<Vec<u8>>;
    pub fn base64url_encode(&self, data: &[u8]) -> String;
    pub fn base64url_decode(&self, data: &str) -> Result<Vec<u8>, SecurityError>;
}
```

**Global service cache** (replacing Python's `_webauthn_services`):
```rust
static WEBAUTHN_SERVICES: Lazy<Mutex<HashMap<String, Arc<WebAuthnService>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn get_webauthn_service(rp_id: &str, rp_name: &str, origin: &str) -> Arc<WebAuthnService>;
```

The `webauthn-rs` crate provides `Passkey` and `Credential` types. Python's `py_webauthn` library is replaced by `webauthn-rs` — the Rust-native WebAuthn implementation with full CTAP2/FIDO2 support.

### `src/auth_strategy.rs` — AuthStrategy Trait

```rust
#[async_trait]
pub trait AuthStrategy: Send + Sync {
    async fn authenticate(
        &self, username: Option<&str>, credential: &Value,
    ) -> Result<String, SecurityError>;  // Returns username on success
}

pub struct PasswordStrategy {
    user_repo: UserRepo,
}

impl PasswordStrategy {
    pub fn new(pool: Pool<Sqlite>) -> Self;
}

#[async_trait]
impl AuthStrategy for PasswordStrategy {
    async fn authenticate(
        &self, username: Option<&str>, credential: &Value,
    ) -> Result<String, SecurityError> {
        let username = username.ok_or(SecurityError::AuthFailed)?;
        let password = credential.get("password")
            .and_then(|v| v.as_str())
            .ok_or(SecurityError::AuthFailed)?;
        if self.user_repo.auth_user(username, password).await? {
            Ok(username.to_string())
        } else {
            Err(SecurityError::AuthFailed)
        }
    }
}

pub struct PasskeyStrategy {
    webauthn: Arc<WebAuthnService>,
    passkey_repo: PasskeyRepo,
    user_repo: UserRepo,
}

impl PasskeyStrategy {
    pub fn new(
        pool: Pool<Sqlite>, webauthn: Arc<WebAuthnService>,
    ) -> Self;
}

#[async_trait]
impl AuthStrategy for PasskeyStrategy {
    async fn authenticate(
        &self, username: Option<&str>, credential: &Value,
    ) -> Result<String, SecurityError> {
        // 1. Extract credential_id from rawId
        // 2. Lookup passkey by credential_id
        // 3. Get user
        // 4. Verify signature
        // 5. Update passkey usage
        // Return username
    }
}
```

### `src/middleware.rs` — Axum Auth Middleware

Replaces Python's `get_current_user`, `check_login_ip`, `active_user` dict.

```rust
use std::collections::HashMap;

/// Active user sessions: username → last activity timestamp.
static ACTIVE_USERS: Lazy<Mutex<HashMap<String, Instant>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Axum middleware that extracts the current user from cookie/token.
/// Priority: DEV_AUTH_BYPASS → Bearer login_tokens → JWT cookie.
pub async fn get_current_user(
    request: Request<Body>,
    next: Next,
) -> Result<impl IntoResponse, ApiError>;

/// Middleware that enforces login IP whitelist.
pub async fn check_login_ip(
    request: Request<Body>, next: Next,
) -> Result<impl IntoResponse, ApiError>;

/// Check if a host is within a CIDR whitelist.
pub fn is_ip_allowed(host: &str, whitelist: &[String]) -> bool;
```

Python's `DEV_AUTH_BYPASS` (VERSION == "DEV_VERSION") is handled by checking `cfg!(debug_assertions)` or a compile-time feature flag.

### `src/lib.rs` — Re-exports
```rust
pub mod error;
pub mod jwt;
pub mod webauthn;
pub mod auth_strategy;
pub mod middleware;

pub use error::SecurityError;
pub use jwt::{JwtService, hash_password, verify_password};
pub use webauthn::{WebAuthnService, get_webauthn_service};
pub use auth_strategy::{AuthStrategy, PasswordStrategy, PasskeyStrategy};
pub use middleware::{get_current_user, check_login_ip, is_ip_allowed};
```

### Cargo.toml — ab-security
```toml
[package]
name = "ab-security"
version.workspace = true
edition.workspace = true

[dependencies]
ab-core = { path = "../ab-core" }
ab-database = { path = "../ab-database" }
jsonwebtoken.workspace = true
argon2.workspace = true
webauthn-rs = "0.5"
rand.workspace = true
hex.workspace = true
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
base64.workspace = true
tracing.workspace = true
thiserror.workspace = true
once_cell.workspace = true
async-trait.workspace = true
```

## Crate Design — ab-api

### `src/error.rs` — `ApiError`
```rust
pub enum ApiError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("internal: {0}")]
    Internal(String),
}
```

Implement `IntoResponse` for `ApiError` — maps each variant to the appropriate HTTP status code and JSON body `{msg_en, msg_zh}`.

### `src/response.rs` — Response Helpers

```rust
pub fn u_response(status: bool, msg_en: &str, msg_zh: &str) -> impl IntoResponse;
pub fn api_response<T: Serialize>(data: T) -> impl IntoResponse;
pub fn json_response(status_code: u16, body: Value) -> impl IntoResponse;
```

### `src/router.rs` — Router Tree

```rust
pub fn create_router(
    pool: Pool<Sqlite>,
    config: Arc<Config>,
    // Service handles for routes that need business logic:
    manager: Arc<TorrentManager>,
    rss_engine: Arc<RSSEngine>,
    downloader_factory: Arc<dyn Fn() -> DownloadClient + Send + Sync>,
    // etc.
) -> Router;
```

Or more practically — since `ab-api` is the final integration layer, it takes all dependencies upfront:

```rust
pub fn create_router(app_state: AppState) -> Router {
    Router::new()
        .nest("/v1/auth", routes::auth::router())
        .nest("/v1/passkey", routes::passkey::router())
        .nest("/v1/bangumi", routes::bangumi::router())
        .nest("/v1/rss", routes::rss::router())
        .nest("/v1/config", routes::config::router())
        .nest("/v1/downloader", routes::downloader::router())
        .nest("/v1/log", routes::log::router())
        .nest("/v1/notification", routes::notification::router())
        .nest("/v1/program", routes::program::router())
        .nest("/v1/search", routes::search::router())
        .nest("/v1/setup", routes::setup::router())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
}
```

**AppState** — Shared state injected into all route handlers:
```rust
#[derive(Clone)]
pub struct AppState {
    pub pool: Pool<Sqlite>,
    pub config: Arc<Config>,
    pub jwt_service: Arc<JwtService>,
    pub webauthn_service: Arc<WebAuthnService>,
    // Lazily-instantiated for each request or created per-route
}
```

Route handlers create `TorrentManager`, `RSSEngine`, etc. on-demand from the pool, or use thread-local/factory-pattern singletons. Python uses `with TorrentManager() as manager:` (context manager entering/exiting DB session) — in Rust, each route handler creates lightweight wrappers from the pool.

### Route Modules

Each route module is an Axum router. Example signatures:

**`routes/auth.rs`**: 4 endpoints:
- `POST /login` — `Form<LoginForm>` → JWT cookie + bearer token
- `GET /refresh_token` — Cookie → new JWT
- `GET /logout` — Clear cookie, remove from active_users
- `POST /update` — `Json<UserUpdate>` → update credentials + re-issue token

**`routes/bangumi.rs`**: 15 endpoints (replacing Python's 15 routes):
- `GET /get/all` → `Vec<Bangumi>`
- `GET /get/{id}` → `Bangumi`
- `PATCH /update/{id}` → `Json<BangumiUpdate>` → `ApiResponse`
- `DELETE /delete/{id}?file=false` → `ApiResponse`
- `DELETE /delete/many/?file=false` → `ApiResponse`
- `DELETE /disable/{id}?file=false` → `ApiResponse`
- `DELETE /disable/many/` → `ApiResponse`
- `GET /enable/{id}` → `ApiResponse`
- `GET /refresh/poster/all` → `ApiResponse`
- `GET /refresh/poster/{id}` → `ApiResponse`
- `GET /refresh/calendar` → `ApiResponse`
- `PATCH /archive/{id}` / `PATCH /unarchive/{id}` → `ApiResponse`
- `GET /refresh/metadata` → `ApiResponse`
- `GET /suggest-offset/{id}` → `Json<OffsetSuggestion>`
- `POST /detect-offset` → `Json<DetectOffsetResponse>`
- `GET /needs-review` → `Vec<Bangumi>`
- `POST /dismiss-review/{id}` → `ApiResponse`
- `PATCH /{id}/weekday` → `ApiResponse`
- `GET /reset/all` → `ApiResponse`

**`routes/config.rs`**: 2 endpoints with sensitive field masking (_SENSITIVE_KEYS + _MASK):
- `GET /get` → sanitized config JSON
- `PATCH /update` → `Json<Config>` → save + reload

**`routes/downloader.rs`**: 5 endpoints:
- `GET /torrents` → `Vec<TorrentInfo>`
- `POST /torrents/pause` → `Json<TorrentHashesRequest>`
- `POST /torrents/resume` → same
- `POST /torrents/delete` → `Json<TorrentDeleteRequest>`
- `POST /torrents/tag` → `Json<TorrentTagRequest>` → `ab:ID`
- `POST /torrents/tag/auto` → auto-tag unmatched torrents

**`routes/rss.rs`**: 12 endpoints:
- `GET /` → `Vec<RSSItem>`
- `POST /add` → `Json<RSSItem>` → `ApiResponse`
- `POST /enable/many` → `Json<Vec<i32>>`
- `DELETE /delete/{id}`, `POST /delete/many`
- `PATCH /disable/{id}`, `POST /disable/many`
- `PATCH /update/{id}` → `Json<RSSUpdate>`
- `GET /refresh/all`, `GET /refresh/{id}`
- `GET /torrent/{id}` → `Vec<Torrent>`
- `POST /analysis` → `Json<RSSItem>` → `Bangumi`
- `POST /collect` → `Json<Bangumi>` → `ApiResponse`
- `POST /subscribe` → `Json<{data: Bangumi, rss: RSSItem}>` → `ApiResponse`

**`routes/setup.rs`**: 5 endpoints:
- `GET /status` → `SetupStatusResponse`
- `POST /test-downloader` → `Json<TestDownloaderRequest>` → direct HTTP test
- `POST /test-rss` → `Json<TestRSSRequest>` → RSS fetch + validate
- `POST /test-notification` → `Json<TestNotificationRequest>` → send test
- `POST /complete` → `Json<SetupCompleteRequest>` → save config + create sentinel

**Route composition** — Each route file returns an `axum::Router`:
```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/refresh_token", get(refresh))
        .route("/logout", get(logout))
        .route("/update", post(update_user))
}
```

### `src/routes/mod.rs` — Re-exports
```rust
pub mod auth;
pub mod bangumi;
pub mod config;
pub mod downloader;
pub mod log;
pub mod notification;
pub mod passkey;
pub mod program;
pub mod rss;
pub mod search;
pub mod setup;
```

### `src/lib.rs` — Crate Root
```rust
pub mod router;
pub mod routes;
pub mod error;
pub mod response;

pub use router::create_router;
pub use error::ApiError;
```

### ab-api Cargo.toml
```toml
[package]
name = "ab-api"
version.workspace = true
edition.workspace = true

[dependencies]
ab-core = { path = "../ab-core" }
ab-database = { path = "../ab-database" }
ab-security = { path = "../ab-security" }
ab-downloader = { path = "../ab-downloader" }
ab-rss = { path = "../ab-rss" }
ab-parser = { path = "../ab-parser" }
ab-manager = { path = "../ab-manager" }
ab-network = { path = "../ab-network" }
axum.workspace = true
tokio = { workspace = true, features = ["full"] }
tower-http = { workspace = true, features = ["cors", "trace"] }
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
tracing.workspace = true
thiserror.workspace = true
chrono.workspace = true
```

Note: `ab-searcher` and `ab-notification` are not yet implemented. The `search` route returns a stub (empty SSE stream) and `notification` route returns "not implemented" until those stages are completed.

## File-by-File Build Order (19 files)

### ab-security (6 files)

| # | File | Lines | Description |
|---|------|-------|-------------|
| 1 | `ab-security/Cargo.toml` | 25 | Workspace member |
| 2 | `ab-security/src/error.rs` | 25 | SecurityError enum |
| 3 | `ab-security/src/jwt.rs` | 80 | JWTService + argon2 password hash |
| 4 | `ab-security/src/webauthn.rs` | 200 | WebAuthnService (register/authenticate) |
| 5 | `ab-security/src/auth_strategy.rs` | 100 | AuthStrategy trait + 2 implementations |
| 6 | `ab-security/src/middleware.rs` | 80 | get_current_user Axum middleware |
| 7 | `ab-security/src/lib.rs` | 15 | Re-exports |

### ab-api (13 files)

| # | File | Lines | Description |
|---|------|-------|-------------|
| 1 | `ab-api/Cargo.toml` | 25 | Workspace member |
| 2 | `ab-api/src/error.rs` | 40 | ApiError enum with IntoResponse |
| 3 | `ab-api/src/response.rs` | 20 | u_response, api_response helpers |
| 4 | `ab-api/src/routes/mod.rs` | 15 | pub mod declarations |
| 5 | `ab-api/src/routes/auth.rs` | 90 | Login/refresh/logout/update |
| 6 | `ab-api/src/routes/bangumi.rs` | 350 | Bangumi CRUD + poster/calendar/metadata |
| 7 | `ab-api/src/routes/config.rs` | 80 | Config get/update with sensitive masking |
| 8 | `ab-api/src/routes/downloader.rs` | 120 | Torrent list/pause/resume/delete/tag |
| 9 | `ab-api/src/routes/log.rs` | 40 | Tail log + clear |
| 10 | `ab-api/src/routes/notification.rs` | 100 | Test notification (stub) |
| 11 | `ab-api/src/routes/passkey.rs` | 250 | WebAuthn register/auth/list/delete |
| 12 | `ab-api/src/routes/program.rs` | 80 | Start/stop/restart/status |
| 13 | `ab-api/src/routes/rss.rs` | 180 | RSS CRUD + refresh + analysis + collect |
| 14 | `ab-api/src/routes/search.rs` | 40 | SSE search (stub) |
| 15 | `ab-api/src/routes/setup.rs` | 300 | Setup wizard endpoints |
| 16 | `ab-api/src/router.rs` | 40 | Compose all routers |
| 17 | `ab-api/src/lib.rs` | 10 | Re-exports |

## Key Design Decisions

1. **`bcrypt` → `argon2`** — Python's `passlib.bcrypt` is replaced by the `argon2` crate (as specified in PROJECT.md). `JwtService` uses `jsonwebtoken` crate for HS256 tokens.

2. **`python-jose` → `jsonwebtoken`** — HS256 algorithm, secret loaded from `config/.jwt_secret`, same file path as Python.

3. **`webauthn-rs` replaces `py_webauthn`** — Rust-native WebAuthn implementation. Challenge store uses `Arc<Mutex<HashMap>>` instead of Python's plain dict. TTL cleanup runs on each `store`/`pop` call.

4. **Axum middleware for auth** — Python's FastAPI `Depends(get_current_user)` becomes Axum middleware layers. `get_current_user` runs before protected routes, extracts username, and injects it into request extensions.

5. **`active_user` dict → `Mutex<HashMap<String, Instant>>`** — Python's module-level `active_user: dict[str, datetime]` becomes a global static. Checks on each request to validate session freshness.

6. **`AppState` pattern** — All route handlers share an `AppState` struct (cloned per request) containing `Pool<Sqlite>`, config, and service singletons. Route handlers create lightweight wrappers (`TorrentManager`, `RSSEngine`, etc.) from the pool per request.

7. **Sensitive config masking** — Python's `_sanitize_dict` / `_restore_masked` is replicated exactly: `GET /config` masks sensitive keys (`password`, `api_key`, `token`, `secret` → `********`); `PATCH /config` restores masked values from current config.

8. **Setup wizard** — `POST /setup/complete` creates `config/.setup_complete` sentinel file. `GET /setup/status` checks sentinel + config equality. Logic matches Python exactly including DEV_VERSION bypass.

9. **Search + Notification stubs** — `search.rs` returns empty SSE stream; `notification.rs` returns "not implemented". These are placeholders until Stage 9 (ab-notification) and Stage 10 (ab-searcher) are implemented.

10. **No Python context managers** — `with TorrentManager() as manager:`, `async with DownloadClient() as client:`, `with RSSEngine() as engine:` become direct construction in Axum handlers using state/pool.

## Test Plan

**Unit tests** (`ab-security`):
- `JwtService::create_access_token` + `verify_token` round-trip
- `verify_token` rejects expired tokens
- `hash_password` + `verify_password` round-trip
- `verify_password` rejects wrong password
- WebAuthn challenge store: store + pop by key
- WebAuthn challenge store: TTL cleanup
- WebAuthn challenge store: max capacity eviction
- `is_ip_allowed` with CIDR whitelist
- Auth middleware: invalid cookie returns 401
- Auth middleware: valid cookie passes through

**Integration tests** (`ab-api`, with wiremock + in-memory SQLite):
- `POST /auth/login` with valid credentials → 200 + cookie
- `POST /auth/login` with invalid credentials → 401
- `GET /auth/logout` → clears cookie
- `GET /bangumi/get/all` → returns bangumi list
- `PATCH /bangumi/update/{id}` → updates bangumi
- `DELETE /bangumi/delete/{id}` → deletes bangumi
- `GET /config/get` → sanitized config
- `PATCH /config/update` → saves config
- `GET /log` → returns log tail
- `GET /setup/status` → setup needed
- `POST /setup/complete` → creates sentinel file
- `POST /setup/test-downloader` → tests connection

## Acceptance Criteria

1. `cargo build -p ab-security` compiles without errors
2. `cargo build -p ab-api` compiles without errors
3. `cargo test -p ab-security` passes all tests
4. `cargo test -p ab-api` passes all tests
5. JWT tokens: create (HS256, `sub` claim), decode, verify expiration
6. Password hashing: argon2 hash + verify, rejects wrong passwords
7. WebAuthn: full registration + authentication flow (start → verify → store)
8. Auth middleware: rejects unauthenticated requests (401), passes authenticated requests
9. All 11 API route groups registered under `/v1` prefix
10. Config GET masks sensitive fields; Config PATCH properly restores them
11. Setup wizard writes sentinel file and persists configuration
12. Downloader endpoints: CRUD torrent operations via `DownloadClient`
13. Setup `test-downloader` validates qBittorrent without `ab-downloader` crate (direct `reqwest`)
14. Search and notification endpoints return graceful stubs (not panics)
15. `ab:ID` tag paradigm works end-to-end: tag → lookup → rename offset
