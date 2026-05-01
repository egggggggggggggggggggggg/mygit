CREATE TABLE repository_collaborators (
     repo_id         UUID REFERENCES repositories(id),
     user_id         UUID REFERENCES users(id),
     role            TEXT NOT NULL CHECK (ROLE IN (
                       'read',
                       'write',
                       'admin'
                     )),
     added_at        TIMESTAMP DEFAULT now(),
     primary key (repo_id, user_id)
);
