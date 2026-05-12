-- Won't actually use this yet, don't see a need for it as this is a school project. 
-- If performance is needed we could add more here later on. 
CREATE INDEX idx_repositories_owner_id
ON repositories(owner_id);
CREATE INDEX idx_repositories_forked_from_id
ON repositories(forked_from_id);

