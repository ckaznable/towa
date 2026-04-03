ALTER TABLE article_processing ADD COLUMN agent_id TEXT;
ALTER TABLE article_processing ADD COLUMN last_error TEXT;
ALTER TABLE article_processing ADD COLUMN batch_name TEXT;
ALTER TABLE article_processing ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE article_processing ADD COLUMN completed_at TEXT;

CREATE INDEX idx_article_processing_status
ON article_processing(status, updated_at DESC);

CREATE INDEX idx_article_processing_agent_status
ON article_processing(agent_id, status, updated_at DESC)
WHERE agent_id IS NOT NULL;

CREATE INDEX idx_article_processing_batch_name
ON article_processing(batch_name)
WHERE batch_name IS NOT NULL;
