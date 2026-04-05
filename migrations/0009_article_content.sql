ALTER TABLE articles ADD COLUMN content TEXT NOT NULL DEFAULT '';

UPDATE articles
SET content = summary
WHERE content = '';
