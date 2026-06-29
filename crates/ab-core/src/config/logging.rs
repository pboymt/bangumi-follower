use tracing_subscriber::EnvFilter;

pub fn init_logging(debug_enable: bool) {
    let level = if debug_enable { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(format!(
            "bangumi_follower={level},ab_core={level}"
        )))
        .with_target(true)
        .init();
}
