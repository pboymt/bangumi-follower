# Stage 1: 配置管理 — 详细实施计划

> 迁移 `module/conf/` + `module/checker/` + `module/update/` 到 `ab-core` crate。
> 核心目标: Config struct 定义、JSON 读/写、环境变量覆盖、系统检查、启动迁移。

---

## 一、范围

### 1.1 需要迁移的 Python 源文件

| Python 文件 | 行数 | Rust 目标模块 | 说明 |
|------------|------|--------------|------|
| `models/config.py` | 266 | `config::model` | 9 个配置子结构体 + 根 Config |
| `conf/config.py` | 128 | `config::settings` | Settings 单例, load/save/init/migrate |
| `conf/const.py` | 136 | `config::const` | DEFAULT_SETTINGS, ENV_TO_ATTR |
| `conf/parse.py` | — | `config::parser` | JSON 解析辅助 |
| `conf/log.py` | — | `config::logging` | 日志初始化 |
| `conf/search_provider.py` | — | `config::search` | 搜索站点配置 |
| `conf/uvicorn_logging.py` | — | `config::logging` | UVicorn 风格日志 |
| `checker/checker.py` | 101 | `checker` | 系统健康检查 |
| `update/startup.py` | 21 | `update` | first_run, start_up |
| `update/cross_version.py` | 63 | `update::migration` | 跨版本迁移 |
| `update/data_migration.py` | 24 | `update::data_migration` | 数据迁移 |
| `update/version_check.py` | — | `update::version_check` | 版本检查 |

### 1.2 不迁移的部分

- `module/update/rss.py` — 与 RSS 引擎耦合，留在后续阶段
- `conf/__init__.py` — 定义模块级常量（`TMDB_API`、`DATA_PATH`、`PLATFORM` 等），这些常量放入 Rust 的 `config::const`

---

## 二、需要创建/修改的文件

### 2.1 新建文件

```
crates/ab-core/
├── Cargo.toml                     # 已有骨架 (Stage 0)
└── src/
    ├── lib.rs                     # 修改，添加模块声明
    ├── config/
    │   ├── mod.rs                 # pub mod + Settings 单例
    │   ├── model.rs               # 9 个子 struct + Config 根 struct
    │   ├── const.rs               # 默认值常量 + env-to-attr 映射
    │   ├── parser.rs              # JSON 加载/保存逻辑
    │   └── logging.rs             # 日志初始化 (tracing)
    ├── checker/
    │   ├── mod.rs                 # Checker 结构体
    │   └── downloader.rs          # downloader 健康检查 (占位, 依赖 reqwest)
    └── update/
        ├── mod.rs                 # startup + migration 编排
        ├── migration.rs           # 跨版本迁移
        └── version_check.rs       # 版本检查
```

### 2.2 总计文件数

- `Cargo.toml`: 1 (已有)
- `src/lib.rs`: 1 (修改)
- `src/config/*.rs`: 5
- `src/checker/*.rs`: 2
- `src/update/*.rs`: 3
- **总计: 12 个文件**

---

## 三、Config 模型设计 (对应 python `models/config.py`)

### 3.1 设计原则

- 所有 struct 派生 `Serialize, Deserialize, Debug, Clone, PartialEq`
- 使用 `serde_json::Value` 处理 `$VAR` 环境变量展开
- 使用 `serde(alias = "...")` 处理字段别名
- 默认值通过 `#[serde(default)]` + `Default` trait 实现
- 验证使用 `validator` crate 或自定义 `fn validate()`

### 3.2 Rust struct 定义

