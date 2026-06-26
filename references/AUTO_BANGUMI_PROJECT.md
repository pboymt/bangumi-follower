# Auto_Bangumi 原始项目后端分析报告

> 基于 `/mnt/usb/git/Auto_Bangumi` 源代码分析的完整架构文档，供 Rust 迁移参考。

---

## 一、项目概览

| 项目 | 值 |
|------|-----|
| 语言/框架 | Python 3.13+ / **FastAPI** |
| 数据库 | SQLite + **SQLModel** (Pydantic + SQLAlchemy) |
| HTTP 客户端 | **httpx** (异步, 支持 SOCKS/HTTP 代理) |
| 任务调度 | **asyncio** 原生（`asyncio.create_task` + 事件循环） |
| 认证 | JWT (HttpOnly Cookie) + WebAuthn Passkey + Bearer Token |
| 配置 | JSON 文件 (`config/config.json`) + 环境变量覆盖 |
| 前端 | Vue 3 + TypeScript (SPA, 不在 Rust 迁移范围内) |
| 测试 | pytest + pytest-asyncio |
| 包管理 | UV (Python) |

### 文件统计

- 后端 Python 源文件: ~124 个文件 (不含测试)
- 测试文件: ~42 个
- 最大源文件: `database/bangumi.py` (693 行), `manager/renamer.py` (489 行), `api/bangumi.py` (383 行), `manager/torrent.py` (371 行), `mcp/tools.py` (359 行)

---

## 二、模块架构与依赖关系

```
main.py (FastAPI app 入口)
├── module/api/          (路由层 - 11 个路由文件)
├── module/conf/         (配置管理)
├── module/core/         (后台任务调度器)
│   ├── program.py       (Program - 主控制器)
│   ├── sub_thread.py    (4 个后台循环线程)
│   ├── status.py        (运行状态检查)
│   └── offset_scanner.py (偏移量扫描)
├── module/database/     (数据访问层)
│   ├── engine.py        (SQLAlchemy 引擎)
│   ├── combine.py       (Database 会话封装 + 迁移)
│   ├── bangumi.py       (Bangumi CRUD, 693 行)
│   ├── rss.py / torrent.py / user.py / passkey.py
├── module/models/       (数据模型 - Pydantic + SQLModel)
├── module/network/      (HTTP 网络层)
│   ├── request_url.py   (httpx 客户端, 代理支持)
│   ├── request_contents.py (XML/JSON/二进制下载)
│   └── site/mikan.py    (Mikan RSS XML 解析)
├── module/parser/       (解析引擎)
│   ├── title_parser.py  (解析编排器)
│   └── analyser/        (8 个解析器)
├── module/rss/          (RSS 处理)
│   ├── engine.py        (RSSEngine - RSS 刷新/匹配引擎)
│   └── analyser.py      (RSSAnalyser - RSS 到 Bangumi 数据转换)
├── module/downloader/   (下载客户端抽象)
│   ├── download_client.py (DownloadClient - 统一接口)
│   ├── path.py          (TorrentPath - 路径与文件管理)
│   └── client/          (qb_downloader, aria2_downloader, mock_downloader, tr_downloader)
├── module/manager/      (业务编排层)
│   ├── torrent.py       (TorrentManager - 番剧订阅管理)
│   ├── renamer.py       (Renamer - 文件重命名, 489 行)
│   └── collector.py     (SeasonCollector - 整季收集)
├── module/notification/ (通知系统)
│   ├── base.py          (NotificationProvider 抽象基类)
│   ├── manager.py       (NotificationManager - 多提供者分发)
│   └── providers/       (8 个提供者)
├── module/searcher/     (种子搜索 - SSE 流式)
├── module/security/     (认证安全层)
│   ├── jwt.py           (JWT + bcrypt)
│   ├── webauthn.py      (WebAuthn/Passkey)
│   └── api.py           (FastAPI 认证依赖)
├── module/mcp/          (MCP AI 集成)
│   ├── server.py        (MCP SSE 服务器, Starlette 子应用)
│   ├── tools.py         (10 个 LLM 工具, 359 行)
│   ├── resources.py     (MCP 资源定义)
│   └── security.py      (MCP IP 白名单中间件)
├── module/checker/      (系统健康检查)
├── module/update/       (启动迁移)
└── module/utils/        (工具函数)
```

---

## 三、关键数据流

### 3.1 RSS 处理流水线（核心业务流程）

