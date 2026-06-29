pub mod version_check;
pub mod migration;

use crate::config::consts;

pub fn start_up() -> Result<(), String> {
    tracing::info!("Starting up bangumi-follower");
    Ok(())
}

pub fn first_run() -> Result<(), String> {
    start_up()?;
    std::fs::create_dir_all(consts::POSTERS_PATH).map_err(|e| format!("create posters dir failed: {e}"))?;
    Ok(())
}
