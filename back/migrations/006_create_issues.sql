
CREATE TYPE issue_state AS ENUM ('open', 'closed');

CREATE TABLE issues (
    id             UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id  UUID          NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    author_id      UUID          REFERENCES users(id) ON DELETE SET NULL,
    assignee_id    UUID          REFERENCES users(id) ON DELETE SET NULL,
    title          VARCHAR(255)  NOT NULL,
    body           TEXT,
    state          issue_state   NOT NULL DEFAULT 'open',
    number         INT           NOT NULL,  -- repo-scoped human-readable number (#1, #2 …)
    closed_at      TIMESTAMP,
    created_at     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
 
    UNIQUE (repository_id, number)
);

