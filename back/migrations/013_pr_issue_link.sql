CREATE TYPE pr_issue_link_type AS ENUM (
    'references',
    'closes'
);

CREATE TABLE pull_request_issue_links (
    pull_request_id UUID NOT NULL
        REFERENCES pull_requests(id) ON DELETE CASCADE,

    issue_id UUID NOT NULL
        REFERENCES issues(id) ON DELETE CASCADE,
    link_type pr_issue_link_type NOT NULL,

    PRIMARY KEY (pull_request_id, issue_id)
);

