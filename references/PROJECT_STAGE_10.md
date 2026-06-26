# Stage 10: ab-mcp + ab-bin

## Overview

The last two crates — MCP protocol server and the final binary that wires everything together.

| Crate | Purpose | Depends On |
|---|---|---|
| `ab-mcp` | MCP SSE server (tools, resources, security middleware) | ab-manager, ab-downloader, ab-rss, ab-searcher, ab-core, ab-security, ab-core-thread |
| `ab-bin` | Binary entrypoint (Axum server, route mounting, CLI) | all 13 crates |

## ab-mcp

### Files

```
ab-mcp/
  Cargo.toml
  src/
    lib.rs
    server.rs        — MCP Server + SSE transport
    tools.rs         — 10 tool definitions + dispatch
    resources.rs     — 3 static resources + 1 template
    security.rs      — McpAccessMiddleware (IP whitelist + bearer token)
```

Uses the [`mcp-server`](https://crates.io/crates/mcp-server) Rust SDK (or equivalent). Python uses `mcp` PyPI package with `Starlette` SSE transport.

If no mature Rust MCP server crate exists, implement a minimal one:
- SSE endpoint at `/sse`
- POST endpoint at `/messages/` for client-to-server
- JSON-RPC format for tool calls and resource reads
- The `mcp-rs` crate family or `modelcontextprotocol` crate

### Server (server.rs)

```rust
pub fn create_mcp_app() -> Router {
    // SSE + POST /messages/
    // Wrapped with McpAccessMiddleware
}
```

**Endpoints:**
- `GET /sse` — SSE stream for MCP clients to receive tool results
- `POST /messages/` — Client-to-server JSON-RPC messages

**Tools registered:**
1. `list_anime` — List all tracked anime (active_only filter)
2. `get_anime` — Get anime detail by ID
3. `search_anime` — Search torrent sites by keyword
4. `subscribe_anime` — Subscribe via RSS link
5. `unsubscribe_anime` — Disable or delete subscription
6. `list_downloads` — Download client torrent status
7. `list_rss_feeds` — RSS feed health
8. `get_program_status` — Version + running state
9. `refresh_feeds` — Trigger RSS refresh
10. `update_anime` — Update episode/season offset, filter

**JSON-RPC dispatch** maps method names to handler functions:

```rust
async fn handle_tool(name: &str, arguments: &HashMap<String, Value>) -> Result<Value>;
```

### Tools (tools.rs)

Each tool maps exactly to its Python MCP counterpart in `module/mcp/tools.py`:

| Tool Name | Handler | Python Source |
|---|---|---|
| `list_anime` | `TorrentManager::search_all()` / `search_all_bangumi()` | `_list_anime` |
| `get_anime` | `TorrentManager::search_one(id)` | `_get_anime` |
| `search_anime` | `SearchTorrent::analyse_keyword()` | `_search_anime` |
| `subscribe_anime` | `RSSAnalyser::link_to_data()` + `SeasonCollector::subscribe_season()` | `_subscribe_anime` |
| `unsubscribe_anime` | `TorrentManager::delete_rule()` / `disable_rule()` | `_unsubscribe_anime` |
| `list_downloads` | `DownloadClient::get_torrent_info(category="Bangumi")` | `_list_downloads` |
| `list_rss_feeds` | `RSSEngine::rss.search_all()` | `_list_rss_feeds` |
| `get_program_status` | `Program::is_running()` + `Program::first_run()` | `_get_program_status` |
| `refresh_feeds` | `DownloadClient` + `RSSEngine::refresh_rss()` | `_refresh_feeds` |
| `update_anime` | `TorrentManager::bangumi.search_id()` + `update_rule()` | `_update_anime` |

**Helper:** `_bangumi_to_dict(b: Bangumi) -> serde_json::Value` (same 19 fields as Python)

**search_anime note:** Python's `analyse_keyword` yields JSON via async generator. In Rust, collect up to 20 results into a `Vec<Value>` and return as JSON array. The SSE streaming is handled by the MCP transport, not at this level.

### Resources (resources.rs)

Static resources:
| URI | Description | Source |
|---|---|---|
| `autobangumi://anime/list` | All tracked anime | `TorrentManager::bangumi.search_all()` |
| `autobangumi://status` | Version + running state | `Program` singleton |
| `autobangumi://rss/feeds` | RSS feed health | `RSSEngine::rss.search_all()` |

Resource template:
| URI Template | Description |
|---|---|
| `autobangumi://anime/{id}` | Single anime by ID |

**Handler dispatch:** match URI prefix, extract ID for template, return JSON string.

### Security (security.rs)

```rust
pub struct McpAccessMiddleware {
    whitelist: Vec<IpNet>,       // parsed CIDR ranges
    tokens: Vec<String>,          // valid bearer tokens
}
```

Implementation (matches Python exactly):
- Check `Authorization: Bearer <token>` header against configured tokens
- Check client IP against `settings.security.mcp_whitelist` CIDR ranges
- If both empty, deny all (403)
- Cache parsed CIDR networks (use `LRU` cache or just re-parse on each request — Python uses `lru_cache(maxsize=128)`)

IP parsing:
```rust
use std::net::IpAddr;
fn is_allowed(host: &str, whitelist: &[IpNet]) -> bool {
    let addr: IpAddr = host.parse().ok()?;
    whitelist.iter().any(|net| net.contains(&addr))
}
```

Uses [`ipnet`](https://crates.io/crates/ipnet) crate for CIDR parsing.

### Configuration fields

Already defined in Stage 8 ab-core config:
```rust
pub struct SecurityConfig {
    pub mcp_whitelist: Vec<String>,  // CIDR strings
    pub mcp_tokens: Vec<String>,     // bearer tokens
}
```

## ab-bin

### Files

```
ab-bin/
  Cargo.toml
  src/
    main.rs
```

This is the final binary. Dependencies: **all 13 crates** (ab-core, ab-database, ab-network, ab-parser, ab-downloader, ab-rss, ab-manager, ab-notification, ab-searcher, ab-core-thread, ab-security, ab-api, ab-mcp).

### main.rs

```rust
#[tokio::main]
async fn main() {
    // 1. Setup logging (tracing)
    // 2. Parse CLI args (host, port)
    // 3. Create Program
    // 4. Build Axum app
    // 5. Start server with lifespan
}
```

**Axum app structure** (matches Python `main.py:create_app()`):

```rust
fn create_app(program: Arc<Program>) -> Router {
    Router::new()
        // API routes
        .nest("/api/v1", ab_api::create_router())
        // MCP server
        .nest("/mcp", ab_mcp::create_mcp_app())
        // CORS middleware
        .layer(CorsLayer::permissive())  // Python allows empty origins
        // Static files (production only)
        // ...
        // Poster serving
        .route("/posters/{*path}", get(serve_poster))
}
```

**Lifespan** (equivalent to Python's `@asynccontextmanager lifespan`):

```rust
// On startup: tokio::spawn(program.startup())
// On shutdown: program.stop().await
```

Python's lifespan runs `program.startup()` in background task, then yields. In Rust:
1. Spawn `program.startup()` as background task
2. `axum::serve` with graceful shutdown
3. On signal: call `program.stop()`

**Poster serving** (Python `@app.get("/posters/{path:path}")`):

```rust
async fn serve_poster(path: PathBuf) -> Result<Response, StatusCode> {
    let base = std::path::Path::new("data/posters").canonicalize()?;
    let requested = base.join(path).canonicalize()?;
    if !requested.starts_with(&base) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Response::new(/* file contents */))
}
```

**Development mode** (DEV_VERSION → redirect `/` to `/docs`):
```rust
fn is_dev_version() -> bool {
    option_env!("DEV_VERSION").is_some() || VERSION == "DEV_VERSION"
}
```

If dev: redirect `/` to `/docs` (utoipa/swagger).
If production: serve static files from `dist/`.

**CLI arguments:**
```rust
struct Cli {
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
    #[arg(short, long, default_value_t = 3000)]
    port: u16,
    #[arg(long)]
    ipv6: bool,
}
```

Uses `clap` for argument parsing.

### Cargo.toml

```toml
[package]
name = "ab-bin"
version.workspace = true
edition.workspace = true

[dependencies]
ab-core = { path = "../ab-core" }
ab-database = { path = "../ab-database" }
ab-network = { path = "../ab-network" }
ab-parser = { path = "../ab-parser" }
ab-downloader = { path = "../ab-downloader" }
ab-rss = { path = "../ab-rss" }
ab-manager = { path = "../ab-manager" }
ab-notification = { path = "../ab-notification" }
ab-searcher = { path = "../ab-searcher" }
ab-core-thread = { path = "../ab-core-thread" }
ab-security = { path = "../ab-security" }
ab-api = { path = "../ab-api" }
ab-mcp = { path = "../ab-mcp" }
axum = { workspace = true }
tower-http = { workspace = true, features = ["cors"] }
tokio = { workspace = true, features = ["full"] }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
clap = { workspace = true, features = ["derive"] }
serde = { workspace = true }
```

## Integration points with ab-api (Stage 8)

Stage 8 left stubs for these API routes that now get real implementations:

1. **`GET /search/bangumi`** — Now uses `SearchTorrent::analyse_keyword()` with SSE streaming via `axum::response::sse::Sse`
2. **`GET /search/provider`** — Returns keys from search provider config
3. **`GET|PUT /search/provider/config`** — Read/write `config/search_provider.json`
4. **`POST /notification/test`** — Uses `NotificationManager::test_provider()`
5. **`POST /notification/test-config`** — Uses `NotificationManager::test_provider_config()`
6. **`GET /start`** — Calls `Program::start()`
7. **`GET /stop`** — Calls `Program::stop()`
8. **`GET /restart`** — Calls `Program::restart()`
9. **`GET /status`** — Returns status from `Program`

## Key decisions

1. **Minimal MCP SDK.** If no mature Rust MCP server crate exists, implement SSE + JSON-RPC handling directly with axum. The MCP protocol is simple enough: SSE for server→client, POST /messages/ for client→server, JSON-RPC 2.0 for method calls.

2. **`search_anime` returns collected Vec, not stream.** Python yields individual JSON lines via SSE. In MCP context, the JSON-RPC response contains the full array. The SSE streaming happens at the MCP transport level, not the tool level.

3. **Program singleton.** Python uses a module-level `program = Program()` instance. Rust uses `Arc<Program>` shared via `axum::extract::State`.

4. **Static files only in release.** Development mode redirects to swagger. Production mounts `dist/assets` and `dist/images` and serves `index.html` for SPA fallback.

5. **Security middleware for MCP only.** The MCP endpoint is restricted to local network + bearer tokens. The main API uses JWT auth (Stage 8).

6. **`ab-bin` must be the only binary crate.** All other crates are `lib` type. This ensures a single entrypoint matching Python's `python main.py`.

7. **Environment-based config.** `IPV6` env var toggles IPv6 binding (matches Python). `HOST` env var overrides default 0.0.0.0.

8. **Optional: `tower-http` trace layer** for request logging. Replaces Python's `uvicorn` logging config.