```rust
// === config/model.rs ===

/// 调度定时设置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Program {
    #[serde(default = "default_rss_time")]
    pub rss_time: u64,           // 900s
    #[serde(default = "default_rename_time")]
    pub rename_time: u64,        // 60s
    #[serde(default = "default_webui_port")]
    pub webui_port: u16,         // 7892
}

/// 下载客户端连接设置 (支持 $VAR 展开)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Downloader {
    #[serde(default)]
    pub r#type: DownloaderType,  // qbittorner | aria2 | mock | transmission
    #[serde(alias = "host", default)]
    pub host_: String,
    #[serde(alias = "username", default)]
    pub username_: String,
    #[serde(alias = "password", default)]
    pub password_: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub ssl: bool,
}
// 每个 credential 提供 getter: pub fn host(&self) -> String { expand(&self.host_) }

/// RSS 解析设置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RssParser {
    #[serde(default = "default_true")]
    pub enable: bool,
    #[serde(default)]
    pub filter: Vec<String>,     // ["720", "\\d+-\\d+"]
    #[serde(default = "default_language")]
    pub language: String,        // "zh"
}

/// 番剧管理设置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BangumiManage {
    #[serde(default = "default_true")]
    pub enable: bool,
    #[serde(default)]
    pub eps_complete: bool,
    #[serde(default = "default_rename_method")]
    pub rename_method: String,  // "pn"
    #[serde(default)]
    pub group_tag: bool,
    #[serde(default)]
    pub remove_bad_torrent: bool,
}

/// 日志设置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Log {
    #[serde(default)]
    pub debug_enable: bool,
}

/// 代理设置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Proxy {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub r#type: ProxyType,       // http | socks
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(alias = "username", default)]
    pub username_: String,
    #[serde(alias = "password", default)]
    pub password_: String,
}
// getter: pub fn username(&self) -> String { expand(&self.username_) }

/// 通知提供者配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotificationProvider {
    pub r#type: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(alias = "token", default)]
    pub token_: Option<String>,
    #[serde(alias = "chat_id", default)]
    pub chat_id_: Option<String>,
    #[serde(alias = "webhook_url", default)]
    pub webhook_url_: Option<String>,
    #[serde(alias = "server_url", default)]
    pub server_url_: Option<String>,
    #[serde(alias = "device_key", default)]
    pub device_key_: Option<String>,
    #[serde(alias = "user_key", default)]
    pub user_key_: Option<String>,
    #[serde(alias = "api_token", default)]
    pub api_token_: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(alias = "url", default)]
    pub url_: Option<String>,
}
// 每个提供 getter

/// 通知系统设置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Notification {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub providers: Vec<NotificationProvider>,
}

/// 实验性 OpenAI 设置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperimentalOpenAi {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_openai_base")]
    pub api_base: String,
    #[serde(default)]
    pub api_type: OpenAiType,    // openai | azure
    #[serde(default = "default_openai_version")]
    pub api_version: String,
    #[serde(default = "default_openai_model")]
    pub model: String,
    #[serde(default)]
    pub deployment_id: String,
}

/// 安全设置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Security {
    #[serde(default)]
    pub login_whitelist: Vec<String>,
    #[serde(default)]
    pub login_tokens: Vec<String>,
    #[serde(default = "default_mcp_whitelist")]
    pub mcp_whitelist: Vec<String>,
    #[serde(default)]
    pub mcp_tokens: Vec<String>,
}

fn default_mcp_whitelist() -> Vec<String> {
    vec![
        "192.168.0.0/16".into(),
        "10.0.0.0/8".into(),
        "172.16.0.0/12".into(),
        "127.0.0.1/32".into(),
    ]
}

/// 根配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    #[serde(default)]
    pub program: Program,
    #[serde(default)]
    pub downloader: Downloader,
    #[serde(default = "default_rss_parser")]
    pub rss_parser: RssParser,
    #[serde(default)]
    pub bangumi_manage: BangumiManage,
    #[serde(default)]
    pub log: Log,
    #[serde(default)]
    pub proxy: Proxy,
    #[serde(default)]
    pub notification: Notification,
    #[serde(default)]
    pub experimental_openai: ExperimentalOpenAi,
    #[serde(default)]
    pub security: Security,
}
```

### 3.3 需要定义的枚举

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DownloaderType {
    Qbittorrent,
    Aria2,
    Mock,
    Transmission,
}
// Default → Qbittorrent

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    Http,
    Socks,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OpenAiType {
    Openai,
    Azure,
}
```

### 3.4 模块级常量 (对应 Python `conf/__init__.py`)

```rust
// === config/const.rs ===

/// TMDB API 密钥 (Python: conf/__init__.py:8)
pub const TMDB_API: &str = "32b19d6a05b512190a056fa4e747cbbc";

/// SQLite 数据库连接串 (Python: conf/__init__.py:9)
pub const DATA_PATH: &str = "sqlite:///data/data.db";

