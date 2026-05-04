CREATE TABLE branches (
    id             UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id  UUID          NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    name           VARCHAR(255)  NOT NULL,
    created_at     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
 
    UNIQUE (repository_id, name)
);

