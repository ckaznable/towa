use std::{collections::VecDeque, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use feed_rs::{model::Entry, parser};
use reqwest::header::{
    CACHE_CONTROL, ETAG, EXPIRES, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
};
use thiserror::Error;
use tokio::{sync::Mutex, time};
use uuid::Uuid;

use crate::{
    db::{FetchedArticleInput, SourceFetchUpdate},
    domain::{FeedKind, Source},
    state::{ApiError, AppState, FeedValidator, ValidatedFeed, ValidationError},
};

const MIN_FETCH_INTERVAL_MINUTES: i64 = 15;
const MAX_FETCH_INTERVAL_HOURS: i64 = 6;
const SCHEDULER_TICK_SECONDS: u64 = 30;
const ARTICLE_RETENTION_DAYS: i64 = 30;

#[derive(Clone)]
pub struct Scheduler {
    state: AppState,
    fetcher: Arc<dyn FeedFetcher>,
}

#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchStatus {
    Modified,
    NotModified,
}

#[derive(Debug, Clone)]
pub struct FetchResponse {
    pub status: FetchStatus,
    pub body: Option<Vec<u8>>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub cache_control: Option<String>,
    pub expires: Option<String>,
}

#[derive(Debug, Error, Clone)]
pub enum SchedulerError {
    #[error("{0}")]
    Api(String),
    #[error("{0}")]
    Fetch(String),
    #[error("{0}")]
    Parse(String),
}

#[async_trait]
pub trait FeedFetcher: Send + Sync {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, SchedulerError>;
}

pub struct HttpFeedFetcher {
    client: reqwest::Client,
}

impl Scheduler {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            fetcher: Arc::new(HttpFeedFetcher::new()),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_fetcher(state: AppState, fetcher: Arc<dyn FeedFetcher>) -> Self {
        Self { state, fetcher }
    }

