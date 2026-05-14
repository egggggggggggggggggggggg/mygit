CREATE TABLE repositories (
    id              UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id        UUID          NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name            VARCHAR(100)  NOT NULL,
    description     TEXT,
    is_private      BOOLEAN       NOT NULL DEFAULT FALSE,
    default_branch  VARCHAR(255)  NOT NULL DEFAULT 'main',
    forked_from_id  UUID          REFERENCES repositories(id) ON DELETE SET NULL,
    created_at      TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (owner_id, name)
);
--Could possibly add a commits table as a sorta semi-cache for displaying repo history
--Avoids file reading which is slower when the whole data isn't really needed.   
   
