CREATE TABLE IF NOT EXISTS passkey (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES user(id),
    name VARCHAR(64) NOT NULL,
    credential_id VARCHAR NOT NULL UNIQUE,
    public_key VARCHAR NOT NULL,
    sign_count INTEGER DEFAULT 0,
    aaguid VARCHAR,
    transports VARCHAR,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    last_used_at TIMESTAMP,
    backup_eligible INTEGER DEFAULT 0,
    backup_state INTEGER DEFAULT 0
);
CREATE INDEX IF NOT EXISTS ix_passkey_user_id ON passkey(user_id);
CREATE UNIQUE INDEX IF NOT EXISTS ix_passkey_credential_id ON passkey(credential_id);
