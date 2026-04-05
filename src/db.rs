use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;
use tokio::task;
use uuid::Uuid;

use crate::domain::{
    Article, ArticleDetail, ArticleListItem, ArticleQuery, FeedKind, ProcessingStatus, Source,
};

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../migrations/0001_init.sql")),
    (
        "0002_scheduler_metadata",
        include_str!("../migrations/0002_scheduler_metadata.sql"),
    ),
    (
        "0003_llm_processing",
        include_str!("../migrations/0003_llm_processing.sql"),
    ),
    (
        "0004_processing_observability",
        include_str!("../migrations/0004_processing_observability.sql"),
    ),
    (
        "0005_article_favorites",
        include_str!("../migrations/0005_article_favorites.sql"),
    ),
    (
        "0006_article_read_state",
        include_str!("../migrations/0006_article_read_state.sql"),
    ),
    (
        "0007_llm_titles",
        include_str!("../migrations/0007_llm_titles.sql"),
    ),
    (
        "0008_article_visibility",
        include_str!("../migrations/0008_article_visibility.sql"),
    ),
    (
        "0009_article_content",
        include_str!("../migrations/0009_article_content.sql"),
    ),
];

#[derive(Clone)]
pub struct Database {
    path: Arc<PathBuf>,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database I/O failed: {0}")]
    Io(String),
    #[error("database operation failed: {0}")]
    Sql(String),
    #[error("database task failed: {0}")]
    Task(String),
}

