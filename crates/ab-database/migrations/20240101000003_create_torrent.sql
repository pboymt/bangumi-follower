CREATE TABLE IF NOT EXISTS torrent (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    refer_id INTEGER REFERENCES bangumi(id),
    rss_id INTEGER REFERENCES rssitem(id),
    name TEXT,
    url TEXT NOT NULL,
    homepage TEXT,
    downloaded INTEGER NOT NULL DEFAULT 0,
    qb_hash TEXT
);
CREATE INDEX IF NOT EXISTS ix_torrent_url ON torrent(url);
CREATE INDEX IF NOT EXISTS ix_torrent_qb_hash ON torrent(qb_hash);
