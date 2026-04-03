ALTER TABLE article_processing ADD COLUMN last_batch_name TEXT;

CREATE INDEX idx_article_processing_last_batch_name
ON article_processing(last_batch_name)
WHERE last_batch_name IS NOT NULL;
