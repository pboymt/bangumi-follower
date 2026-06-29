pub const TMDB_API: &str = "32b19d6a05b512190a056fa4e747cbbc";
pub const DATA_PATH: &str = "sqlite:///data/data.db";
pub const VERSION_PATH: &str = "config/version.info";
pub const POSTERS_PATH: &str = "data/posters";
pub const LEGACY_DATA_PATH: &str = "data/data.json";

pub fn detect_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Unix"
    }
}

pub fn app_version() -> &'static str {
    let v = crate::VERSION;
    if v.is_empty() { "DEV_VERSION" } else { v }
}
