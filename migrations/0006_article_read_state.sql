ALTER TABLE articles ADD COLUMN read_at TEXT;

CREATE INDEX idx_articles_read_at
ON articles(read_at DESC);