    pub async fn run_loop(self) -> Result<(), SchedulerError> {
        let mut interval = time::interval(Duration::from_secs(SCHEDULER_TICK_SECONDS));
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            let processed = self.run_once().await?;
            if processed > 0 {
                tracing::info!("scheduler processed {processed} source(s)");
            }
        }
    }

    pub async fn run_once(&self) -> Result<usize, SchedulerError> {
        let now = Utc::now();
        let deleted = self
            .state
            .cleanup_expired_articles(now - article_retention_duration())
            .await
            .map_err(api_error)?;
        let due_sources = self.state.list_due_sources(now).await.map_err(api_error)?;
        let due_source_count = due_sources.len();

        let mut processed = 0usize;
        for source in due_sources {
            self.process_source(source, now).await?;
            processed += 1;
        }

        if deleted > 0 {
            tracing::info!("retention cleanup removed {deleted} expired article(s)");
        }
        if due_source_count > 0 {
            tracing::info!(due_source_count, "scheduler found source(s) ready to fetch");
        }

        Ok(processed)
    }

    async fn process_source(
        &self,
        source: Source,
        now: DateTime<Utc>,
    ) -> Result<(), SchedulerError> {
        let fetch_state = self
            .state
            .get_source_fetch_state(source.id)
            .await
            .map_err(api_error)?;
        tracing::info!(
            source_id = %source.id,
            source_title = %source.title,
            feed_url = %source.feed_url,
            feed_kind = ?source.feed_kind,
            has_etag = fetch_state.etag.is_some(),
            has_last_modified = fetch_state.last_modified.is_some(),
            "scheduler fetching source"
        );
        let response = self
            .fetcher
            .fetch(FetchRequest {
                url: source.feed_url.clone(),
                etag: fetch_state.etag.clone(),
                last_modified: fetch_state.last_modified.clone(),
            })
            .await;

        match response {
            Ok(response) => {
                self.handle_fetch_response(source, fetch_state, response, now)
                    .await
            }
            Err(error) => {
                let retry_at = now + min_fetch_interval();
                self.state
                    .apply_source_fetch_update(
                        source.id,
                        SourceFetchUpdate {
                            last_fetch_at: source.last_fetch_at,
                            next_fetch_at: retry_at,
                            etag: fetch_state.etag,
                            last_modified: fetch_state.last_modified,
                            validation_status: "fetch_error".to_string(),
                        },
                    )
                    .await
                    .map_err(api_error)?;
                tracing::warn!(
                    source_id = %source.id,
                    source_title = %source.title,
                    retry_at = %retry_at,
                    error = %error,
                    "scheduler fetch failed; source rescheduled"
                );
                Err(error)
            }
        }
    }

    async fn handle_fetch_response(
        &self,
        source: Source,
        fetch_state: crate::db::SourceFetchState,
        response: FetchResponse,
        now: DateTime<Utc>,
    ) -> Result<(), SchedulerError> {
        let next_fetch_at = compute_next_fetch_at(&source, &response, now);
        let etag = response.etag.clone().or(fetch_state.etag);
        let last_modified = response.last_modified.clone().or(fetch_state.last_modified);

        if response.status == FetchStatus::Modified {
            let body = response
                .body
                .as_ref()
                .ok_or_else(|| SchedulerError::Parse("missing feed body".to_string()))?;
            let feed = parser::parse(body.as_slice())
                .map_err(|error| SchedulerError::Parse(error.to_string()))?;
            let entry_count = feed.entries.len();
            let mut upserted_articles = 0usize;

            for entry in feed.entries {
                let article = FetchedArticleInput {
                    source_id: source.id,
                    dedupe_key: dedupe_key(&source.feed_kind, &entry),
                    title: entry_title(&entry),
                    summary: entry_summary(&entry),
                    url: entry_url(&entry),
                    published_at: entry.published.or(entry.updated),
                    fetched_at: now,
                };
                self.state
                    .upsert_fetched_article(article)
                    .await
                    .map_err(api_error)?;
                upserted_articles += 1;
            }

            tracing::info!(
                source_id = %source.id,
                source_title = %source.title,
                entry_count,
                upserted_articles,
                next_fetch_at = %next_fetch_at,
                etag = etag.as_deref().unwrap_or("-"),
                last_modified = last_modified.as_deref().unwrap_or("-"),
                "scheduler fetched and stored feed entries"
            );
        } else {
            tracing::info!(
                source_id = %source.id,
                source_title = %source.title,
                next_fetch_at = %next_fetch_at,
                etag = etag.as_deref().unwrap_or("-"),
                last_modified = last_modified.as_deref().unwrap_or("-"),
                "scheduler source not modified"
            );
        }

        self.state
            .apply_source_fetch_update(
                source.id,
                SourceFetchUpdate {
                    last_fetch_at: Some(now),
                    next_fetch_at,
                    etag,
                    last_modified,
                    validation_status: "validated".to_string(),
                },
            )
            .await
            .map_err(api_error)?;

        Ok(())
    }
}

impl HttpFeedFetcher {
    fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("towa-scheduler/0.1.0")
            .build()
            .expect("scheduler http client should build");
        Self { client }
    }
}

#[async_trait]
impl FeedFetcher for HttpFeedFetcher {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, SchedulerError> {
        let mut builder = self.client.get(&request.url);