```
订阅 RSS → RSSThread 循环(默认 15min)
  ── RSSEngine.refresh_rss()
      ├── 1. 获取所有启用的 RSSItem
      ├── 2. 并发拉取所有 RSS feed (asyncio.gather)
      ├── 3. 更新连接状态 (healthy/error)
      ├── 4. 逐 torrent 调用 match_torrent() 匹配 Bangumi
      │      └── 使用 title_raw + title_aliases 做子串匹配
      └── 5. 匹配成功的 torrent 调用 client.add_torrent()
           └── 传递 tags="ab:<bangumi_id>" 用于后续偏移量查找

并行发生的:
  RSSAnalyser.rss_to_data()  # 新番识别
      ├── 1. 从 RSS 获取所有 torrent 列表
      ├── 2. 匹配已有 Bangumi (match_list)
      ├── 3. 未匹配的 → raw_parser() 解析种子名
      │      └── 正则解析: 组名、标题、季度、集数、分辨率、来源、字幕
      ├── 4. official_title_parser()
      │      ├── Mikan → mikan_parser() (从 Mikan 首页获取官方标题)
      │      └── TMDB → tmdb_parser() (从 TMDB API 获取)
      └── 5. 写入 Bangumi 数据库
```

### 3.2 重命名流水线

```
RenameThread 循环(默认 60s)
  ── Renamer.rename()
      ├── 1. client.get_torrent_info() 获取所有已完成的 Bangumi 种子
      ├── 2. 并发获取所有种子的文件列表
      ├── 3. 批量查找偏移量 (batch_lookup_offsets)
      │      └── 查找顺序: qb_hash → tags("ab:ID") → title_raw → save_path
      ├── 4. 分类文件: media_list (mp4/mkv) + subtitle_list (ass/srt)
      ├── 5. 逐文件:
      │      ├── torrent_parser() 解析文件路径提取 SxxExx
      │      ├── gen_path() 生成新路径 (pn/advance/none 方法)
      │      └── client.rename_torrent_file() 调用 qBittorrent API
      └── 6. 发送通知
```

### 3.3 设置/订阅新番流程

```
搜索新番 → 用户选择 → 订阅:
  POST /api/v1/rss/subscribe
  ── SeasonCollector.subscribe_season()
      ├── 1. RSSAnalyser.link_to_data() 解析 RSS 链接
      ├── 2. RSSEngine.add_rss() 添加 RSS 源
      ├── 3. RSSEngine.download_bangumi() 立即下载所有匹配种子
      │      └── 使用 filter 过滤(默认排除 "720" 和 "\\d+-\\d+")
      └── 4. 写入 Bangumi 记录

整季补全 (eps_complete):
  ├── 条件: bangumi.eps_collect == false
  └── SeasonCollector.collect_season()
      ├── SearchTorrent.search_season() 搜索旧集数
      └── DownloadClient.add_torrent() 下载
```

---

## 四、模块详细说明

### 4.1 `module/conf/` — 配置管理

**文件**: `config.py` (128 行), `const.py` (136 行), `parse.py`, `log.py`, `search_provider.py`

- **`Settings` 类** 继承自定义 `Config` 模型 (`module.models.config.Config`，Pydantic `BaseModel` 子类)
- JSON 文件存储于 `config/config.json`（开发模式 `config_dev.json`）
- 支持通过 `AB_*` 环境变量覆盖（定义在 `const.py:ENV_TO_ATTR`）
- 配置段（9 个）:
  - `program`: RSS/重命名间隔, WebUI 端口
  - `downloader`: 类型/主机/用户名/密码/SSL
  - `rss_parser`: 启用/过滤规则/语言
  - `bangumi_manage`: 启用/重命名方法/ep 补全/组标签/删除坏种子
  - `log`: 调试模式
  - `proxy`: HTTP/SOCKS5 代理（支持 `$VAR` 环境变量展开）
  - `notification`: 启用 + 提供者列表
  - `experimental_openai`: ChatGPT 解析配置
  - `security`: 登录白名单/MCP 白名单/令牌
- 旧配置迁移 (`_migrate_old_config`): 处理 3.1.x → 3.2.x 字段重命名

### 4.2 `module/database/` — 数据持久化

**核心文件**: `combine.py` (344 行), `bangumi.py` (693 行), `engine.py` (30 行)

