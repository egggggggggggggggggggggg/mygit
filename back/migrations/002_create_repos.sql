CREATE TABLE repos (
     id              UUID PRIMARY KEY,
     owner_id        UUID NOT NULL REFERENCES users(id),

     name            TEXT NOT NULL,
     description     TEXT,

     visibility      TEXT NOT NULL CHECK (visibility IN ('public', 'private', 'internal')),

     default_branch  TEXT DEFAULT 'main',

     created_at      TIMESTAMP DEFAULT NOW(),
     updated_at      TIMESTAMP DEFAULT NOW()
);
