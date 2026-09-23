PRAGMA foreign_keys = ON;

CREATE TABLE user (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    email_verified INTEGER NOT NULL DEFAULT 0,
    image TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE session (
    id TEXT PRIMARY KEY NOT NULL,
    expires_at INTEGER NOT NULL,
    token TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE
);
CREATE INDEX session_user_id_idx ON session(user_id);

CREATE TABLE account (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    access_token TEXT,
    refresh_token TEXT,
    id_token TEXT,
    access_token_expires_at INTEGER,
    refresh_token_expires_at INTEGER,
    scope TEXT,
    password TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX account_user_id_idx ON account(user_id);

CREATE TABLE verification (
    id TEXT PRIMARY KEY NOT NULL,
    identifier TEXT NOT NULL,
    value TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    created_at INTEGER,
    updated_at INTEGER
);
CREATE INDEX verification_identifier_idx ON verification(identifier);

CREATE TABLE sync_entities (
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    entity_kind TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('upsert', 'delete')),
    version INTEGER NOT NULL CHECK (version > 0),
    cursor INTEGER NOT NULL CHECK (cursor > 0),
    server_timestamp_unix_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    PRIMARY KEY (user_id, entity_kind, entity_id)
);
CREATE INDEX sync_entities_user_cursor_idx ON sync_entities(user_id, cursor);

CREATE TABLE sync_changes (
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    cursor INTEGER NOT NULL CHECK (cursor > 0),
    entity_kind TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('upsert', 'delete')),
    version INTEGER NOT NULL CHECK (version > 0),
    server_timestamp_unix_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    PRIMARY KEY (user_id, cursor)
);
CREATE INDEX sync_changes_user_entity_idx
    ON sync_changes(user_id, entity_kind, entity_id, version);

CREATE TABLE sync_mutations (
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    mutation_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    entity_kind TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    cursor INTEGER NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY (user_id, mutation_id)
);
CREATE INDEX sync_mutations_user_cursor_idx ON sync_mutations(user_id, cursor);

CREATE TABLE tts_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'completed', 'failed')),
    input_text TEXT,
    voice_id TEXT NOT NULL,
    speed REAL NOT NULL,
    audio_key TEXT,
    error_code TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
CREATE INDEX tts_jobs_user_created_idx ON tts_jobs(user_id, created_at_unix_ms DESC);