- **引擎**: SQLite + SQLAlchemy (同步 + 异步双引擎)
- **ORM**: SQLModel（Pydantic + SQLAlchemy 混合）
- **Database 类** 继承 `sqlmodel.Session`，聚合 5 个子数据访问对象
- **9 个 schema 迁移版本**（手动管理 `MIGRATIONS` 列表）
- **NULL 值填充**: 启动时自动按模型默认值填充 NULL 列
- **TTL 缓存**: `search_all()` 结果缓存 5 分钟，通过全局变量 `_bangumi_cache` 实现
- **5 表模型**:
  | 表 | 关键字段 | 用途 |
  |-----|----------|------|
  | `bangumi` | id, official_title, title_raw, season, season_offset, episode_offset, group_name, dpi, source, subtitle, filter, rss_link, poster_link, added, rule_name, save_path, deleted, archived, air_weekday, weekday_locked, needs_review, needs_review_reason, suggested_*_offset, title_aliases (JSON) | 番剧订阅主体 |
  | `rssitem` | id, name, url, aggregate, parser, enabled, connection_status, last_checked_at, last_error | RSS 源配置 |
  | `torrent` | id, bangumi_id (FK), rss_id (FK), name, url, homepage, downloaded, qb_hash | 种子记录 |
  | `user` | id, username, password (bcrypt) | 用户认证 |
  | `passkey` | id, user_id (FK), credential_id, public_key, sign_count, aaguid, transports, created_at, last_used_at, backup_eligible, backup_state | WebAuthn 凭证 |

**BangumiDatabase 关键方法**:
- `find_semantic_duplicate()`: 语义重复检测（同番不同字幕组命名）
- `add_title_alias()`: 标题别名管理（处理季中改名）
- `match_torrent()`: 种子名 → Bangumi 匹配（最长子串优先）
- `match_list()`: 批量匹配 + 自动更新 rss_link
- `get_needs_review()` / `set_needs_review()` / `clear_needs_review()`: 偏移量审查

### 4.3 `module/network/` — HTTP 网络层

**文件**: `request_url.py` (153 行), `request_contents.py` (83 行), `site/mikan.py` (26 行)

- **共享客户端**: 模块级单例 `_shared_client` (httpx.AsyncClient)
  - 代理配置变化时自动重建
  - 支持 HTTP + SOCKS5 代理 (`httpx_socks`)
- **RequestURL**: 基础 HTTP 请求方法 (GET/POST/HEAD)，3 次重试
- **RequestContent** (继承 RequestURL):
  - `get_torrents()`: RSS XML → 解析 → Torrent 列表
  - `get_xml()`: XML 解析（`xml.etree.ElementTree`）
  - `get_json()` / `post_json()`: JSON API 调用
  - `get_content()`: 二进制内容下载
- **Mikan RSS 解析**: `rss_parser()` 提取 title + enclosure URL + homepage
- User-Agent 伪装绕过 Cloudflare

### 4.4 `module/parser/` — 解析引擎

**编排器**: `title_parser.py` (114 行)

#### 4.4.1 `raw_parser.py` (219 行) — 核心正则解析器

解析种子标题格式: `[字幕组] 标题 SxxExx [分辨率 编码...][字幕信息]`

流程:
1. `pre_process()`: `【】` → `[]`
2. `get_group()`: `[组名]` 提取
3. `TITLE_RE`: 主匹配 `(标题信息) (集数信息) (其他)`
4. `prefix_process()`: 去除「新番」「港澳台地区」等前缀
5. `season_process()`: 提取 Sxx/Season xx/第x季 → 季度数字
6. `name_process()`: 分割中/英/日文标题
7. `find_tags()`: 提取 字幕/分辨率/来源
8. 返回 `Episode` (title_en/zh/jp, season, episode, sub, group, resolution, source)

退避模式 `FALLBACK_EP_PATTERNS`: 当主正则不匹配时的备选方案。

#### 4.4.2 `tmdb_parser.py` (325 行) — TMDB API 客户端

- 搜索 TV show → 按 `genre_id=16` 过滤动画
- 获取详情: 季列表、集数、海报、状态
- **虚拟季度检测**: 通过播出日期跨度（默认 >6 月）识别 cour 分割
- **集数偏移计算**: `get_offset_for_season()` 累加前季总集数
- LRU 缓存 (512 条)
- 海报图片下载 + 本地缓存

#### 4.4.3 `mikan_parser.py` — Mikan 首页解析

从 Mikan 番剧页面抓取官方中文标题 + 海报。

