CREATE TABLE sources (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    feed_url TEXT NOT NULL,
    feed_kind TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    assigned_agent_id TEXT,
    validation_status TEXT NOT NULL,
    last_fetch_at TEXT,
    next_fetch_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE articles (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    url TEXT NOT NULL,
    published_at TEXT,
    fetched_at TEXT NOT NULL,
    bookmarked INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE article_processing (
    article_id TEXT PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    llm_summary TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_sources_feed_url ON sources(feed_url);
CREATE INDEX idx_articles_source_id ON articles(source_id);
CREATE INDEX idx_articles_fetched_at ON articles(fetched_at DESC);
CREATE INDEX idx_articles_bookmarked ON articles(bookmarked);
