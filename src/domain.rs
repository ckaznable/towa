use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeedKind {
    Rss,
    Atom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStatus {
    Pending,
    Processing,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSummary {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub model: String,
    pub batch_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: Uuid,
    pub title: String,
    pub feed_url: String,
    pub feed_kind: FeedKind,
    pub enabled: bool,
    pub assigned_agent_id: Option<String>,
    pub validation_status: String,
    pub last_fetch_at: Option<DateTime<Utc>>,
    pub next_fetch_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub id: Uuid,
    pub source_id: Uuid,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
    pub bookmarked: bool,
    pub llm_status: ProcessingStatus,
    pub llm_summary: Option<String>,
    pub llm_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceListResponse {
    pub items: Vec<Source>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListResponse {
    pub items: Vec<AgentSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleListItem {
    pub id: Uuid,
    pub source_id: Uuid,
    pub source_title: String,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub read: bool,
    pub read_at: Option<DateTime<Utc>>,
    pub favorited: bool,
    pub bookmarked: bool,
    pub llm_status: ProcessingStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleListResponse {
    pub items: Vec<ArticleListItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleDetail {
    pub id: Uuid,
    pub source_id: Uuid,
    pub source_title: String,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub read: bool,
    pub read_at: Option<DateTime<Utc>>,
    pub favorited: bool,
    pub bookmarked: bool,
    pub llm_status: ProcessingStatus,
    pub llm_summary: Option<String>,
    pub llm_error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSourceRequest {
    pub title: Option<String>,
    pub feed_url: String,
    pub enabled: Option<bool>,
    pub assigned_agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSourceRequest {
    pub title: Option<String>,
    pub feed_url: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct AssignAgentRequest {
    pub assigned_agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ArticleQuery {
    pub source_id: Option<Uuid>,
    pub favorited: Option<bool>,
    pub bookmarked: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct FavoriteRequest {
    pub favorited: Option<bool>,
    pub bookmarked: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ReadStateRequest {
    pub read: bool,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminProcessingOverview {
    pub pending_jobs: Vec<PendingJobSummary>,
    pub active_batches: Vec<ActiveBatchSummary>,
    pub failed_jobs: Vec<FailedJobSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingJobSummary {
    pub article_id: Uuid,
    pub agent_id: String,
    pub source_title: String,
    pub title: String,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveBatchSummary {
    pub batch_name: String,
    pub agent_id: String,
    pub article_count: usize,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedJobSummary {
    pub article_id: Uuid,
    pub agent_id: Option<String>,
    pub source_title: String,
    pub title: String,
    pub attempts: u32,
    pub last_error: String,
    pub last_batch_name: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct RetryBatchRequest {
    pub batch_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RetryResult {
    pub retried: usize,
}

impl ArticleDetail {
    pub fn from_article(
        article: Article,
        source_title: String,
        available_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: article.id,
            source_id: article.source_id,
            source_title,
            title: article.title,
            summary: article.summary,
            url: article.url,
            published_at: article.published_at,
            fetched_at: article.fetched_at,
            available_at,
            read: article.read_at.is_some(),
            read_at: article.read_at,
            favorited: article.bookmarked,
            bookmarked: article.bookmarked,
            llm_status: article.llm_status,
            llm_summary: article.llm_summary,
            llm_error: article.llm_error,
        }
    }
}

impl ArticleListItem {
    pub fn from_article(
        article: Article,
        source_title: String,
        available_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: article.id,
            source_id: article.source_id,
            source_title,
            title: article.title,
            summary: article.summary,
            url: article.url,
            published_at: article.published_at,
            fetched_at: article.fetched_at,
            available_at,
            read: article.read_at.is_some(),
            read_at: article.read_at,
            favorited: article.bookmarked,
            bookmarked: article.bookmarked,
            llm_status: article.llm_status,
        }
    }
}

impl ArticleQuery {
    pub fn favorite_filter(&self) -> Option<bool> {
        self.favorited.or(self.bookmarked)
    }
}

impl FavoriteRequest {
    pub fn favorite_state(&self) -> Option<bool> {
        self.favorited.or(self.bookmarked)
    }
}