/// 平台检测 (Python: conf/__init__.py:14)
pub fn detect_platform() -> &'static str {
    if cfg!(target_os = "windows") { "Windows" } else { "Unix" }
}

/// 配置路径常量
pub const VERSION_PATH: &str = "config/version.info";
pub const POSTERS_PATH: &str = "data/posters";
pub const LEGACY_DATA_PATH: &str = "data/data.json";

/// VERSION 获取 (Python: module.__version__ → fallback "DEV_VERSION")
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
// 如果编译时未设置, 回退到 "DEV_VERSION"
pub fn app_version() -> &'static str {
    if VERSION.is_empty() { "DEV_VERSION" } else { VERSION }
}
```

### 3.5 环境变量展开函数

```rust
/// 展开 `$VAR` / `${VAR}` 环境变量引用
/// 对应 Python `_expand()` 和 `os.path.expandvars()`
pub fn expand(value: &str) -> String {
    // 使用 regex 或 shell-words 风格解析 $VAR 引用
    // 或者用 std::env::var() + 手动替换
}
```

---

## 四、Settings 单例 (对应 Python `conf/config.py`)

```rust
// === config/mod.rs ===

use std::sync::RwLock;
use std::path::PathBuf;

pub struct Settings {
    inner: RwLock<AppConfig>,
    config_path: PathBuf,
}

impl Settings {
    /// 加载 JSON → 迁移 → 验证 → 写入
    pub fn load() -> Result<Self>;

    /// 写入 JSON 到文件
    pub fn save(&self) -> Result<()>;

    /// 从环境变量引导初始配置 (无 JSON 文件时)
    pub fn init() -> Result<Self>;

    /// 旧配置迁移 (3.1.x → 3.2.x)
    fn migrate_old_config(config: &mut serde_json::Value);

    /// 从环境变量覆盖 `ENV_TO_ATTR` 映射
    fn load_from_env() -> AppConfig;

    /// 访问内部配置
    pub fn get(&self) -> std::sync::RwLockReadGuard<AppConfig>;
    pub fn set(&self, config: AppConfig) -> Result<()>;

    /// 快捷访问各个子段
    pub fn program(&self) -> Program;
    pub fn downloader(&self) -> Downloader;
    // ... 等
}

// 全局单例 (与 Python `settings` 实例一致)
pub static SETTINGS: Lazy<Settings> = Lazy::new(|| Settings::load().expect("Failed to load config"));
```

### 4.1 配置路径

| 环境 | 路径 |
|------|------|
| Debug | `config/config_dev.json` |
| Release | `config/config.json` |

### 4.2 环境变量映射 (ENV_TO_ATTR)

| 环境变量 | 配置路径 | 类型转换 |
|---------|---------|---------|
| `AB_INTERVAL_TIME` | `program.rss_time` | int |
| `AB_RENAME_FREQ` | `program.rename_time` | int |
| `AB_WEBUI_PORT` | `program.webui_port` | int |
| `AB_DOWNLOADER_HOST` | `downloader.host_` | string |
| `AB_DOWNLOADER_USERNAME` | `downloader.username_` | string |
| `AB_DOWNLOADER_PASSWORD` | `downloader.password_` | string |
| `AB_DOWNLOAD_PATH` | `downloader.path` | string |
| `AB_RSS_COLLECTOR` | `rss_parser.enable` | bool ("true"/"1") |
| `AB_NOT_CONTAIN` | `rss_parser.filter` | string.split("\|") |
| `AB_LANGUAGE` | `rss_parser.language` | string |
| `AB_RENAME` | `bangumi_manage.enable` | bool |
| `AB_METHOD` | `bangumi_manage.rename_method` | lower |
| `AB_GROUP_TAG` | `bangumi_manage.group_tag` | bool |
| `AB_EP_COMPLETE` | `bangumi_manage.eps_complete` | bool |
| `AB_REMOVE_BAD_BT` | `bangumi_manage.remove_bad_torrent` | bool |
| `AB_DEBUG_MODE` | `log.debug_enable` | bool |
| `AB_HTTP_PROXY` | `proxy.enable/type/host/port` | 复合解析 |
| `AB_SOCKS` | `proxy.*` | 复合解析 (host,port,username,password) |

---

## 五、Checker 模块 (对应 Python `checker/checker.py`)

```rust
// === checker/mod.rs ===