#[derive(Debug)]
struct ArticleRow {
    article: Article,
    source_title: String,
    available_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct SourceFetchState {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SourceFetchUpdate {
    pub last_fetch_at: Option<DateTime<Utc>>,
    pub next_fetch_at: DateTime<Utc>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub validation_status: String,
}

#[derive(Debug, Clone)]
pub struct FetchedArticleInput {
    pub source_id: Uuid,
    pub dedupe_key: String,
    pub title: String,
    pub summary: String,
    pub content: String,
    pub url: String,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub ignored: bool,
}

#[derive(Debug, Clone)]
pub struct PendingProcessingJob {
    pub article_id: Uuid,
    pub agent_id: String,
    pub source_title: String,
    pub title: String,
    pub summary: String,
    pub content: String,
    pub url: String,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ActiveBatch {
    pub name: String,
    pub agent_id: String,
    pub article_count: usize,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BatchArticleOutput {
    pub article_id: Uuid,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FailedProcessingJob {
    pub article_id: Uuid,
    pub agent_id: Option<String>,
    pub source_title: String,
    pub title: String,
    pub attempts: u32,
    pub last_error: String,
    pub last_batch_name: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl Database {
    pub async fn new(path: PathBuf) -> Result<Self, DbError> {
        let database = Self {
            path: Arc::new(path),
        };
        database.initialize().await?;
        Ok(database)
    }

    pub async fn list_sources(&self) -> Result<Vec<Source>, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let mut statement = connection
                .prepare(
                    "SELECT id, title, feed_url, feed_kind, enabled, assigned_agent_id, validation_status, \
                     last_fetch_at, next_fetch_at, created_at, updated_at \
                     FROM sources ORDER BY title ASC",
                )
                .map_err(sql_error)?;

            let rows = statement
                .query_map([], read_source)
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;

            Ok(rows)
        })
        .await
    }

    pub async fn list_due_sources(
        &self,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<Source>, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let mut statement = connection
                .prepare(
                    "SELECT id, title, feed_url, feed_kind, enabled, assigned_agent_id, validation_status, \
                     last_fetch_at, next_fetch_at, created_at, updated_at \
                     FROM sources
                     WHERE enabled = 1 AND (next_fetch_at IS NULL OR next_fetch_at <= ?1)
                     ORDER BY COALESCE(next_fetch_at, created_at) ASC
                     LIMIT ?2",
                )
                .map_err(sql_error)?;

            let rows = statement
                .query_map(params![datetime_to_string(now), limit as i64], read_source)
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;

            Ok(rows)
        })
        .await
    }

    pub async fn get_source(&self, id: Uuid) -> Result<Option<Source>, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            connection
                .query_row(
                    "SELECT id, title, feed_url, feed_kind, enabled, assigned_agent_id, validation_status, \
                     last_fetch_at, next_fetch_at, created_at, updated_at \
                     FROM sources WHERE id = ?1",
                    [id.to_string()],
                    read_source,
                )
                .optional()
                .map_err(sql_error)
        })
        .await
    }

    pub async fn insert_source(&self, source: &Source) -> Result<(), DbError> {
        let path = self.path.clone();
        let source = source.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            connection
                .execute(
                    "INSERT INTO sources (
                        id, title, feed_url, feed_kind, enabled, assigned_agent_id, validation_status,
                        last_fetch_at, next_fetch_at, created_at, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        source.id.to_string(),
                        source.title,
                        source.feed_url,
                        feed_kind_to_str(source.feed_kind),
                        bool_to_int(source.enabled),
                        source.assigned_agent_id,
                        source.validation_status,
                        source.last_fetch_at.map(datetime_to_string),
                        source.next_fetch_at.map(datetime_to_string),
                        datetime_to_string(source.created_at),
                        datetime_to_string(source.updated_at),
                    ],
                )
                .map_err(sql_error)?;
            Ok(())
        })
        .await
    }

    pub async fn update_source(&self, source: &Source) -> Result<(), DbError> {
        let path = self.path.clone();
        let source = source.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            connection
                .execute(
                    "UPDATE sources
                     SET title = ?2, feed_url = ?3, feed_kind = ?4, enabled = ?5, assigned_agent_id = ?6,
                         validation_status = ?7, last_fetch_at = ?8, next_fetch_at = ?9, updated_at = ?10
                     WHERE id = ?1",
                    params![
                        source.id.to_string(),
                        source.title,
                        source.feed_url,
                        feed_kind_to_str(source.feed_kind),
                        bool_to_int(source.enabled),
                        source.assigned_agent_id,
                        source.validation_status,
                        source.last_fetch_at.map(datetime_to_string),
                        source.next_fetch_at.map(datetime_to_string),
                        datetime_to_string(source.updated_at),
                    ],
                )
                .map_err(sql_error)?;
            Ok(())
        })
        .await
    }

    pub async fn delete_source(&self, id: Uuid) -> Result<bool, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let affected = connection
                .execute("DELETE FROM sources WHERE id = ?1", [id.to_string()])
                .map_err(sql_error)?;
            Ok(affected > 0)
        })
        .await
    }

    #[cfg(test)]
    pub async fn insert_article(&self, article: &Article) -> Result<(), DbError> {
        let path = self.path.clone();
        let article = article.clone();
        self.run_blocking(move || {
            let mut connection = open_connection(path.as_ref())?;
            let transaction = connection.transaction().map_err(sql_error)?;
            let now = datetime_to_string(Utc::now());
            transaction
                .execute(
                    "INSERT OR REPLACE INTO articles (
                        id, source_id, title, summary, content, url, published_at, fetched_at, read_at, ignored, bookmarked
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        article.id.to_string(),
                        article.source_id.to_string(),
                        article.title,
                        article.summary,
                        article.content,
                        article.url,
                        article.published_at.map(datetime_to_string),
                        datetime_to_string(article.fetched_at),
                        article.read_at.map(datetime_to_string),
                        bool_to_int(article.ignored),
                        bool_to_int(article.bookmarked),
                    ],
                )
                .map_err(sql_error)?;
            transaction
                .execute(
                    "INSERT OR REPLACE INTO article_processing (
                        article_id, agent_id, status, llm_title, llm_summary, last_error, batch_name, last_batch_name, attempts, updated_at, completed_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        article.id.to_string(),
                        Option::<String>::None,
                        processing_status_to_str(article.llm_status),
                        article.llm_title,
                        article.llm_summary,
                        article.llm_error,
                        Option::<String>::None,
                        Option::<String>::None,
                        0i64,
                        now.clone(),
                        matches!(
                            article.llm_status,
                            ProcessingStatus::Done | ProcessingStatus::Failed
                        )
                        .then_some(now),
                    ],
                )
                .map_err(sql_error)?;
            transaction.commit().map_err(sql_error)?;
            Ok(())
        })
        .await
    }

    pub async fn get_source_fetch_state(
        &self,
        source_id: Uuid,
    ) -> Result<SourceFetchState, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let state = connection
                .query_row(
                    "SELECT etag, last_modified FROM source_fetch_state WHERE source_id = ?1",
                    [source_id.to_string()],
                    |row| {
                        Ok(SourceFetchState {
                            etag: row.get(0)?,
                            last_modified: row.get(1)?,
                        })
                    },
                )
                .optional()
                .map_err(sql_error)?
                .unwrap_or_default();
            Ok(state)
        })
        .await
    }

    pub async fn apply_source_fetch_update(
        &self,
        source_id: Uuid,
        update: SourceFetchUpdate,
    ) -> Result<(), DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let mut connection = open_connection(path.as_ref())?;
            let transaction = connection.transaction().map_err(sql_error)?;
            transaction
                .execute(
                    "UPDATE sources
                     SET last_fetch_at = ?2, next_fetch_at = ?3, validation_status = ?4, updated_at = ?5
                     WHERE id = ?1",
                    params![
                        source_id.to_string(),
                        update.last_fetch_at.map(datetime_to_string),
                        datetime_to_string(update.next_fetch_at),
                        update.validation_status,
                        datetime_to_string(Utc::now()),
                    ],
                )
                .map_err(sql_error)?;
            transaction
                .execute(
                    "INSERT INTO source_fetch_state (source_id, etag, last_modified)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(source_id) DO UPDATE SET
                        etag = excluded.etag,
                        last_modified = excluded.last_modified",
                    params![
                        source_id.to_string(),
                        update.etag,
                        update.last_modified,
                    ],
                )
                .map_err(sql_error)?;
            transaction.commit().map_err(sql_error)?;
            Ok(())
        })
        .await
    }

    pub async fn upsert_fetched_article(
        &self,
        input: FetchedArticleInput,
    ) -> Result<Article, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let mut connection = open_connection(path.as_ref())?;
            let transaction = connection.transaction().map_err(sql_error)?;

            let existing = transaction
                .query_row(
                    "SELECT id, bookmarked, read_at, ignored FROM articles WHERE source_id = ?1 AND dedupe_key = ?2",
                    params![input.source_id.to_string(), input.dedupe_key],
                    |row| Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)? != 0,
                        parse_datetime_opt(row.get::<_, Option<String>>(2)?)?,
                        row.get::<_, i64>(3)? != 0,
                    )),
                )
                .optional()
                .map_err(sql_error)?;

            let assigned_agent_id = transaction
                .query_row(
                    "SELECT assigned_agent_id FROM sources WHERE id = ?1",
                    [input.source_id.to_string()],
                    |row| row.get::<_, Option<String>>(0),
                )
                .map_err(sql_error)?;

            let (article_id, bookmarked, read_at, ignored, is_new) = match existing {
                Some((id, bookmarked, read_at, ignored)) => (
                    parse_uuid(id).map_err(sql_error)?,
                    bookmarked,
                    read_at,
                    ignored,
                    false,
                ),
                None => (Uuid::new_v4(), false, None, input.ignored, true),
            };

            transaction
                .execute(
                    "INSERT INTO articles (
                        id, source_id, title, summary, content, url, published_at, fetched_at, read_at, ignored, bookmarked, dedupe_key
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                    ON CONFLICT(id) DO UPDATE SET
                        title = excluded.title,
                        summary = excluded.summary,
                        content = excluded.content,
                        url = excluded.url,
                        published_at = excluded.published_at,
                        fetched_at = excluded.fetched_at,
                        dedupe_key = excluded.dedupe_key",
                    params![
                        article_id.to_string(),
                        input.source_id.to_string(),
                        input.title,
                        input.summary,
                        input.content,
                        input.url,
                        input.published_at.map(datetime_to_string),
                        datetime_to_string(input.fetched_at),
                        read_at.map(datetime_to_string),
                        bool_to_int(ignored),
                        bool_to_int(bookmarked),
                        input.dedupe_key,
                    ],
                )
                .map_err(sql_error)?;

            if is_new {
                let initial_status = if assigned_agent_id.is_some() && !input.ignored {
                    ProcessingStatus::Pending
                } else {
                    ProcessingStatus::Done
                };
                let processing_agent_id = if input.ignored {
                    None
                } else {
                    assigned_agent_id.clone()
                };
                transaction
                    .execute(
                        "INSERT OR REPLACE INTO article_processing (
                            article_id, agent_id, status, llm_title, llm_summary, last_error, batch_name, last_batch_name, attempts, updated_at, completed_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                        params![
                            article_id.to_string(),
                            processing_agent_id,
                            processing_status_to_str(initial_status),
                            Option::<String>::None,
                            Option::<String>::None,
                            Option::<String>::None,
                            Option::<String>::None,
                            Option::<String>::None,
                            0i64,
                            datetime_to_string(Utc::now()),
                            if initial_status == ProcessingStatus::Done {
                                Some(datetime_to_string(Utc::now()))
                            } else {
                                None
                            },
                        ],
                    )
                    .map_err(sql_error)?;
            }

            transaction.commit().map_err(sql_error)?;

            let processing = if is_new {
                (
                    if assigned_agent_id.is_some() && !input.ignored {
                        ProcessingStatus::Pending
                    } else {
                        ProcessingStatus::Done
                    },
                    None,
                    None,
                    None,
                )
            } else {
                transactionless_processing_snapshot(path.as_ref(), article_id)?
            };

            Ok(Article {
                id: article_id,
                source_id: input.source_id,
                title: input.title,
                summary: input.summary,
                content: input.content,
                url: input.url,
                published_at: input.published_at,
                fetched_at: input.fetched_at,
                read_at,
                ignored,
                bookmarked,
                llm_status: processing.0,
                llm_title: processing.1,
                llm_summary: processing.2,
                llm_error: processing.3,
            })
        })
        .await
    }

    pub async fn list_articles(
        &self,
        query: ArticleQuery,
    ) -> Result<Vec<ArticleListItem>, DbError> {
        let rows = self.fetch_article_rows().await?;
        let mut items = rows
            .into_iter()
            .filter(|row| !row.article.ignored)
            .filter(|row| {
                query
                    .source_id
                    .is_none_or(|source_id| row.article.source_id == source_id)
            })
            .filter(|row| {
                query
                    .favorite_filter()
                    .is_none_or(|favorited| row.article.bookmarked == favorited)
            })
            .map(|row| {
                ArticleListItem::from_article(row.article, row.source_title, row.available_at)
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.available_at.cmp(&left.available_at));
        Ok(items)
    }

    pub async fn get_article(&self, id: Uuid) -> Result<Option<ArticleDetail>, DbError> {
        let rows = self.fetch_article_rows().await?;
        Ok(rows
            .into_iter()
            .filter(|row| !row.article.ignored)
            .find(|row| row.article.id == id)
            .map(|row| {
                ArticleDetail::from_article(row.article, row.source_title, row.available_at)
            }))
    }

    pub async fn set_favorite(&self, id: Uuid, favorited: bool) -> Result<bool, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let affected = connection
                .execute(
                    "UPDATE articles SET bookmarked = ?2 WHERE id = ?1",
                    params![id.to_string(), bool_to_int(favorited)],
                )
                .map_err(sql_error)?;
            Ok(affected > 0)
        })
        .await
    }

    pub async fn set_read_state(&self, id: Uuid, read: bool) -> Result<bool, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let affected = connection
                .execute(
                    "UPDATE articles SET read_at = ?2 WHERE id = ?1",
                    params![
                        id.to_string(),
                        if read {
                            Some(datetime_to_string(Utc::now()))
                        } else {
                            Option::<String>::None
                        }
                    ],
                )
                .map_err(sql_error)?;
            Ok(affected > 0)
        })
        .await
    }

    pub async fn delete_expired_non_favorited_articles(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<usize, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let deleted = connection
                .execute(
                    "DELETE FROM articles
                     WHERE bookmarked = 0
                       AND fetched_at < ?1",
                    params![datetime_to_string(cutoff)],
                )
                .map_err(sql_error)?;
            Ok(deleted)
        })
        .await
    }

    pub async fn list_pending_processing_jobs(
        &self,
        limit: usize,
    ) -> Result<Vec<PendingProcessingJob>, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let mut statement = connection
                .prepare(
                    "SELECT
                        a.id,
                        p.agent_id,
                        s.title,
                        a.title,
                        a.summary,
                        a.content,
                        a.url,
                        a.published_at
                     FROM article_processing p
                     INNER JOIN articles a ON a.id = p.article_id
                     INNER JOIN sources s ON s.id = a.source_id
                     WHERE p.status = 'pending'
                       AND p.agent_id IS NOT NULL
                     ORDER BY a.fetched_at ASC
                     LIMIT ?1",
                )
                .map_err(sql_error)?;

            let rows = statement
                .query_map([limit as i64], |row| {
                    Ok(PendingProcessingJob {
                        article_id: parse_uuid(row.get::<_, String>(0)?)?,
                        agent_id: row.get(1)?,
                        source_title: row.get(2)?,
                        title: row.get(3)?,
                        summary: row.get(4)?,
                        content: row.get(5)?,
                        url: row.get(6)?,
                        published_at: parse_datetime_opt(row.get::<_, Option<String>>(7)?)?,
                    })
                })
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;

            Ok(rows)
        })
        .await
    }

    pub async fn mark_batch_started(
        &self,
        article_ids: &[Uuid],
        batch_name: &str,
    ) -> Result<(), DbError> {
        let path = self.path.clone();
        let article_ids = article_ids.to_vec();
        let batch_name = batch_name.to_string();
        self.run_blocking(move || {
            let mut connection = open_connection(path.as_ref())?;
            let transaction = connection.transaction().map_err(sql_error)?;
            let now = datetime_to_string(Utc::now());

            for article_id in article_ids {
                transaction
                    .execute(
                        "UPDATE article_processing
                         SET status = 'processing',
                             batch_name = ?2,
                             last_batch_name = ?2,
                             last_error = NULL,
                             attempts = attempts + 1,
                             updated_at = ?3,
                             completed_at = NULL
                         WHERE article_id = ?1",
                        params![article_id.to_string(), batch_name.clone(), now.clone()],
                    )
                    .map_err(sql_error)?;
            }

            transaction.commit().map_err(sql_error)?;
            Ok(())
        })
        .await
    }

    pub async fn record_processing_failure(
        &self,
        article_ids: &[Uuid],
        error_message: &str,
        retry_limit: u32,
    ) -> Result<(), DbError> {
        let path = self.path.clone();
        let article_ids = article_ids.to_vec();
        let error_message = error_message.to_string();
        self.run_blocking(move || {
            let mut connection = open_connection(path.as_ref())?;
            let transaction = connection.transaction().map_err(sql_error)?;
            let now = datetime_to_string(Utc::now());

            for article_id in article_ids {
                let attempts = transaction
                    .query_row(
                        "SELECT attempts FROM article_processing WHERE article_id = ?1",
                        [article_id.to_string()],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .unwrap_or(0);

                let next_attempts = attempts + 1;
                let exhausted = next_attempts >= retry_limit as i64;
                transaction
                    .execute(
                        "UPDATE article_processing
                         SET status = ?2,
                             last_error = ?3,
                             batch_name = NULL,
                             attempts = ?4,
                             updated_at = ?5,
                             completed_at = ?6,
                             llm_title = NULL,
                             llm_summary = NULL
                         WHERE article_id = ?1",
                        params![
                            article_id.to_string(),
                            if exhausted { "failed" } else { "pending" },
                            error_message.clone(),
                            next_attempts,
                            now.clone(),
                            exhausted.then_some(now.clone()),
                        ],
                    )
                    .map_err(sql_error)?;
            }

            transaction.commit().map_err(sql_error)?;
            Ok(())
        })
        .await
    }

    pub async fn list_active_batches(&self) -> Result<Vec<ActiveBatch>, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let mut statement = connection
                .prepare(
                    "SELECT batch_name, agent_id, article_id, updated_at
                     FROM article_processing
                     WHERE status = 'processing'
                       AND batch_name IS NOT NULL
                       AND agent_id IS NOT NULL
                     ORDER BY updated_at ASC",
                )
                .map_err(sql_error)?;

            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        parse_uuid(row.get::<_, String>(2)?)?,
                        parse_datetime(row.get::<_, String>(3)?)?,
                    ))
                })
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;

            let mut grouped: BTreeMap<(String, String), (Vec<Uuid>, DateTime<Utc>)> =
                BTreeMap::new();
            for (batch_name, agent_id, article_id, updated_at) in rows {
                grouped
                    .entry((batch_name, agent_id))
                    .and_modify(|(article_ids, newest_updated_at)| {
                        article_ids.push(article_id);
                        if updated_at > *newest_updated_at {
                            *newest_updated_at = updated_at;
                        }
                    })
                    .or_insert_with(|| (vec![article_id], updated_at));
            }

            Ok(grouped
                .into_iter()
                .map(
                    |((name, agent_id), (article_ids, updated_at))| ActiveBatch {
                        name,
                        agent_id,
                        article_count: article_ids.len(),
                        updated_at,
                    },
                )
                .collect())
        })
        .await
    }

    pub async fn apply_batch_outputs(
        &self,
        batch_name: &str,
        outputs: &[BatchArticleOutput],
        retry_limit: u32,
    ) -> Result<(), DbError> {
        let path = self.path.clone();
        let batch_name = batch_name.to_string();
        let outputs = outputs.to_vec();
        self.run_blocking(move || {
            let mut connection = open_connection(path.as_ref())?;
            let transaction = connection.transaction().map_err(sql_error)?;
            let now = datetime_to_string(Utc::now());

            let rows = transaction
                .prepare(
                    "SELECT article_id, attempts
                     FROM article_processing
                     WHERE batch_name = ?1 AND status = 'processing'",
                )
                .map_err(sql_error)?
                .query_map([batch_name.clone()], |row| {
                    Ok((parse_uuid(row.get::<_, String>(0)?)?, row.get::<_, i64>(1)?))
                })
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;

            let output_map = outputs
                .into_iter()
                .map(|output| (output.article_id, output))
                .collect::<BTreeMap<_, _>>();

            for (article_id, attempts) in rows {
                let Some(output) = output_map.get(&article_id) else {
                    let exhausted = attempts >= retry_limit as i64;
                    transaction
                        .execute(
                            "UPDATE article_processing
                             SET status = ?2,
                                 last_error = ?3,
                                 batch_name = NULL,
                                 last_batch_name = ?6,
                                 updated_at = ?4,
                                 completed_at = ?5,
                                 llm_title = NULL,
                                 llm_summary = NULL
                             WHERE article_id = ?1",
                            params![
                                article_id.to_string(),
                                if exhausted { "failed" } else { "pending" },
                                "batch completed without a response for this article",
                                now.clone(),
                                exhausted.then_some(now.clone()),
                                batch_name.clone(),
                            ],
                        )
                        .map_err(sql_error)?;
                    continue;
                };

                match (&output.summary, &output.error) {
                    (Some(summary), None) => {
                        transaction
                            .execute(
                                "UPDATE article_processing
                                 SET status = 'done',
                                     llm_title = ?2,
                                     llm_summary = ?3,
                                     last_error = NULL,
                                     batch_name = NULL,
                                     last_batch_name = ?6,
                                     updated_at = ?4,
                                     completed_at = ?5
                                 WHERE article_id = ?1",
                                params![
                                    article_id.to_string(),
                                    output.title,
                                    summary,
                                    now.clone(),
                                    now.clone(),
                                    batch_name.clone(),
                                ],
                            )
                            .map_err(sql_error)?;
                    }
                    (_, Some(error_message)) => {
                        let exhausted = attempts >= retry_limit as i64;
                        transaction
                            .execute(
                                "UPDATE article_processing
                                 SET status = ?2,
                                     llm_title = NULL,
                                     llm_summary = NULL,
                                     last_error = ?3,
                                     batch_name = NULL,
                                     last_batch_name = ?6,
                                     updated_at = ?4,
                                     completed_at = ?5
                                 WHERE article_id = ?1",
                                params![
                                    article_id.to_string(),
                                    if exhausted { "failed" } else { "pending" },
                                    error_message,
                                    now.clone(),
                                    exhausted.then_some(now.clone()),
                                    batch_name.clone(),
                                ],
                            )
                            .map_err(sql_error)?;
                    }
                    _ => {
                        let exhausted = attempts >= retry_limit as i64;
                        transaction
                            .execute(
                                "UPDATE article_processing
                                 SET status = ?2,
                                     llm_title = NULL,
                                     llm_summary = NULL,
                                     last_error = ?3,
                                     batch_name = NULL,
                                     last_batch_name = ?6,
                                     updated_at = ?4,
                                     completed_at = ?5
                                 WHERE article_id = ?1",
                                params![
                                    article_id.to_string(),
                                    if exhausted { "failed" } else { "pending" },
                                    "batch response did not include output text",
                                    now.clone(),
                                    exhausted.then_some(now.clone()),
                                    batch_name.clone(),
                                ],
                            )
                            .map_err(sql_error)?;
                    }
                }
            }

            transaction.commit().map_err(sql_error)?;
            Ok(())
        })
        .await
    }

    pub async fn fail_batch(
        &self,
        batch_name: &str,
        error_message: &str,
        retry_limit: u32,
    ) -> Result<(), DbError> {
        let path = self.path.clone();
        let batch_name = batch_name.to_string();
        let error_message = error_message.to_string();
        self.run_blocking(move || {
            let mut connection = open_connection(path.as_ref())?;
            let transaction = connection.transaction().map_err(sql_error)?;
            let now = datetime_to_string(Utc::now());

            let rows = transaction
                .prepare(
                    "SELECT article_id, attempts
                     FROM article_processing
                     WHERE batch_name = ?1 AND status = 'processing'",
                )
                .map_err(sql_error)?
                .query_map([batch_name.clone()], |row| {
                    Ok((parse_uuid(row.get::<_, String>(0)?)?, row.get::<_, i64>(1)?))
                })
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;

            for (article_id, attempts) in rows {
                let exhausted = attempts >= retry_limit as i64;
                transaction
                    .execute(
                        "UPDATE article_processing
                         SET status = ?2,
                             llm_title = NULL,
                             llm_summary = NULL,
                             last_error = ?3,
                             batch_name = NULL,
                             last_batch_name = ?6,
                             updated_at = ?4,
                             completed_at = ?5
                         WHERE article_id = ?1",
                        params![
                            article_id.to_string(),
                            if exhausted { "failed" } else { "pending" },
                            error_message.clone(),
                            now.clone(),
                            exhausted.then_some(now.clone()),
                            batch_name.clone(),
                        ],
                    )
                    .map_err(sql_error)?;
            }

            transaction.commit().map_err(sql_error)?;
            Ok(())
        })
        .await
    }

    pub async fn list_failed_processing_jobs(
        &self,
        limit: usize,
    ) -> Result<Vec<FailedProcessingJob>, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let mut statement = connection
                .prepare(
                    "SELECT
                        a.id,
                        p.agent_id,
                        s.title,
                        a.title,
                        p.attempts,
                        p.last_error,
                        p.last_batch_name,
                        p.updated_at
                     FROM article_processing p
                     INNER JOIN articles a ON a.id = p.article_id
                     INNER JOIN sources s ON s.id = a.source_id
                     WHERE p.status = 'failed'
                     ORDER BY p.updated_at DESC
                     LIMIT ?1",
                )
                .map_err(sql_error)?;

            let rows = statement
                .query_map([limit as i64], |row| {
                    Ok(FailedProcessingJob {
                        article_id: parse_uuid(row.get::<_, String>(0)?)?,
                        agent_id: row.get(1)?,
                        source_title: row.get(2)?,
                        title: row.get(3)?,
                        attempts: row.get::<_, i64>(4)? as u32,
                        last_error: row.get(5)?,
                        last_batch_name: row.get(6)?,
                        updated_at: parse_datetime(row.get::<_, String>(7)?)?,
                    })
                })
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;

            Ok(rows)
        })
        .await
    }

    pub async fn retry_article_processing(&self, article_id: Uuid) -> Result<bool, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;

            let exists = connection
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1
                        FROM article_processing
                        WHERE article_id = ?1
                          AND agent_id IS NOT NULL
                          AND status IN ('failed', 'done')
                    )",
                    [article_id.to_string()],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(sql_error)?
                != 0;
            if !exists {
                return Ok(false);
            }

            let updated = connection
                .execute(
                    "UPDATE article_processing
                     SET status = 'pending',
                         llm_title = NULL,
                         llm_summary = NULL,
                         last_error = NULL,
                         batch_name = NULL,
                         updated_at = ?2,
                         completed_at = NULL
                     WHERE article_id = ?1
                       AND agent_id IS NOT NULL
                       AND status IN ('failed', 'done')",
                    params![article_id.to_string(), datetime_to_string(Utc::now())],
                )
                .map_err(sql_error)?;
            Ok(updated > 0)
        })
        .await
    }

    pub async fn retry_batch_processing(&self, batch_name: &str) -> Result<usize, DbError> {
        let path = self.path.clone();
        let batch_name = batch_name.to_string();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let updated = connection
                .execute(
                    "UPDATE article_processing
                     SET status = 'pending',
                         llm_title = NULL,
                         llm_summary = NULL,
                         last_error = NULL,
                         batch_name = NULL,
                         updated_at = ?2,
                         completed_at = NULL
                     WHERE last_batch_name = ?1
                       AND agent_id IS NOT NULL
                       AND status IN ('failed', 'processing', 'done')",
                    params![batch_name, datetime_to_string(Utc::now())],
                )
                .map_err(sql_error)?;
            Ok(updated)
        })
        .await
    }

    async fn initialize(&self) -> Result<(), DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| DbError::Io(error.to_string()))?;
            }

            let mut connection = open_connection(path.as_ref())?;
            connection
                .execute(
                    "CREATE TABLE IF NOT EXISTS schema_migrations (
                        name TEXT PRIMARY KEY,
                        applied_at TEXT NOT NULL
                    )",
                    [],
                )
                .map_err(sql_error)?;

            for (name, sql) in MIGRATIONS {
                let already_applied = connection
                    .query_row(
                        "SELECT 1 FROM schema_migrations WHERE name = ?1",
                        [*name],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .is_some();

                if already_applied {
                    continue;
                }

                let transaction = connection.transaction().map_err(sql_error)?;
                transaction.execute_batch(sql).map_err(sql_error)?;
                transaction
                    .execute(
                        "INSERT INTO schema_migrations (name, applied_at) VALUES (?1, ?2)",
                        params![name, datetime_to_string(Utc::now())],
                    )
                    .map_err(sql_error)?;
                transaction.commit().map_err(sql_error)?;
            }

            Ok(())
        })
        .await
    }

    async fn fetch_article_rows(&self) -> Result<Vec<ArticleRow>, DbError> {
        let path = self.path.clone();
        self.run_blocking(move || {
            let connection = open_connection(path.as_ref())?;
            let mut statement = connection
                .prepare(
                    "SELECT
                        a.id,
                        a.source_id,
                        s.title,
                        a.title,
                        a.summary,
                        a.content,
                        a.url,
                        a.published_at,
                        a.fetched_at,
                        a.read_at,
                        a.ignored,
                        a.bookmarked,
                        COALESCE(p.status, 'pending'),
                        p.llm_title,
                        p.llm_summary,
                        p.last_error,
                        p.completed_at
                     FROM articles a
                     INNER JOIN sources s ON s.id = a.source_id
                     LEFT JOIN article_processing p ON p.article_id = a.id",
                )
                .map_err(sql_error)?;

            let rows = statement
                .query_map([], |row| {
                    let article = Article {
                        id: parse_uuid(row.get::<_, String>(0)?)?,
                        source_id: parse_uuid(row.get::<_, String>(1)?)?,
                        title: row.get(3)?,
                        summary: row.get(4)?,
                        content: row.get(5)?,
                        url: row.get(6)?,
                        published_at: parse_datetime_opt(row.get::<_, Option<String>>(7)?)?,
                        fetched_at: parse_datetime(row.get::<_, String>(8)?)?,
                        read_at: parse_datetime_opt(row.get::<_, Option<String>>(9)?)?,
                        ignored: row.get::<_, i64>(10)? != 0,
                        bookmarked: row.get::<_, i64>(11)? != 0,
                        llm_status: parse_processing_status(row.get::<_, String>(12)?)?,
                        llm_title: row.get(13)?,
                        llm_summary: row.get(14)?,
                        llm_error: row.get(15)?,
                    };
                    let completed_at = parse_datetime_opt(row.get::<_, Option<String>>(16)?)?;
                    let available_at = completed_at.unwrap_or(article.fetched_at);
                    Ok(ArticleRow {
                        article,
                        source_title: row.get(2)?,
                        available_at,
                    })
                })
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;

            Ok(rows)
        })
        .await
    }

    async fn run_blocking<T, F>(&self, job: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, DbError> + Send + 'static,
    {
        task::spawn_blocking(job)
            .await
            .map_err(|error| DbError::Task(error.to_string()))?
    }
}

