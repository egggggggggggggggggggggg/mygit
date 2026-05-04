
CREATE TYPE pr_state AS ENUM ('open', 'closed', 'merged');

CREATE TABLE pull_requests (
    id              UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id   UUID          NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    author_id       UUID          REFERENCES users(id) ON DELETE SET NULL,
    title           VARCHAR(255)  NOT NULL,
    body            TEXT,
    state           pr_state      NOT NULL DEFAULT 'open',
    number          INT           NOT NULL,  -- repo-scoped, shares sequence with issues
    head_branch_id  UUID          REFERENCES branches(id) ON DELETE SET NULL,
    base_branch_id  UUID          REFERENCES branches(id) ON DELETE SET NULL,
    merged_at       TIMESTAMP,
    closed_at       TIMESTAMP,
    created_at      TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
 
    UNIQUE (repository_id, number)
);

