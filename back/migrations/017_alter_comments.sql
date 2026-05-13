CREATE INDEX comments_target_idx
ON comments(repository_id, target_type, target_id, created_at, id);
ALTER TABLE comments
ALTER COLUMN created_at
TYPE TIMESTAMPTZ 
USING created_at AT TIME ZONE 'UTC';

ALTER TABLE comments
ALTER COLUMN updated_at
TYPE TIMESTAMPTZ
USING updated_at AT TIME ZONE 'UTC';
-- lwk useless to specify updating to 'UTC' as the db has like zero data, but its good practice to indicate. 
