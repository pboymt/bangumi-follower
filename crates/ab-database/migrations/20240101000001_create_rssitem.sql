CREATE TABLE IF NOT EXISTS rssitem (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT,
    url TEXT NOT NULL,
    aggregate INTEGER DEFAULT 0,
    parser TEXT DEFAULT 'mikan',
    enabled INTEGER DEFAULT 1,
    connection_status TEXT,
    last_checked_at TEXT,
    last_error TEXT
);
CREATE INDEX IF NOT EXISTS ix_rssitem_url ON rssitem(url);
