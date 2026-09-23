CREATE TABLE library_content (
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    item_id TEXT NOT NULL,
    variant TEXT NOT NULL CHECK (variant IN ('reader', 'source')),
    object_key TEXT NOT NULL,
    sha256_base64 TEXT NOT NULL CHECK (length(sha256_base64) = 44),
    etag TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length > 0),
    content_type TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY (user_id, item_id, variant)
);
CREATE INDEX library_content_user_updated_idx
    ON library_content(user_id, updated_at_unix_ms DESC);
