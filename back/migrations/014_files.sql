CREATE TABLE files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    uploader_id UUID REFERENCES users(id) ON DELETE SET NULL,

    storage_key TEXT NOT NULL UNIQUE,
    original_filename TEXT NOT NULL,

    mime_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
-- include a hash field with type BYTEA maybe  
