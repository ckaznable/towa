ALTER TABLE articles ADD COLUMN ignored INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_articles_ignored ON articles(ignored);