pub struct Checker;

impl Checker {
    /// 检查是否启用重命名 (bangumi_manage.enable)
    pub fn check_renamer() -> bool;

    /// 检查是否启用解析器 (rss_parser.enable)
    pub fn check_analyser() -> bool;

    /// 检查是否首次运行
    /// Python 逻辑 (checker/checker.py:42-45):
    ///   如果 config/.setup_complete 存在 → false (非首次)
    ///   否则 → settings 是否等于 Config() 默认值
    ///   两者皆为 true 才判定为首次运行
    pub fn check_first_run() -> bool;

    /// 检查版本变更
    /// 委托 update::version_check, 返回 (版本是否相同, 上一版小版本号)
    pub fn check_version() -> (bool, Option<u64>);

    /// 检查数据库 (data/data.db 存在)
    pub fn check_database() -> bool;

    /// 检查下载器连通性 (异步)
    pub async fn check_downloader() -> bool;

    /// 检查海报缓存 (data/posters 目录)
    /// Python (checker/checker.py:95-101): 如果目录不存在则创建它
    /// 返回值: true = 目录已存在, false = 刚创建
    pub fn check_img_cache() -> bool;
}
```

### 5.1 需要实现的功能

- `check_downloader()`: 依赖 `reqwest`，检测 qBittorrent WebUI 是否可达。
  如果 `settings.downloader.type == "mock"` 则直接返回 true (跳过网络检查)。
  Python 先做 GET 检查响应体是否含 `"qbittorrent"` / `"vuetorrent"`，再通过 `DownloadClient.authed` 验证。
- `check_first_run()`: 读 `config/.setup_complete` 哨兵文件。如果存在则返回 false (非首次)；
  否则比较当前 settings 是否等于 `Config().dict()` (默认配置的序列化值)。
  使用 `_get_default_config_dict()` 缓存默认值 (Python: checker/checker.py:13-20)。
- `check_version()`: 调用 `update::version_check` 模块
- `check_img_cache()`: 检查 `data/posters` 目录，如果不存在则创建 (Python: checker/checker.py:99-100)

---

## 六、Update 模块 (对应 Python `update/startup.py` + `cross_version.py`)

```rust
// === update/mod.rs ===

/// 启动入口 (Python update/startup.py:9-13):
/// 1. 创建 RSSEngine (sync)
/// 2. engine.create_table()
/// 3. engine.run_migrations()
/// 4. engine.user.add_default_user()
pub fn start_up() -> Result<()>;

/// 首次运行 (Python update/startup.py:16-21):
/// 1. 执行 start_up() 全部步骤
/// 2. POSTERS_PATH.mkdir(parents=true)
pub fn first_run() -> Result<()>;

/// 运行所有待处理的迁移 (Python: 委托到 engine.db.run_migrations())
pub fn run_migrations() -> Result<()>;

/// 缓存海报图片 (Python update/cross_version.py:52-63):
/// 遍历所有 bangumi, 通过 HTTP 获取海报, 保存到本地, 更新 poster_link
pub async fn cache_image(client: &NetworkClient) -> Result<()>;
```

### 6.1 Migration trait

```rust
// === update/migration.rs ===

pub trait Migration {
    fn version(&self) -> u64;
    fn description(&self) -> &'static str;
    fn run(&self, db: &mut Database) -> Result<()>;
}

