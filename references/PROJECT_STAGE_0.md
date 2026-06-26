# Stage 0: 项目骨架 — 详细实施计划

> 建立 Cargo workspace 根结构、所有 crate 的占位 Cargo.toml 和 lib.rs，确保 `cargo build` 通过。

---

## 一、目标

- 创建 workspace 根 `Cargo.toml`，声明所有 member crate
- 创建 14 个 crate 各自的 `Cargo.toml` 和最小 `lib.rs`/`main.rs`
- 配置 `.gitignore` 覆盖 Rust + 运行时目录
- 验证 `cargo build` 一次通过

---

## 二、需要创建/修改的文件清单

| # | 文件路径 | 操作 | 说明 |
|---|---------|------|------|
| 0 | `Cargo.toml` | 创建 | workspace 根，列所有 member |
| 1 | `.gitignore` | 修改 | 补充 `config/`、`posters/` |
| 2 | `README.md` | 不改 | 已有内容，阶段 0 不涉及 |
| 3 | `crates/ab-core/Cargo.toml` | 创建 | 依赖: serde, serde_json, config, dotenvy |
| 4 | `crates/ab-core/src/lib.rs` | 创建 | 空 pub mod + `pub fn version()` |
| 5 | `crates/ab-database/Cargo.toml` | 创建 | 依赖: serde, serde_json, sqlx, ab-core |
| 6 | `crates/ab-database/src/lib.rs` | 创建 | 空 pub mod |
| 7 | `crates/ab-network/Cargo.toml` | 创建 | 依赖: reqwest, tokio, quick-xml, ab-core |
| 8 | `crates/ab-network/src/lib.rs` | 创建 | 空 pub mod |
| 9 | `crates/ab-parser/Cargo.toml` | 创建 | 依赖: serde, serde_json, regex, ab-core |
| 10 | `crates/ab-parser/src/lib.rs` | 创建 | 空 pub mod |
| 11 | `crates/ab-downloader/Cargo.toml` | 创建 | 依赖: reqwest, ab-core |
| 12 | `crates/ab-downloader/src/lib.rs` | 创建 | 空 pub mod |
| 13 | `crates/ab-rss/Cargo.toml` | 创建 | 依赖: quick-xml, serde, ab-core, ab-network, ab-parser |
| 14 | `crates/ab-rss/src/lib.rs` | 创建 | 空 pub mod |
| 15 | `crates/ab-manager/Cargo.toml` | 创建 | 依赖: ab-core, ab-database, ab-downloader, ab-rss |
| 16 | `crates/ab-manager/src/lib.rs` | 创建 | 空 pub mod |
| 17 | `crates/ab-notification/Cargo.toml` | 创建 | 依赖: reqwest, serde, ab-core |
| 18 | `crates/ab-notification/src/lib.rs` | 创建 | 空 pub mod |
| 19 | `crates/ab-searcher/Cargo.toml` | 创建 | 依赖: reqwest, ab-core, ab-parser |
| 20 | `crates/ab-searcher/src/lib.rs` | 创建 | 空 pub mod |
| 21 | `crates/ab-core-thread/Cargo.toml` | 创建 | 依赖: tokio, ab-manager, ab-notification |
| 22 | `crates/ab-core-thread/src/lib.rs` | 创建 | 空 pub mod |
| 23 | `crates/ab-security/Cargo.toml` | 创建 | 依赖: serde, jsonwebtoken, argon2, ab-core |
| 24 | `crates/ab-security/src/lib.rs` | 创建 | 空 pub mod |
| 25 | `crates/ab-api/Cargo.toml` | 创建 | 依赖: axum, tower, tokio, serde, ab-core, ab-security, ab-manager |
| 26 | `crates/ab-api/src/lib.rs` | 创建 | 空 pub mod |
| 27 | `crates/ab-mcp/Cargo.toml` | 创建 | 依赖: axum, serde, serde_json, ab-manager |
| 28 | `crates/ab-mcp/src/lib.rs` | 创建 | 空 pub mod |
| 29 | `crates/ab-bin/Cargo.toml` | 创建 | 依赖: 所有 ab-* crate, tokio |
| 30 | `crates/ab-bin/src/main.rs` | 创建 | `fn main() { println!("Hello"); }` |
| 31 | `config/.gitkeep` | 创建 | 占位，保证 config/ 入 git |
| 32 | `posters/.gitkeep` | 创建 | 占位 |

