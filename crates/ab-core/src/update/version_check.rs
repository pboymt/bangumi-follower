use std::path::Path;
use crate::config::consts;

struct VersionInfo {
    version: String,
}

fn read_version_info() -> Option<VersionInfo> {
    let path = Path::new(consts::VERSION_PATH);
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    Some(VersionInfo { version: content.trim().to_string() })
}

fn write_version_info(version: &str) {
    let path = Path::new(consts::VERSION_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(path, version).ok();
}

pub fn version_check() -> (bool, Option<u64>) {
    let current = crate::VERSION;
    if current.is_empty() || current == "DEV_VERSION" {
        write_version_info(current);
        return (true, None);
    }
    match read_version_info() {
        Some(info) if info.version == current => (true, None),
        Some(info) => {
            let old_minor = info.version.split('.').nth(1).and_then(|s| s.parse::<u64>().ok());
            write_version_info(current);
            (false, old_minor)
        }
        None => {
            write_version_info(current);
            (false, None)
        }
    }
}