#### 4.4.4 `bgm_calendar.py` + `bgm_parser.py` — Bangumi.tv 集成

- 抓取 Bangumi.tv 每周放送表
- `match_weekday()`: 匹配番剧到对应星期

#### 4.4.5 `offset_detector.py` (135 行) — 偏移量检测

检测场景:
- 简单季度偏移: RSS S3 但 TMDB 只有 2 季 → season_offset = -1
- 虚拟季度偏移: RSS S2E01 实际是 TMDB S1E25 → episode_offset = 24
- 集数越界: 调整后集数超出 TMDB 记录

返回 `OffsetSuggestion` (season_offset, episode_offset, reason, confidence)。

#### 4.4.6 `openai.py` — ChatGPT 解析 (可选)

当 `experimental_openai.enable` 时，使用 ChatGPT 解析种子标题替代正则。

#### 4.4.7 `torrent_parser.py` — 文件级解析

从种子文件内的文件名提取 SxxExx 信息，用于重命名。

### 4.5 `module/rss/` — RSS 引擎

**文件**: `engine.py` (196 行), `analyser.py` (104 行)

#### RSSEngine (继承 Database)

- `refresh_rss(client, rss_id?)`: 主入口
  - 并发获取所有启用的 RSS items
  - 更新 `connection_status` | `last_checked_at` | `last_error`
  - `match_torrent()`: 种子名 → Bangumi 匹配
  - 匹配成功 → `client.add_torrent()` → 标记 `downloaded = true`
  - 写入 torrent 记录
- `add_rss()`: 添加新 RSS 源
- `download_bangumi()`: 手动下载指定 Bangumi 的所有种子
- Filter 机制: 正则/字面量双模式，结果缓存

#### RSSAnalyser (继承 TitleParser)

- `rss_to_data()`: RSS → 新 Bangumi 发现流程
- `torrent_to_data()`: 单 torrent 解析为 Bangumi
- `link_to_data()`: RSS 链接 → 第一条 Bangumi 数据

### 4.6 `module/downloader/` — 下载客户端

**文件**: `download_client.py` (260 行), `client/qb_downloader.py` (327 行), `path.py` (94 行)

#### DownloadClient (继承 TorrentPath)

工厂方法选择具体客户端:

```
settings.downloader.type → "qbittorrent" | "aria2" | "mock" | "transmission"
```

**关键方法**:
- `auth()`: 登录下载客户端
- `init_downloader()`: 设置 qBittorrent RSS 偏好 + 创建 Bangumi 分类
- `set_rule(data)`: 创建 qBittorrent RSS 自动下载规则
- `add_torrent()`: 支持 magnet + .torrent 文件, 自动加 `ab:<id>` tag
- `rename_torrent_file()`: 重命名客户端内文件
- `get_torrent_info()`: 查询已完成种子
- `move_torrent()`: 更新路径时移动种子

#### QbDownloader (327 行) — qBittorrent Web API

**API 端点封装**:
| 方法 | qBittorrent API | 用途 |
|------|----------------|------|
| `auth()` | `/api/v2/auth/login` | 登录 |
| `prefs_init()` | `/api/v2/app/setPreferences` | 设置 RSS 偏好 |
| `torrents_info()` | `/api/v2/torrents/info` | 查询种子列表 |
| `torrents_files()` | `/api/v2/torrents/files` | 查询种子文件 |
| `add_torrents()` | `/api/v2/torrents/add` | 添加种子 |
| `torrents_rename_file()` | `/api/v2/torrents/renameFile` | 重命名文件 |
| `rss_set_rule()` | `/api/v2/rss/setRule` | 设置 RSS 规则 |
| `torrents_delete()` | `/api/v2/torrents/delete` | 删除种子 |
| `move_torrent()` | `/api/v2/torrents/setLocation` | 移动种子路径 |
| `set_category()` | `/api/v2/torrents/setCategory` | 设置分类 |
| `add_tag()` | `/api/v2/torrents/addTags` | 添加标签 |

**关键细节**:
- 重命名验证: 3 次指数退避重试确认
- 添加种子: 3 次网络重试
- `@qb_connect_failed_wait` 装饰器处理连接失败

#### TorrentPath (94 行)

- `check_files()`: 分类 media vs subtitle
- `_gen_save_path()`: 生成 `{下载路径}/{标题} ({年份})/Season {调整后季度}`
- `_rule_name()`: 生成 qBittorrent 规则名
- `_path_to_bangumi()`: 从保存路径反推番剧名和季度

