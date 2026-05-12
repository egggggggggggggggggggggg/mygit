CREATE TYPE comment_target AS ENUM ('issue', 'pull_request');
CREATE TABLE comments (
    id          UUID            PRIMARY KEY DEFAULT gen_random_uuid(),
    author_id   UUID            REFERENCES users(id) ON DELETE SET NULL,
    target_type comment_target  NOT NULL,
    target_id   UUID            NOT NULL,  -- FK enforced in application layer
    body        TEXT            NOT NULL,
    created_at  TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);