---

## 三、Workspace 根 Cargo.toml 内容

```toml
[workspace]
resolver = "2"
members = [
    "crates/ab-core",
    "crates/ab-database",
    "crates/ab-network",
    "crates/ab-parser",
    "crates/ab-downloader",
    "crates/ab-rss",
    "crates/ab-manager",
    "crates/ab-notification",
    "crates/ab-searcher",
    "crates/ab-core-thread",
    "crates/ab-security",
    "crates/ab-api",
    "crates/ab-mcp",
    "crates/ab-bin",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["bangumi-follower"]
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "socks"] }
quick-xml = { version = "0.37", features = ["serialize"] }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
config = "0.15"
dotenvy = "0.15"
regex = "1"
jsonwebtoken = "9"
argon2 = "0.5"
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
axum = "0.8"
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace"] }
```

---

## 四、各 crate Cargo.toml 模板

所有 crate 统一使用 `edition = "2024"`、`version = "0.1.0"`。

### 4.1 ab-core

```toml
[package]
name = "ab-core"
version.workspace = true
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
config.workspace = true
dotenvy.workspace = true
thiserror.workspace = true
tracing.workspace = true
```

### 4.2 ab-database

```toml
[package]
name = "ab-database"
version.workspace = true
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
sqlx.workspace = true
thiserror.workspace = true
ab-core = { path = "../ab-core" }
```

### 4.3 ab-network

```toml
[package]
name = "ab-network"
version.workspace = true
edition.workspace = true

[dependencies]
reqwest.workspace = true
quick-xml.workspace = true
thiserror.workspace = true
tracing.workspace = true
ab-core = { path = "../ab-core" }
```

### 4.4 ab-parser

```toml
[package]
name = "ab-parser"
version.workspace = true
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
regex.workspace = true
thiserror.workspace = true
tracing.workspace = true
ab-core = { path = "../ab-core" }
```

### 4.5 ab-downloader

```toml
[package]
name = "ab-downloader"
version.workspace = true
edition.workspace = true

[dependencies]
reqwest.workspace = true
serde.workspace = true
thiserror.workspace = true
tracing.workspace = true
ab-core = { path = "../ab-core" }
```

### 4.6 ab-rss

```toml
[package]
name = "ab-rss"
version.workspace = true
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
quick-xml.workspace = true
thiserror.workspace = true
tracing.workspace = true
ab-core = { path = "../ab-core" }
ab-network = { path = "../ab-network" }
ab-parser = { path = "../ab-parser" }
```

### 4.7 ab-manager

```toml
[package]
name = "ab-manager"
version.workspace = true
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
ab-core = { path = "../ab-core" }
ab-database = { path = "../ab-database" }
ab-downloader = { path = "../ab-downloader" }
ab-rss = { path = "../ab-rss" }
ab-searcher = { path = "../ab-searcher" }
```

### 4.8 ab-notification

```toml
[package]
name = "ab-notification"
version.workspace = true
edition.workspace = true

[dependencies]
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
ab-core = { path = "../ab-core" }
```

### 4.9 ab-searcher

```toml
[package]
name = "ab-searcher"
version.workspace = true
edition.workspace = true

[dependencies]
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
tracing.workspace = true
ab-core = { path = "../ab-core" }
ab-parser = { path = "../ab-parser" }
```

### 4.10 ab-core-thread

