use super::model::AppConfig;

fn validate_config(config: &AppConfig) -> Result<(), String> {
    if config.program.webui_port == 0 {
        return Err("webui_port must be non-zero".to_string());
    }
    if config.program.rss_time == 0 {
        return Err("rss_time must be non-zero".to_string());
    }
    if config.program.rename_time == 0 {
        return Err("rename_time must be non-zero".to_string());
    }
    Ok(())
}

pub fn parse_config(json: &str) -> Result<AppConfig, String> {
    let config: AppConfig = serde_json::from_str(json).map_err(|e| format!("config parse error: {e}"))?;
    validate_config(&config)?;
    Ok(config)
}

pub fn serialize_config(config: &AppConfig) -> Result<String, String> {
    serde_json::to_string_pretty(config).map_err(|e| format!("config serialize error: {e}"))
}