### 4.7 `module/manager/` — 业务编排

#### TorrentManager (371 行, 继承 Database)

**CRUD 操作**:
- `delete_rule()`: 删除番剧规则 + 可选删除种子文件
- `disable_rule()`: 软删除 (`deleted = true`)
- `enable_rule()`: 恢复软删除
- `update_rule()`: 更新 + 自动移动种子文件 + 更新 RSS 规则
- `archive_rule()` / `unarchive_rule()`: 归档/取消归档

**元数据操作**:
- `refresh_poster()`: 批量更新 TMDB 海报
- `refresh_calendar()`: 从 Bangumi.tv 更新放送日
- `refresh_metadata()`: 刷新 TMDB 元数据 + 自动归档已完结番剧
- `suggest_offset()`: 基于 TMDB 集数建议偏移量

#### Renamer (489 行, 继承 DownloadClient)

**重命名方法**:
| 方法 | 格式 | 适用 |
|------|------|------|
| `pn` | `{title} S{season}E{episode}{suffix}` | 默认 |
| `advance` | `{bangumi_name} S{season}E{episode}{suffix}` | 统一番剧名 |
| `none` | 原始路径 | 不重命名 |
| `subtitle_pn` | `{title} S{season}E{episode}.{language}{suffix}` | 字幕 |
| `subtitle_advance` | `{bangumi_name} S{season}E{episode}.{language}{suffix}` | 字幕 |

**批量偏移量查找** (`_batch_lookup_offsets`):
1. 通过 `qb_hash` 在 Torrent 表查找 bangumi_id
2. 通过 `tags("ab:ID")` 提取 bangumi_id
3. 通过 `torrent_name` 匹配 `title_raw`
4. 通过 `save_path` 匹配（带路径标准化）

**Pending Rename Cache**: 防重复命名，5 分钟冷却期。

**Bad Torrent 处理**: 解析失败时可选删除种子。

> 注: 实际方法名 `_batch_lookup_offsets`（私有方法，带下划线前缀）。

#### SeasonCollector (75 行, 继承 DownloadClient)

- `collect_season()`: 整季收集（搜索 + 下载 + 标记 eps_collect）
- `subscribe_season()`: 订阅新番
- `eps_complete()`: 模块级全局函数(非类方法)，遍历所有 `eps_collect == false` 的番剧进行补全

### 4.8 `module/notification/` — 通知系统

**文件**: `base.py` (51 行), `manager.py` (133 行), `providers/__init__.py` (40 行)

- **`NotificationProvider`**: 抽象基类 (继承 `RequestContent` + `ABC`)，定义 `send()` + `test()`
- **`NotificationManager`**: 
  - `_load_providers()`: 从配置实例化所有启用的提供者
  - `send_all()`: 并行发送到所有提供者
  - `test_provider()` / `test_provider_config()`: 测试
- **8 个提供者**: Telegram, Discord, Bark, ServerChan, WeCom, Gotify, Pushover, Webhook
- 通知消息格式: `番剧名称：{official_title}\n季度： 第{season}季\n更新集数： 第{episode}集`

### 4.9 `module/core/` — 后台任务系统

**文件**: `program.py` (176 行), `sub_thread.py` (204 行), `status.py` (73 行), `offset_scanner.py` (123 行)

#### 4 个 asyncio 后台循环

| 任务类 | 默认间隔 | 描述 |
|--------|---------|------|
| `RSSThread` | 900s (15min) | RSS 刷新 + 新番匹配 + 下载 |
| `RenameThread` | 60s (1min) | 已完成种子重命名 + 通知 |
| `OffsetScanThread` | 6h | TMDB 偏移量检测 + 标记审查 |
| `CalendarRefreshThread` | 24h | Bangumi.tv 放送表更新 |

#### Program 类 (多继承 MRO)

```
Program → RenameThread → RSSThread → OffsetScanThread → CalendarRefreshThread → ProgramStatus → Checker
```

**启动流程**:
1. `startup()`: 首次运行检测 → 数据库迁移 → 图片缓存 → `start()`
2. `start()`: 等待下载器 → 启动 4 个后台任务
3. `stop()`: 停止所有任务
4. `restart()`: stop + start

#### ProgramStatus (73 行)

