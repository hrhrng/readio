CREATE TABLE device_code (
    id TEXT PRIMARY KEY NOT NULL,
    device_code TEXT NOT NULL UNIQUE,
    user_code TEXT NOT NULL UNIQUE,
    user_id TEXT REFERENCES user(id) ON DELETE CASCADE,
    client_id TEXT,
    scope TEXT,
    status TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'denied')),
    expires_at INTEGER NOT NULL,
    last_polled_at INTEGER,
    polling_interval INTEGER
);
CREATE INDEX device_code_user_id_idx ON device_code(user_id);
CREATE INDEX device_code_status_expires_idx ON device_code(status, expires_at);
