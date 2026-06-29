use std::path::Path;
use crate::config::model::AppConfig;
use crate::config::consts;

pub struct Checker;

impl Checker {
    pub fn check_renamer() -> bool {
        crate::config::SETTINGS.bangumi_manage().enable
    }

    pub fn check_analyser() -> bool {
        crate::config::SETTINGS.rss_parser().enable
    }

    pub fn check_first_run() -> bool {
        let setup_complete = Path::new("config/.setup_complete").exists();
        if setup_complete {
            return false;
        }
        let default = AppConfig::default();
        let current = crate::config::SETTINGS.get();
        *current == default
    }

    pub fn check_database() -> bool {
        Path::new("data/data.db").exists()
    }

    pub fn check_img_cache() -> bool {
        let path = Path::new(consts::POSTERS_PATH);
        if !path.exists() {
            std::fs::create_dir_all(path).ok();
            false
        } else {
            true
        }
    }
}