属性代理:
- `is_running`: 任务已启动且非首次运行
- `downloader_status`: 60 秒 TTL 缓存
- `enable_rss` / `enable_renamer`: 对应配置开关
- `first_run` / `legacy_data` / `database` / `img_cache`: 系统状态

#### OffsetScanner (123 行)

- 遍历所有活跃（非删除非归档）Bangumi
- 调用 `tmdb_parser` 获取 TMDB 信息
- 调用 `detect_offset_mismatch()` (定义于 `parser/analyser/offset_detector.py`，offset_scanner.py 中仅导入使用) 检测偏移
- 标记 `needs_review` + 建议偏移值

### 4.10 `module/api/` — REST API 层

**文件**: `__init__.py` (29 行) + 11 个路由文件

- 全部挂载在 `/api/v1` 前缀下
- JWT 认证保护 (依赖 `get_current_user`)，setup 端点除外
- 业务逻辑委派到 `module/manager/*` 和 `module/rss/*`

| 路由 | 前缀 | 主要端点 |
|------|------|---------|
| `api/auth.py` | `/auth` | POST /login, GET /refresh_token, GET /logout, POST /update |
| `api/passkey.py` | `/passkey` | WebAuthn 注册/验证/列表/删除 |
| `api/bangumi.py` | `/bangumi` | get/all/{id}, update, delete, disable/enable, archive, offset detection, calendar, metadata, weekday |
| `api/config.py` | `/config` | GET /get, PATCH /update |
| `api/downloader.py` | `/downloader` | torrents CRUD, pause/resume/delete/tag |
| `api/log.py` | `/log` | GET logs, clear |
| `api/notification.py` | `/notification` | POST /test, /test-config |
| `api/program.py` | (root) | restart, start, stop, status, shutdown, check/downloader |
| `api/rss.py` | `/rss` | CRUD, enable/disable/many, refresh, analysis, collect, subscribe |
| `api/search.py` | `/search` | GET /bangumi (SSE), provider config |
| `api/setup.py` | `/setup` | status, test-downloader/rss/notification, complete |

**额外挂载点**:
- `/mcp/sse` — MCP SSE 服务器
- `/posters/{path}` — 海报文件服务
- `/assets`, `/images` — 前端静态文件 (生产模式)
- `/{path:path}` — SPA 兜底路由

### 4.11 `module/security/` — 认证安全层

**文件**: `jwt.py` (67 行), `webauthn.py`, `api.py`, `auth_strategy.py`

- **JWT**: python-jose (HS256), 24 小时过期, 密钥存储于 `config/.jwt_secret`
- **密码**: bcrypt (passlib)
- **Cookie 认证**: HttpOnly cookie + Bearer Token 双模式
- **IP 白名单**: `login_whitelist` CIDR 范围
- **Bearer 令牌**: `login_tokens` 绕过登录认证
- **WebAuthn Passkey**: 支持硬件密钥无密码认证

### 4.12 `module/mcp/` — AI 集成

**文件**: `server.py` (88 行), `tools.py` (359 行), `resources.py`, `security.py`

- **MCP (Model Context Protocol)**: SSE 传输, Starlette 子应用
- **10 个工具**:
  | 工具 | 描述 |
  |------|------|
  | `list_anime` | 列出所有追踪的番剧 |
  | `get_anime` | 获取单个番剧详情 |
  | `search_anime` | 搜索番剧种子 |
  | `subscribe_anime` | 订阅新番 |
  | `unsubscribe_anime` | 取消订阅/删除番剧 |
  | `list_downloads` | 查看下载状态 |
  | `list_rss_feeds` | 查看 RSS 源状态 |
  | `get_program_status` | 获取程序状态 |
  | `refresh_feeds` | 立即刷新 RSS |
  | `update_anime` | 更新番剧设置 |
- **安全**: IP CIDR 白名单 + Bearer Token

### 4.13 `module/searcher/` — 种子搜索

**文件**: `searcher.py` (81 行), `provider.py`

- **SSE 流式输出**: `analyse_keyword()` 使用 `yield` 实时返回搜索结果
- 支持站点: Mikan / DMHY / Nyaa
- TMDB 海报缓存 (`_poster_cache`)
- `special_url()`: 从 Bangumi 数据生成精准搜索 URL

### 4.14 `module/checker/` — 系统健康检查