        if let Some(etag) = request.etag {
            builder = builder.header(IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = request.last_modified {
            builder = builder.header(IF_MODIFIED_SINCE, last_modified);
        }

        let response = builder
            .send()
            .await
            .map_err(|error| SchedulerError::Fetch(error.to_string()))?;

        let status = if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            FetchStatus::NotModified
        } else if response.status().is_success() {
            FetchStatus::Modified
        } else {
            return Err(SchedulerError::Fetch(format!(
                "feed fetch failed with status {}",
                response.status()
            )));
        };

        let headers = response.headers().clone();
        let body = if status == FetchStatus::Modified {
            Some(
                response
                    .bytes()
                    .await
                    .map_err(|error| SchedulerError::Fetch(error.to_string()))?
                    .to_vec(),
            )
        } else {
            None
        };

        Ok(FetchResponse {
            status,
            body,
            etag: header_value(&headers, &ETAG),
            last_modified: header_value(&headers, &LAST_MODIFIED),
            cache_control: header_value(&headers, &CACHE_CONTROL),
            expires: header_value(&headers, &EXPIRES),
        })
    }
}

fn header_value(
    headers: &reqwest::header::HeaderMap,
    name: &reqwest::header::HeaderName,
) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn compute_next_fetch_at(
    source: &Source,
    response: &FetchResponse,
    now: DateTime<Utc>,
) -> DateTime<Utc> {
    header_next_fetch_at(response, now).unwrap_or_else(|| {
        fallback_next_fetch_at(source, response.status == FetchStatus::NotModified, now)
    })
}

fn header_next_fetch_at(response: &FetchResponse, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if let Some(cache_control) = response.cache_control.as_deref()
        && let Some(seconds) = parse_max_age(cache_control)
    {
        return Some(now + ChronoDuration::seconds(seconds as i64));
    }

    if let Some(expires) = response.expires.as_deref()
        && let Some(expires_at) = parse_http_datetime(expires)
        && expires_at > now
    {
        return Some(expires_at);
    }

    None
}

fn fallback_next_fetch_at(source: &Source, unchanged: bool, now: DateTime<Utc>) -> DateTime<Utc> {
    let min_interval = min_fetch_interval();
    let max_interval = max_fetch_interval();

    if unchanged {
        if let (Some(last_fetch_at), Some(next_fetch_at)) =
            (source.last_fetch_at, source.next_fetch_at)
        {
            let previous = next_fetch_at - last_fetch_at;
            let doubled = previous + previous;
            return now + clamp_duration(doubled, min_interval, max_interval);
        }
    }

    now + min_interval
}

fn clamp_duration(
    value: ChronoDuration,
    min_interval: ChronoDuration,
    max_interval: ChronoDuration,
) -> ChronoDuration {
    if value < min_interval {
        min_interval
    } else if value > max_interval {
        max_interval
    } else {
        value
    }
}

fn parse_max_age(cache_control: &str) -> Option<u64> {
    cache_control.split(',').find_map(|part| {
        let part = part.trim();
        let value = part
            .strip_prefix("s-maxage=")
            .or_else(|| part.strip_prefix("max-age="))?;
        value.parse::<u64>().ok()
    })
}

fn parse_http_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(value)
        .map(|value| value.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            DateTime::parse_from_rfc3339(value)
                .map(|value| value.with_timezone(&Utc))
                .ok()
        })
}

fn entry_title(entry: &Entry) -> String {
    entry
        .title
        .as_ref()
        .map(|title| title.content.clone())
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "Untitled Article".to_string())
}

fn entry_summary(entry: &Entry) -> String {
    entry
        .summary
        .as_ref()
        .map(|summary| summary.content.clone())
        .or_else(|| {
            entry
                .content
                .as_ref()
                .and_then(|content| content.body.as_ref().cloned())
        })
        .unwrap_or_default()
}

fn entry_url(entry: &Entry) -> String {
    entry
        .links
        .iter()
        .find(|link| link.rel.as_deref() == Some("alternate"))
        .or_else(|| entry.links.first())
        .map(|link| link.href.clone())
        .unwrap_or_default()
}

fn dedupe_key(feed_kind: &FeedKind, entry: &Entry) -> String {
    let material = if !entry.id.trim().is_empty() {
        entry.id.clone()
    } else {
        format!(
            "{feed_kind:?}|{}|{}|{}",
            entry_title(entry),
            entry_url(entry),
            entry
                .published
                .or(entry.updated)
                .map(|timestamp| timestamp.to_rfc3339())
                .unwrap_or_default()
        )
    };

    Uuid::new_v5(&Uuid::NAMESPACE_URL, material.as_bytes()).to_string()
}

fn min_fetch_interval() -> ChronoDuration {
    ChronoDuration::minutes(MIN_FETCH_INTERVAL_MINUTES)
}

fn max_fetch_interval() -> ChronoDuration {
    ChronoDuration::hours(MAX_FETCH_INTERVAL_HOURS)
}

fn article_retention_duration() -> ChronoDuration {
    ChronoDuration::days(ARTICLE_RETENTION_DAYS)
}