/// 注册所有迁移
pub fn all_migrations() -> Vec<Box<dyn Migration>>;
```

### 6.2 跨版本迁移 (对应 Python `cross_version.py`)

```rust
// 3.0 → 3.1: 更新海报链接格式 + 添加 RSS 源
pub struct From30To31;
// 3.1 → 3.2: 运行数据库 schema 迁移
pub struct From31To32;
```

### 6.3 版本检查 (对应 Python `update/version_check.py`)

```rust
/// 从 version.info 文件检查版本变更
/// Python 返回 (is_same_version, last_minor_version):
///   True, None        — 版本相同或 DEV_VERSION, 无需迁移
///   False, Some(n)    — 版本变更 (n = 上一版 minor), 需要迁移
///   False, None       — 无 version.info 文件, 初次部署
pub fn version_check() -> (bool, Option<u64>);
```

---

## 七、依赖关系

### 7.1 新增 Cargo 依赖

| crate | 用途 | 是否在 workspace.dependencies 中 |
|-------|------|----------------------------------|
| `chrono` | 时间/日期类型 | 否 → 新增 |
| `serde` | 序列化 | 是 |
| `serde_json` | JSON | 是 |
| `config` | 配置加载 | 是 |
| `dotenvy` | .env 文件加载 | 是 |
| `thiserror` | 错误类型 | 是 |
| `tracing` | 日志 | 是 |
| `regex` | 环境变量展开 $VAR 解析 | 是 |

### 7.2 ab-core Cargo.toml 最终版本

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
regex.workspace = true
chrono = "0.4"

[features]
default = []
```

### 7.3 此阶段不需要的依赖

| 依赖 | 原因 |
|------|------|
| `reqwest` | Checker.downloader 检查需要，但这是异步 IO，放入阶段 3 |
| `tokio` | ab-core 应为同步/纯逻辑，异步留在上层 |
| `sqlx` | 数据库操作不在本阶段 |
| `jsonwebtoken` | 安全模块不在本阶段 |

---

## 八、实施步骤

```
Step 1:  在 crates/ab-core/src/ 下创建 config/ checker/ update/ 目录
Step 2:  编写 config/model.rs — 9 个配置子结构体 + AppConfig
Step 3:  编写 config/const.rs — 默认值常量 + ENV_TO_ATTR 映射
Step 4:  编写 config/parser.rs — JSON 加载/保存/迁移
Step 5:  编写 config/mod.rs — Settings 单例 + 环境变量覆盖
Step 6:  编写 checker/mod.rs — Checker 结构体 (check_downloader 留桩)
Step 7:  编写 update/mod.rs — start_up + first_run 入口
Step 8:  编写 update/migration.rs — Migration trait + 3.0→3.1, 3.1→3.2
Step 9:  编写 update/version_check.rs — 版本检查
Step 10: 更新 src/lib.rs — pub mod config; pub mod checker; pub mod update;
Step 11: 创建测试 — test_config.rs, test_checker.rs, 测试 JSON 序列化/反序列化
Step 12: 运行 cargo check && cargo test
```

---

## 九、测试计划

### 9.1 单元测试 (同一 crate 内)

| 测试 | 覆盖内容 |
|------|---------|
| `test_config_defaults` | 默认值与 Python 默认值一致 |
| `test_config_json_roundtrip` | serde JSON 序列化/反序列化一致性 |
| `test_config_env_override` | 环境变量覆盖配置字段 |
| `test_config_migration_31` | 3.1.x → 3.2.x 迁移逻辑 |
| `test_checker_renamer` | check_renamer 返回正确 |
| `test_checker_first_run` | check_first_run 逻辑 |
| `test_expand_env_var` | $VAR 环境变量展开 |

### 9.2 集成测试 (tests/ 目录)

| 测试 | 覆盖内容 |
|------|---------|
| `test_config_file_io` | 写入 JSON 文件 → 读取 → 验证 |
| `test_checker_upgrade_simulate` | 模拟新旧配置迁移场景 |

### 9.3 测试策略

- 所有测试不依赖外部网络，使用 `tempfile` 创建临时文件
- 环境变量测试使用 `temp_env::with_var` 或手动 `set_var`/`remove_var` 隔离
- 迁移测试: 写入旧格式 JSON → 加载 → 断言新字段正确

---

## 十、验收标准

- [ ] `cargo check` 零 warning 零 error
- [ ] `cargo test` 所有测试通过
- [ ] `AppConfig` 能用 serde 正确解析/生成 `config_dev.json` 格式
- [ ] `Settings` 单例从 JSON 文件加载，环境变量 `AB_*` 能覆盖对应字段
- [ ] 3.1.x 旧格式 JSON 能正确迁移到当前格式
- [ ] `Checker` 检查方法返回预期结果
- [ ] 所有 Python 默认值与 Rust 默认值一致
- [ ] 不引入 `reqwest`、`tokio` 等异步依赖