- `check_downloader()`: qBittorrent WebUI 可达性 + 认证测试
- `check_database()`: 检查 data.db 是否存在
- `check_first_run()`: 通过 `.setup_complete` 哨兵文件判断
- `check_renamer()` / `check_analyser()`: 检查配置开关
- `check_img_cache()`: 检查海报目录

### 4.15 `module/update/` — 启动与迁移

**文件**: `startup.py` (21 行), `cross_version.py`, `data_migration.py`, `version_check.py`

- `first_run()`: 创建数据库表 + 默认管理员用户 + 海报目录
- `start_up()`: 运行迁移 + 确保默认用户
- `data_migration()`: 旧版数据迁移
- `cache_image()`: 下载初始海报缓存

---

## 五、类继承关系图

```
object
└── Checker (系统健康检查)
    └── ProgramStatus (运行状态)
        ├── RSSThread (RSS 循环)
        ├── RenameThread (重命名循环)
        ├── OffsetScanThread (偏移量扫描)
        └── CalendarRefreshThread (日历刷新)
            └── Program(RenameThread, RSSThread,        [MRO 多继承]
                  OffsetScanThread, CalendarRefreshThread)

sqlmodel.Session
├── RSSEngine (RSS 刷新 + 匹配)
└── TorrentManager (番剧管理 CRUD)

RequestURL (httpx HTTP 客户端)
└── RequestContent (XML/JSON/二进制)
    ├── NotificationProvider(RequestContent, ABC)       [MRO]
    │   ├── TelegramProvider
    │   ├── DiscordProvider
    │   ├── BarkProvider
    │   ├── ServerChanProvider
    │   ├── WecomProvider
    │   ├── GotifyProvider
    │   ├── PushoverProvider
    │   └── WebhookProvider
    └── SearchTorrent(RequestContent, RSSAnalyser)      [MRO]
         └── (RSSAnalyser 来自 TitleParser 分支)

TorrentPath (路径管理)
└── DownloadClient (下载客户端统一接口)
    ├── SeasonCollector (整季收集)
    └── Renamer (文件重命名)

TitleParser (解析编排器)
└── RSSAnalyser (RSS → Bangumi)
    └── SearchTorrent (见上方 RequestContent 分支)

组合引用 (非继承):
├── raw_parser (正则解析)
├── tmdb_parser (TMDB API)
├── mikan_parser (Mikan 首页)
├── bgm_calendar / bgm_parser (Bangumi.tv)
├── offset_detector(偏移量检测)
└── OpenAIParser (ChatGPT)
```

---

## 六、外部依赖与 Rust 替代方案

| Python 依赖 | 用途 | Rust 替代 |
|-------------|------|-----------|
| FastAPI + Uvicorn | Web 框架 | Axum + tokio |
| SQLModel / SQLAlchemy / aiosqlite | ORM + DB | sqlx / rusqlite + sea-orm |
| Pydantic v2 | 数据验证 | serde + validator |
| httpx | HTTP 客户端 | reqwest |
| python-jose | JWT | jsonwebtoken |
| passlib (bcrypt) | 密码哈希 | argon2 / bcrypt crate |
| httpx_socks | SOCKS 代理 | reqwest+socks |
| asyncio | 异步运行时 | tokio |
| python-dotenv | 环境变量 | dotenvy |
| xml.etree.ElementTree | XML 解析 | quick-xml / roxmltree |
| mcp (Python SDK) | MCP 协议 | Rust MCP SDK (mcp-core) |
| Jinja2 | 模板引擎 | (前端已分离, 不需要) |
| python-multipart | 文件上传 | axum-extra / multer |

---

## 七、建议分阶段迁移策略

### 阶段 0: 项目骨架
- Rust 项目结构 (Cargo workspace)
- CI/CD (GitHub Actions)
- 配置 cargo 依赖

### 阶段 1: 配置管理 (无外部依赖)
- 迁移 `module/conf/`: Config 结构体 + JSON 读/写 + 环境变量覆盖
- serde + config crate

### 阶段 2: 数据层
- 迁移 `module/database/` + `module/models/`
- 表结构定义 (struct) + 迁移系统 + CRUD 操作

### 阶段 3: 网络 + 解析引擎
- 迁移 `module/network/` + `module/parser/`
- reqwest 客户端 + 代理支持
- 正则解析器 (需要认真重写 raw_parser 的复杂正则)

### 阶段 4: 下载客户端
- 迁移 `module/downloader/`
- qBittorrent Web API 封装 (20+ 端点)
- Aria2 / Transmission 支持

