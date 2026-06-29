CREATE TABLE IF NOT EXISTS bangumi (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    official_title TEXT NOT NULL DEFAULT '',
    year TEXT,
    title_raw TEXT NOT NULL DEFAULT '',
    season INTEGER NOT NULL DEFAULT 1,
    season_raw TEXT,
    group_name TEXT,
    dpi TEXT,
    source TEXT,
    subtitle TEXT,
    eps_collect INTEGER NOT NULL DEFAULT 0,
    episode_offset INTEGER NOT NULL DEFAULT 0,
    season_offset INTEGER NOT NULL DEFAULT 0,
    filter TEXT NOT NULL DEFAULT '720,\\d+-\\d+',
    rss_link TEXT NOT NULL DEFAULT '',
    poster_link TEXT,
    added INTEGER NOT NULL DEFAULT 0,
    rule_name TEXT,
    save_path TEXT,
    deleted INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    air_weekday INTEGER,
    weekday_locked INTEGER NOT NULL DEFAULT 0,
    needs_review INTEGER NOT NULL DEFAULT 0,
    needs_review_reason TEXT,
    suggested_season_offset INTEGER,
    suggested_episode_offset INTEGER,
    title_aliases TEXT
);
CREATE INDEX IF NOT EXISTS ix_bangumi_title_raw ON bangumi(title_raw);
CREATE INDEX IF NOT EXISTS ix_bangumi_deleted ON bangumi(deleted);
CREATE INDEX IF NOT EXISTS ix_bangumi_archived ON bangumi(archived);