fn open_connection(path: &Path) -> Result<Connection, DbError> {
    let connection = Connection::open(path).map_err(sql_error)?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(sql_error)?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(sql_error)?;
    Ok(connection)
}

fn transactionless_processing_snapshot(
    path: &Path,
    article_id: Uuid,
) -> Result<
    (
        ProcessingStatus,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
    DbError,
> {
    let connection = open_connection(path)?;
    let snapshot = connection
        .query_row(
            "SELECT status, llm_title, llm_summary, last_error
             FROM article_processing
             WHERE article_id = ?1",
            [article_id.to_string()],
            |row| {
                Ok((
                    parse_processing_status(row.get::<_, String>(0)?)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?
        .unwrap_or((ProcessingStatus::Done, None, None, None));
    Ok(snapshot)
}

fn read_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<Source> {
    Ok(Source {
        id: parse_uuid(row.get::<_, String>(0)?)?,
        title: row.get(1)?,
        feed_url: row.get(2)?,
        feed_kind: parse_feed_kind(row.get::<_, String>(3)?)?,
        enabled: row.get::<_, i64>(4)? != 0,
        assigned_agent_id: row.get(5)?,
        validation_status: row.get(6)?,
        last_fetch_at: parse_datetime_opt(row.get::<_, Option<String>>(7)?)?,
        next_fetch_at: parse_datetime_opt(row.get::<_, Option<String>>(8)?)?,
        created_at: parse_datetime(row.get::<_, String>(9)?)?,
        updated_at: parse_datetime(row.get::<_, String>(10)?)?,
    })
}

fn sql_error(error: impl std::fmt::Display) -> DbError {
    DbError::Sql(error.to_string())
}

fn parse_uuid(value: String) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            value.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn parse_datetime(value: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                value.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
}

fn parse_datetime_opt(value: Option<String>) -> rusqlite::Result<Option<DateTime<Utc>>> {
    value.map(parse_datetime).transpose()
}

fn parse_feed_kind(value: String) -> rusqlite::Result<FeedKind> {
    match value.as_str() {
        "rss" => Ok(FeedKind::Rss),
        "atom" => Ok(FeedKind::Atom),
        _ => Err(rusqlite::Error::InvalidColumnType(
            3,
            "feed_kind".to_string(),
            rusqlite::types::Type::Text,
        )),
    }
}

fn parse_processing_status(value: String) -> rusqlite::Result<ProcessingStatus> {
    match value.as_str() {
        "pending" => Ok(ProcessingStatus::Pending),
        "processing" => Ok(ProcessingStatus::Processing),
        "done" => Ok(ProcessingStatus::Done),
        "failed" => Ok(ProcessingStatus::Failed),
        _ => Err(rusqlite::Error::InvalidColumnType(
            9,
            "status".to_string(),
            rusqlite::types::Type::Text,
        )),
    }
}

fn feed_kind_to_str(feed_kind: FeedKind) -> &'static str {
    match feed_kind {
        FeedKind::Rss => "rss",
        FeedKind::Atom => "atom",
    }
}

fn processing_status_to_str(status: ProcessingStatus) -> &'static str {
    match status {
        ProcessingStatus::Pending => "pending",
        ProcessingStatus::Processing => "processing",
        ProcessingStatus::Done => "done",
        ProcessingStatus::Failed => "failed",
    }
}

fn datetime_to_string(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn bool_to_int(value: bool) -> i64 {
    if value { 1 } else { 0 }
}