### 阶段 5: RSS + 通知 + 搜索
- 迁移 `module/rss/` + `module/notification/` + `module/searcher/`

### 阶段 6: 业务编排
- 迁移 `module/manager/` (最复杂模块)
- Renamer (489 行逻辑) + TorrentManager (371 行)

### 阶段 7: 后台任务
- 迁移 `module/core/`
- 4 个 tokio 任务循环

### 阶段 8: Web API + 安全
- 迁移 `module/api/` + `module/security/`
- Axum 路由 + JWT + Passkey 认证

### 阶段 9: MCP AI 集成 (可最后)
- 迁移 `module/mcp/`
- SSE 传输 + 工具注册

### 阶段 10: 附属模块
- 迁移 `module/searcher/` + `module/checker/` + `module/update/`

---

## 八、关键算法和复杂逻辑总结

### 8.1 种子名称正则解析 (`raw_parser.py`)

输入: `[动漫国字幕组&LoliHouse] THE MARGINAL SERVICE - 08 [WebRip 1080p HEVC-10bit AAC][简繁内封字幕]`

输出:
```
title_en: "THE MARGINAL SERVICE"
title_zh: None
title_jp: None
season: 1
episode: 8
group: "动漫国字幕组&LoliHouse"
resolution: "1080p"
source: "WebRip"
sub: "简繁内封字幕"
```

### 8.2 虚拟季度检测 (`tmdb_parser.py:detect_virtual_seasons`)

当 TMDB 同一季内的连续两集播出日期跨度 >6 个月 → 视为虚拟季度边界。例如:
- TMDB Season 1 有 48 集, 但按播出日期分为 S1(1-28), S2(29-48)
- RSS 上看到 `S2E01` → 实际是 TMDB S1E29 → episode_offset = 28

### 8.3 语义重复检测 (`database/bangumi.py:find_semantic_duplicate`)

检测标准:
- 相同 `official_title`
- 相同 `dpi` / `subtitle` / `source`
- 相似 `group_name` (一个包含另一个, 或标准化后相同)
- 不同 `title_raw` → 作为别名合并, 不创建新条目

### 8.4 偏移量查找顺序 (`renamer.py:_batch_lookup_offsets`)

> 注意：实际方法名为私有 `_batch_lookup_offsets`（带 `_` 前缀），非 `batch_lookup_offsets`。

1. `qb_hash` → Torrent 表 → bangumi_id
2. `tags("ab:<bangumi_id>")` 直接提取
3. `torrent_name` 匹配 `title_raw` (最长子串优先)
4. `save_path` 匹配 (路径标准化: `\\` → `/`, 去尾部斜杠)

### 8.5 qBittorrent 重命名验证 (`qb_downloader.py:torrents_rename_file`)

重命名 API 返回 200 后:
1. 等待 0.1s → 查询文件列表
2. 如果还是旧名 → 等待 0.2s → 查询
3. 如果还是旧名 → 等待 0.4s → 查询
4. 最多 3 次指数退避
5. 仍不成功 → 返回 false (触发 pending rename cache)

### 8.6 配置环境变量展开 (`models/config.py:_expand`)

下载器/代理/通知的敏感字段存储为 `$VAR_NAME` 格式, 访问时通过 `expandvars()` 展开, 支持系统环境变量引用。

---

## 九、数据表字段参考

### bangumi 表 (SQLModel table)

```python
id: int (PK)
official_title: str
year: Optional[str]
title_raw: str (index)
season: int (default=1)
season_raw: Optional[str]
group_name: Optional[str]
dpi: Optional[str]
source: Optional[str]
subtitle: Optional[str]
eps_collect: bool (default=False)
episode_offset: int (default=0)
season_offset: int (default=0)
filter: str (default="720,\\d+-\\d+")
rss_link: str (default="")
poster_link: Optional[str]
added: bool (default=False)
rule_name: Optional[str]
save_path: Optional[str]
deleted: bool (default=False, index)
archived: bool (default=False, index)
air_weekday: Optional[int]
weekday_locked: bool (default=False)
needs_review: bool (default=False)
needs_review_reason: Optional[str]
suggested_season_offset: Optional[int]
suggested_episode_offset: Optional[int]
title_aliases: Optional[str]  # JSON: ["alt_title_1", "alt_title_2"]
```

---

*文档生成时间: 2026-06-24*
*基于 Auto_Bangumi commit: 最新版 backend/src/*
