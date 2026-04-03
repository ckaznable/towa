CREATE VIEW IF NOT EXISTS favorite_articles AS
SELECT
    id,
    source_id,
    title,
    summary,
    url,
    published_at,
    fetched_at,
    bookmarked,
    dedupe_key
FROM articles
WHERE bookmarked = 1;
