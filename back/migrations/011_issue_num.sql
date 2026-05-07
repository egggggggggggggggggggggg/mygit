ALTER TABLE repositories
ADD COLUMN next_issue_number INTEGER NOT NULL DEFAULT 1;
