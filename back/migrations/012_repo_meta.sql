CREATE TABLE repo_metadata (

    repository_id      UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    -- Git-derived cached stats
    commit_count       INTEGER NOT NULL DEFAULT 0,
    branch_count       INTEGER NOT NULL DEFAULT 0,
    tag_count          INTEGER NOT NULL DEFAULT 0,
    contributor_count  INTEGER NOT NULL DEFAULT 0,

    -- Activity stats (usually rolling windows)
    commits_last_7d    INTEGER NOT NULL DEFAULT 0,
    commits_last_30d   INTEGER NOT NULL DEFAULT 0,

    -- Issue/PR aggregates (from your app DB)
    issue_count        INTEGER NOT NULL DEFAULT 0,
    open_issue_count   INTEGER NOT NULL DEFAULT 0,

    pr_count           INTEGER NOT NULL DEFAULT 0,
    open_pr_count      INTEGER NOT NULL DEFAULT 0,

    -- Useful repo-level timestamps
    first_commit_at    TIMESTAMP,
    last_commit_at     TIMESTAMP,

    updated_at         TIMESTAMP NOT NULL DEFAULT NOW()
);
