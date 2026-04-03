CREATE TABLE source_fetch_state (
    source_id TEXT PRIMARY KEY REFERENCES sources(id) ON DELETE CASCADE,
    etag TEXT,
    last_modified TEXT
);

ALTER TABLE articles ADD COLUMN dedupe_key TEXT;

CREATE UNIQUE INDEX idx_articles_source_dedupe_key
ON articles(source_id, dedupe_key)
WHERE dedupe_key IS NOT NULL;