fn api_error(error: ApiError) -> SchedulerError {
    SchedulerError::Api(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::TempDir;

    use super::*;
    use crate::{
        config::{AppConfig, LlmAgentConfig, LlmConfig},
        domain::{Article, CreateSourceRequest, ProcessingStatus},
        state::AppState,
    };

    struct StubFeedValidator;

    #[async_trait]
    impl FeedValidator for StubFeedValidator {
        async fn validate(&self, feed_url: &str) -> Result<ValidatedFeed, ValidationError> {
            Ok(ValidatedFeed {
                title: feed_url.to_string(),
                feed_kind: FeedKind::Rss,
            })
        }
    }

    struct FakeFetcher {
        responses: Mutex<VecDeque<Result<FetchResponse, SchedulerError>>>,
    }

    #[async_trait]
    impl FeedFetcher for FakeFetcher {
        async fn fetch(&self, _request: FetchRequest) -> Result<FetchResponse, SchedulerError> {
            self.responses
                .lock()
                .await
                .pop_front()
                .expect("fake fetcher should have a queued response")
        }
    }

    #[tokio::test]
    async fn scheduler_fetches_due_source_and_deduplicates_articles() {
        let (_temp_dir, state) = test_state().await;
        let source = state
            .create_source(CreateSourceRequest {
                title: Some("Rust Feed".to_string()),
                feed_url: "https://example.com/feed.xml".to_string(),
                enabled: Some(true),
                assigned_agent_id: None,
            })
            .await
            .unwrap();

        let fetcher = Arc::new(FakeFetcher {
            responses: Mutex::new(VecDeque::from([
                Ok(FetchResponse {
                    status: FetchStatus::Modified,
                    body: Some(sample_rss().into_bytes()),
                    etag: Some("\"v1\"".to_string()),
                    last_modified: Some("Tue, 31 Mar 2026 15:00:00 GMT".to_string()),
                    cache_control: Some("max-age=120".to_string()),
                    expires: None,
                }),
                Ok(FetchResponse {
                    status: FetchStatus::Modified,
                    body: Some(sample_rss().into_bytes()),
                    etag: Some("\"v2\"".to_string()),
                    last_modified: Some("Tue, 31 Mar 2026 15:02:00 GMT".to_string()),
                    cache_control: Some("max-age=120".to_string()),
                    expires: None,
                }),
            ])),
        });
        let scheduler = Scheduler::with_fetcher(state.clone(), fetcher);

        assert_eq!(scheduler.run_once().await.unwrap(), 1);
        let first_batch = state
            .list_articles(crate::domain::ArticleQuery {
                source_id: Some(source.id),
                favorited: None,
                bookmarked: None,
            })
            .await
            .unwrap();
        assert_eq!(first_batch.len(), 1);

        state
            .apply_source_fetch_update(
                source.id,
                SourceFetchUpdate {
                    last_fetch_at: source.last_fetch_at,
                    next_fetch_at: Utc::now() - ChronoDuration::minutes(1),
                    etag: Some("\"v1\"".to_string()),
                    last_modified: Some("Tue, 31 Mar 2026 15:00:00 GMT".to_string()),
                    validation_status: "validated".to_string(),
                },
            )
            .await
            .unwrap();

        assert_eq!(scheduler.run_once().await.unwrap(), 1);
        let second_batch = state
            .list_articles(crate::domain::ArticleQuery {
                source_id: Some(source.id),
                favorited: None,
                bookmarked: None,
            })
            .await
            .unwrap();
        assert_eq!(second_batch.len(), 1);
    }

    #[tokio::test]
    async fn cleanup_removes_only_expired_unbookmarked_articles() {
        let (_temp_dir, state) = test_state().await;
        let source = state
            .create_source(CreateSourceRequest {
                title: Some("Retention Feed".to_string()),
                feed_url: "https://example.com/retention.xml".to_string(),
                enabled: Some(true),
                assigned_agent_id: None,
            })
            .await
            .unwrap();
        state
            .apply_source_fetch_update(
                source.id,
                SourceFetchUpdate {
                    last_fetch_at: Some(Utc::now()),
                    next_fetch_at: Utc::now() + ChronoDuration::hours(12),
                    etag: None,
                    last_modified: None,
                    validation_status: "validated".to_string(),
                },
            )
            .await
            .unwrap();

        let old_unbookmarked_id = Uuid::new_v4();
        let old_bookmarked_id = Uuid::new_v4();
        let recent_id = Uuid::new_v4();
        let now = Utc::now();

        state
            .insert_article(Article {
                id: old_unbookmarked_id,
                source_id: source.id,
                title: "Expired".to_string(),
                summary: "Should be deleted".to_string(),
                url: "https://example.com/expired".to_string(),
                published_at: Some(now - ChronoDuration::days(31)),
                fetched_at: now - ChronoDuration::days(31),
                bookmarked: false,
                llm_status: ProcessingStatus::Pending,
                llm_summary: None,
                llm_error: None,
            })
            .await
            .unwrap();
        state
            .insert_article(Article {
                id: old_bookmarked_id,
                source_id: source.id,
                title: "Bookmarked".to_string(),
                summary: "Should stay".to_string(),
                url: "https://example.com/bookmarked".to_string(),
                published_at: Some(now - ChronoDuration::days(31)),
                fetched_at: now - ChronoDuration::days(31),
                bookmarked: true,
                llm_status: ProcessingStatus::Pending,
                llm_summary: None,
                llm_error: None,
            })
            .await
            .unwrap();
        state
            .insert_article(Article {
                id: recent_id,
                source_id: source.id,
                title: "Recent".to_string(),
                summary: "Should stay".to_string(),
                url: "https://example.com/recent".to_string(),
                published_at: Some(now - ChronoDuration::days(2)),
                fetched_at: now - ChronoDuration::days(2),
                bookmarked: false,
                llm_status: ProcessingStatus::Pending,
                llm_summary: None,
                llm_error: None,
            })
            .await
            .unwrap();

        let scheduler = Scheduler::with_fetcher(
            state.clone(),
            Arc::new(FakeFetcher {
                responses: Mutex::new(VecDeque::new()),
            }),
        );
        assert_eq!(scheduler.run_once().await.unwrap(), 0);

        let all_articles = state
            .list_articles(crate::domain::ArticleQuery {
                source_id: Some(source.id),
                favorited: None,
                bookmarked: None,
            })
            .await
            .unwrap();

        assert_eq!(all_articles.len(), 2);
        assert!(
            all_articles
                .iter()
                .all(|article| article.id != old_unbookmarked_id)
        );
        assert!(
            all_articles
                .iter()
                .any(|article| article.id == old_bookmarked_id)
        );
        assert!(all_articles.iter().any(|article| article.id == recent_id));
    }

    async fn test_state() -> (TempDir, AppState) {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = AppConfig {
            config_path: temp_dir.path().join("config.toml"),
            database_path: temp_dir.path().join("scheduler-test.db"),
            llm: LlmConfig {
                api_key: Some("test-key".to_string()),
                batch_poll_interval_seconds: 1,
                batch_submit_size: 8,
                retry_limit: 3,
                agents: vec![LlmAgentConfig {
                    id: "gemini-brief".to_string(),
                    label: "Gemini Brief".to_string(),
                    provider: "gemini".to_string(),
                    model: "gemini-2.5-flash".to_string(),
                    system_prompt: Some("Summarize the article.".to_string()),
                    batch_enabled: true,
                }],
            },
        };
        let state = AppState::from_config(config, Arc::new(StubFeedValidator))
            .await
            .unwrap();
        (temp_dir, state)
    }

    fn sample_rss() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example Feed</title>
    <link>https://example.com</link>
    <description>Example</description>
    <item>
      <guid>article-1</guid>
      <title>First article</title>
      <link>https://example.com/articles/1</link>
      <description>Hello world</description>
      <pubDate>Tue, 31 Mar 2026 15:00:00 GMT</pubDate>
    </item>
  </channel>
</rss>"#
            .to_string()
    }
}
