# bangumi-follower 项目骨架设计

> 基于 Auto_Bangumi Python 后端的 Rust 迁移项目，Cargo workspace 多 crate 架构。

---

## 一、目录结构

```
bangumi-follower/
├── Cargo.toml                    # workspace 根
├── Cargo.lock

├── config/                       # 运行时配置目录 (JSON)
├── posters/                      # TMDB 海报缓存 (运行时)
├── webui/                        # 前端 Vue SPA (不变)
│
├── crates/
│   ├── ab-core/                  # conf + checker + update (零依赖, 无外部 IO)
│   │   ├── Cargo.toml            # serde, config, serde_json
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config/           # Config struct, JSON r/w, env override
│   │       ├── checker/          # 系统健康检查
│   │       └── update/           # 启动迁移
│   │
│   ├── ab-database/              # database + models
│   │   ├── Cargo.toml            # rusqlite, serde, json
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models/           # 数据表 struct + serde
│   │       ├── schema/           # 迁移
│   │       └── repo/             # CRUD (bangumi, rss, torrent, user, passkey)
│   │
│   ├── ab-network/               # network (HTTP 客户端)
│   │   ├── Cargo.toml            # reqwest, tokio, quick-xml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client/           # 共享 reqwest client, 代理
│   │       └── site/             # Mikan RSS 解析
│   │
│   ├── ab-parser/                # parser (解析引擎, 纯函数)
│   │   ├── Cargo.toml            # regex
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── raw_parser        # 正则解析
│   │       ├── tmdb_parser       # TMDB API
│   │       ├── mikan_parser
│   │       ├── bgm_parser
│   │       ├── offset_detector
│   │       ├── torrent_parser
│   │       └── openai_parser     # (可选, feature gate)
│   │
│   ├── ab-downloader/            # downloader
│   │   ├── Cargo.toml            # reqwest
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client/           # qbittorrent, aria2, transmission, mock
│   │       └── path.rs           # TorrentPath
│   │
│   ├── ab-rss/                   # rss engine + analyser
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── engine.rs
│   │       └── analyser.rs
│   │
│   ├── ab-manager/               # manager (业务编排)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── torrent.rs        # TorrentManager
│   │       ├── renamer.rs        # Renamer
│   │       └── collector.rs      # SeasonCollector
│   │
│   ├── ab-notification/          # notification
│   │   ├── Cargo.toml            # reqwest
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── manager.rs
│   │       ├── base.rs
│   │       └── providers/        # 8 个 provider
│   │
│   ├── ab-searcher/              # searcher (SSE 流式)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── searcher.rs
│   │
│   ├── ab-core-thread/           # core 后台任务 (depends on all above)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── rss_thread.rs
│   │       ├── rename_thread.rs
│   │       ├── offset_scan_thread.rs
│   │       ├── calendar_thread.rs
│   │       └── program.rs
│   │
│   ├── ab-security/              # jwt + webauthn
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── jwt.rs
│   │       ├── webauthn.rs
│   │       └── auth_strategy.rs
│   │
│   ├── ab-api/                   # Axum 路由
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── router.rs         # router tree
│   │       └── routes/           # 11 个路由模块
│   │
│   ├── ab-mcp/                   # MCP SSE 服务器
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── server.rs
│   │       ├── tools.rs          # 10 个工具
│   │       └── resources.rs
│   │
│   └── ab-bin/                   # 最终二进制入口
│       ├── Cargo.toml            # depends on all above
│       └── src/
│           └── main.rs           # Axum server 启动
```

---

## 二、crate 依赖图

```
ab-core (无依赖)
  ↑
ab-database (依赖 ab-core: config)
  ↑
ab-manager (依赖 ab-core, ab-database, ab-downloader, ab-rss, ab-searcher)
  ↑
ab-core-thread (依赖 ab-manager, ab-notification)
  ↑
ab-bin (依赖所有)

水平依赖:
  ab-network ← ab-parser ← ab-rss
                           ← ab-notification
                           ← ab-searcher
  ab-downloader ← ab-manager
  ab-security   ← ab-api     ← ab-bin
  ab-mcp        ← ab-bin
```

---

## 三、关键 Rust crate 选型

| 领域 | Python | Rust | 理由 |
|------|--------|------|------|
| Web 框架 | FastAPI | **axum** | 生态最大, tokio 原生, tower 中间件 |
| 数据库 | SQLModel / SQLAlchemy | **sqlx** | 编译期 SQL 校验, 异步, 原生 SQL |
| 序列化 | Pydantic | **serde** | 标准, derive 宏 |
| HTTP 客户端 | httpx | **reqwest** | 标准, 异步, 代理支持 |
| JWT | python-jose | **jsonwebtoken** | 标准实现 |
| 密码哈希 | passlib (bcrypt) | **argon2** | 更安全, Rust 原生 |
| SOCKS | httpx_socks | reqwest+socks | reqwest 内置 |
| 异步运行时 | asyncio | **tokio** | 标准 |
| 环境变量 | python-dotenv | **dotenvy** | 轻量 |
| XML 解析 | xml.etree.ElementTree | **quick-xml** | 零拷贝, 高性能 |
| MCP | mcp (Python SDK) | mcp-core / mcp-sse-server | Rust MCP SDK |
| 文件上传 | python-multipart | axum-extra / multer | axum 扩展 |

---

## 四、分阶段实现顺序

| 阶段 | 计划文件 | crate | 依赖 | 说明 |
|------|---------|-------|------|------|
| 0 | `PROJECT_STAGE_0.md` | workspace 骨架 | 无 | Cargo.toml, README |
| 1 | `PROJECT_STAGE_1.md` | ab-core | 无 | Config struct + checker + update |
| 2 | `PROJECT_STAGE_2.md` | ab-database | ab-core | 数据表定义 + 迁移 + CRUD |
| 3 | `PROJECT_STAGE_3.md` | ab-network | ab-core | HTTP 客户端 + RSS 解析 |
| 4 | `PROJECT_STAGE_4.md` | ab-parser | ab-core | 解析引擎 |
| 5 | `PROJECT_STAGE_5.md` | ab-downloader | ab-network | qBittorrent/Aria2 客户端 |
| 6 | `PROJECT_STAGE_6.md` | ab-rss | ab-database, ab-network, ab-parser | RSS Engine + Feed Analysis |
| 7 | `PROJECT_STAGE_7.md` | ab-manager | ab-database, ab-downloader, ab-rss | 业务编排 |
| 8 | `PROJECT_STAGE_8.md` | ab-security, ab-api | ab-core, ab-database | Web API + JWT |
| 9 | `PROJECT_STAGE_9.md` | ab-notification, ab-searcher, ab-core-thread | ab-manager, ab-network, ab-parser | 通知 + 搜索 + 后台任务 |
| 10 | `PROJECT_STAGE_10.md` | ab-mcp, ab-bin | ab-manager, ab-api | MCP + 二进制入口 |

---

## 五、设计原则

1. **按 Python 模块边界划分 crate** — 保持与 `AUTO_BANGUMI_PROJECT.md` 分析一致
2. **上层依赖下层** — ab-core 无依赖, ab-bin 依赖所有
3. **纯逻辑与 IO 分离** — parser 纯函数, network/downloader 含 IO
4. **sqlx 原生 SQL** — 不引入 sea-orm, 手动管理迁移
5. **async 优先** — 所有 crate 默认异步 (tokio)
6. **编译期验证** — sqlx 编译期 SQL 检查, serde 严格反序列化
