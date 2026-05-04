CREATE TYPE repo_role AS ENUM ('read', 'write', 'admin');

CREATE TABLE repository_collaborators (
    repository_id  UUID       NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    user_id        UUID       NOT NULL REFERENCES users(id)         ON DELETE CASCADE,
    role           repo_role  NOT NULL DEFAULT 'read',
    created_at     TIMESTAMP  NOT NULL DEFAULT CURRENT_TIMESTAMP,
 
    PRIMARY KEY (repository_id, user_id)
);

