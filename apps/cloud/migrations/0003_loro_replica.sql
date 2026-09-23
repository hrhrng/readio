CREATE TABLE replica_changes (
    cursor INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    change_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    change_blob BLOB NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length > 0),
    created_at_unix_ms INTEGER NOT NULL,
    UNIQUE (user_id, change_id)
);
CREATE INDEX replica_changes_user_cursor_idx
    ON replica_changes(user_id, cursor);

CREATE TABLE replica_devices (
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL,
    ack_cursor INTEGER NOT NULL CHECK (ack_cursor >= 0),
    heads_blob BLOB NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY (user_id, device_id)
);
CREATE INDEX replica_devices_user_ack_cursor_idx
    ON replica_devices(user_id, ack_cursor);

CREATE TABLE replica_snapshots (
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    cursor INTEGER NOT NULL CHECK (cursor >= 0),
    object_key TEXT NOT NULL,
    etag TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length > 0),
    created_by_device_id TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY (user_id, cursor)
);
CREATE INDEX replica_snapshots_user_cursor_idx
    ON replica_snapshots(user_id, cursor DESC);