```toml
[package]
name = "ab-core-thread"
version.workspace = true
edition.workspace = true

[dependencies]
tokio.workspace = true
tracing.workspace = true
ab-core = { path = "../ab-core" }
ab-manager = { path = "../ab-manager" }
ab-notification = { path = "../ab-notification" }
```

### 4.11 ab-security

```toml
[package]
name = "ab-security"
version.workspace = true
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
jsonwebtoken.workspace = true
argon2.workspace = true
thiserror.workspace = true
tracing.workspace = true
ab-core = { path = "../ab-core" }
ab-database = { path = "../ab-database" }
```

### 4.12 ab-api

```toml
[package]
name = "ab-api"
version.workspace = true
edition.workspace = true

[dependencies]
axum.workspace = true
tower.workspace = true
tower-http.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
tracing.workspace = true
ab-core = { path = "../ab-core" }
ab-security = { path = "../ab-security" }
ab-manager = { path = "../ab-manager" }
ab-core-thread = { path = "../ab-core-thread" }
ab-database = { path = "../ab-database" }
```

### 4.13 ab-mcp

```toml
[package]
name = "ab-mcp"
version.workspace = true
edition.workspace = true

[dependencies]
axum.workspace = true
serde.workspace = true
serde_json.workspace = true
tracing.workspace = true
ab-core = { path = "../ab-core" }
ab-manager = { path = "../ab-manager" }
```

### 4.14 ab-bin

```toml
[package]
name = "ab-bin"
version.workspace = true
edition.workspace = true

[dependencies]
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
ab-core = { path = "../ab-core" }
ab-database = { path = "../ab-database" }
ab-api = { path = "../ab-api" }
ab-core-thread = { path = "../ab-core-thread" }
ab-mcp = { path = "../ab-mcp" }
```

---

## 五、最小源文件内容

### 5.1 ab-core/src/lib.rs

```rust
pub mod config;
pub mod checker;
pub mod update;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn version() -> &'static str {
    VERSION
}
```

### 5.2 其他 crate 的 lib.rs

统一格式——仅导出模块树（后续逐步填充具体模块）：

```rust
// ab-core 之外的 crate:
// 例如 ab-database/src/lib.rs:
// pub mod models;
// pub mod repo;
```

阶段 0 只需空文件：

```rust
// 所有 ab-* crate 的 lib.rs 初始内容为空 pub 声明
// 例如 ab-database/src/lib.rs:
```

实际上最简单的办法是每个 lib.rs 只放一行 `// TODO: implement` 注释。但 Rust 要求 lib.rs 至少有一个声明才能通过 `cargo check`。所以放一个空 `pub mod _placeholder {}` 或者：

```rust
//! crate 描述
// TODO: implement
```

这样 `cargo build` 可以过。

### 5.3 ab-bin/src/main.rs

```rust
fn main() {
    println!("bangumi-follower v{}", ab_core::version());
}
```

---

## 六、.gitignore 修改

在现有 `.gitignore`（已有 Rust 标准内容）末尾追加：

```
# Runtime directories
config/config.json
config/config_dev.json
posters/
```

---

## 七、实施步骤与执行顺序

```
Step 1:  创建目录结构
Step 2:  写入 workspace 根 Cargo.toml
Step 3:  写入 14 个 crate 的 Cargo.toml
Step 4:  写入 lib.rs / main.rs 存根
Step 5:  修改 .gitignore
Step 6:  创建 config/.gitkeep, posters/.gitkeep
Step 7:  运行 cargo check（离线约束检查）
Step 8:  运行 cargo build（联网下载依赖后编译）
```

---

## 八、验收标准

- [ ] `cargo check` 通过（零 warning，零 error）
- [ ] `cargo build` 通过
- [ ] `cargo run -p ab-bin` 输出 `bangumi-follower v0.1.0`
- [ ] 14 个 crate 均被 workspace 正确识别（`cargo metadata` 可见）
- [ ] 目录结构与 `PROJECT.md` 一致
