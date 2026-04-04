use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use feed_rs::parser;
use serde::Serialize;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

use crate::{
    config::{AppConfig, ConfigError, LlmAgentConfig, LlmConfig},
    db::{
        ActiveBatch, BatchArticleOutput, Database, DbError, FetchedArticleInput,
        PendingProcessingJob, SourceFetchState, SourceFetchUpdate,
    },
    domain::{
        ActiveBatchSummary, AdminProcessingOverview, AgentSummary, Article, ArticleDetail,
        ArticleListItem, ArticleQuery, AssignAgentRequest, CreateSourceRequest, FailedJobSummary,
        FeedKind, PendingJobSummary, Source, UpdateSourceRequest,
    },
};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    database: Database,
    validator: Arc<dyn FeedValidator>,
    config: AppConfig,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Internal(String),
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("feed_url must be a valid absolute http/https URL")]
    InvalidUrl,
    #[error("feed source request failed: {0}")]
    RequestFailed(String),
    #[error("feed source returned unsupported content")]
    UnsupportedFormat,
    #[error("feed source could not be parsed: {0}")]
    ParseFailed(String),
}

#[derive(Debug, Error)]
pub enum AppInitError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Database(#[from] DbError),
}

#[derive(Debug)]
pub struct ValidatedFeed {
    pub title: String,
    pub feed_kind: FeedKind,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[async_trait]
pub trait FeedValidator: Send + Sync {
    async fn validate(&self, feed_url: &str) -> Result<ValidatedFeed, ValidationError>;
}

pub struct HttpFeedValidator {
    client: reqwest::Client,
}

impl AppState {
    pub async fn new() -> Result<Self, AppInitError> {
        let config = AppConfig::load()?;
        Self::from_config(config, Arc::new(HttpFeedValidator::new())).await
    }

    pub async fn from_config(
        config: AppConfig,
        validator: Arc<dyn FeedValidator>,
    ) -> Result<Self, AppInitError> {
        let database = Database::new(config.database_path.clone()).await?;

        Ok(Self {
            inner: Arc::new(AppStateInner {
                database,
                validator,
                config,
            }),
        })
    }

    pub fn list_agents(&self) -> Vec<AgentSummary> {
        self.inner.config.agent_summaries()
    }

    pub fn llm_config(&self) -> LlmConfig {
        self.inner.config.llm.clone()
    }

    pub fn find_agent(&self, agent_id: &str) -> Option<LlmAgentConfig> {
        self.inner
            .config
            .llm
            .agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .cloned()
    }

    pub fn config_path(&self) -> &std::path::Path {
        &self.inner.config.config_path
    }

    pub fn database_path(&self) -> &std::path::Path {
        &self.inner.config.database_path
    }

    pub async fn list_sources(&self) -> Result<Vec<Source>, ApiError> {
        self.inner
            .database
            .list_sources()
            .await
            .map_err(internal_error)
    }

    pub async fn list_due_sources(
        &self,
        now: chrono::DateTime<Utc>,
    ) -> Result<Vec<Source>, ApiError> {
        self.inner
            .database
            .list_due_sources(now, 32)
            .await
            .map_err(internal_error)
    }

    pub async fn get_source(&self, id: Uuid) -> Result<Source, ApiError> {
        self.inner
            .database
            .get_source(id)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| ApiError::NotFound(format!("source `{id}` not found")))
    }

    pub async fn create_source(&self, request: CreateSourceRequest) -> Result<Source, ApiError> {
        let validated = self
            .inner
            .validator
            .validate(&request.feed_url)
            .await
            .map_err(|error| ApiError::Validation(error.to_string()))?;
        self.ensure_agent_exists(request.assigned_agent_id.as_deref())?;

        let now = Utc::now();
        let source = Source {
            id: Uuid::new_v4(),
            title: request.title.unwrap_or(validated.title),
            feed_url: request.feed_url,
            feed_kind: validated.feed_kind,
            enabled: request.enabled.unwrap_or(true),
            assigned_agent_id: request.assigned_agent_id,
            validation_status: "validated".to_string(),
            last_fetch_at: None,
            next_fetch_at: None,
            created_at: now,
            updated_at: now,
        };

        self.inner
            .database
            .insert_source(&source)
            .await
            .map_err(internal_error)?;

        Ok(source)
    }

    pub async fn update_source(
        &self,
        id: Uuid,
        request: UpdateSourceRequest,
    ) -> Result<Source, ApiError> {
        let mut source = self.get_source(id).await?;

        if let Some(feed_url) = request.feed_url {
            let validated = self
                .inner
                .validator
                .validate(&feed_url)
                .await
                .map_err(|error| ApiError::Validation(error.to_string()))?;
            source.feed_url = feed_url;
            source.feed_kind = validated.feed_kind;
            if request.title.is_none() && source.title.trim().is_empty() {
                source.title = validated.title;
            }
            source.validation_status = "validated".to_string();
        }

        if let Some(title) = request.title {
            source.title = title;
        }

        if let Some(enabled) = request.enabled {
            source.enabled = enabled;
        }

        source.updated_at = Utc::now();
        self.inner
            .database
            .update_source(&source)
            .await
            .map_err(internal_error)?;

        Ok(source)
    }

    pub async fn delete_source(&self, id: Uuid) -> Result<(), ApiError> {
        let deleted = self
            .inner
            .database
            .delete_source(id)
            .await
            .map_err(internal_error)?;
        if !deleted {
            return Err(ApiError::NotFound(format!("source `{id}` not found")));
        }
        Ok(())
    }

    pub async fn assign_agent(
        &self,
        id: Uuid,
        request: AssignAgentRequest,
    ) -> Result<Source, ApiError> {
        self.ensure_agent_exists(request.assigned_agent_id.as_deref())?;

        let mut source = self.get_source(id).await?;
        source.assigned_agent_id = request.assigned_agent_id;
        source.updated_at = Utc::now();

        self.inner
            .database
            .update_source(&source)
            .await
            .map_err(internal_error)?;

        Ok(source)
    }

    pub async fn list_articles(
        &self,
        query: ArticleQuery,
    ) -> Result<Vec<ArticleListItem>, ApiError> {
        self.inner
            .database
            .list_articles(query)
            .await
            .map_err(internal_error)
    }

    pub async fn get_article(&self, id: Uuid) -> Result<ArticleDetail, ApiError> {
        self.inner
            .database
            .get_article(id)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| ApiError::NotFound(format!("article `{id}` not found")))
    }

    pub async fn set_bookmark(
        &self,
        id: Uuid,
        bookmarked: bool,
    ) -> Result<ArticleDetail, ApiError> {
        self.set_favorite(id, bookmarked).await
    }

    pub async fn set_favorite(&self, id: Uuid, favorited: bool) -> Result<ArticleDetail, ApiError> {
        let updated = self
            .inner
            .database
            .set_favorite(id, favorited)
            .await
            .map_err(internal_error)?;
        if !updated {
            return Err(ApiError::NotFound(format!("article `{id}` not found")));
        }
        self.get_article(id).await
    }

    pub async fn set_read_state(&self, id: Uuid, read: bool) -> Result<ArticleDetail, ApiError> {
        let updated = self
            .inner
            .database
            .set_read_state(id, read)
            .await
            .map_err(internal_error)?;
        if !updated {
            return Err(ApiError::NotFound(format!("article `{id}` not found")));
        }
        self.get_article(id).await
    }

    pub async fn list_favorites(&self) -> Result<Vec<ArticleListItem>, ApiError> {
        self.list_articles(ArticleQuery {
            source_id: None,
            favorited: Some(true),
            bookmarked: None,
        })
        .await
    }

    pub async fn cleanup_expired_articles(
        &self,
        cutoff: chrono::DateTime<Utc>,
    ) -> Result<usize, ApiError> {
        self.inner
            .database
            .delete_expired_non_favorited_articles(cutoff)
            .await
            .map_err(internal_error)
    }

    pub async fn insert_article(&self, article: Article) -> Result<(), ApiError> {
        self.inner
            .database
            .insert_article(&article)
            .await
            .map_err(internal_error)
    }

    pub async fn get_source_fetch_state(
        &self,
        source_id: Uuid,
    ) -> Result<SourceFetchState, ApiError> {
        self.inner
            .database
            .get_source_fetch_state(source_id)
            .await
            .map_err(internal_error)
    }

    pub async fn apply_source_fetch_update(
        &self,
        source_id: Uuid,
        update: SourceFetchUpdate,
    ) -> Result<(), ApiError> {
        self.inner
            .database
            .apply_source_fetch_update(source_id, update)
            .await
            .map_err(internal_error)
    }

    pub async fn upsert_fetched_article(
        &self,
        input: FetchedArticleInput,
    ) -> Result<Article, ApiError> {
        self.inner
            .database
            .upsert_fetched_article(input)
            .await
            .map_err(internal_error)
    }

    pub async fn list_pending_processing_jobs(
        &self,
        limit: usize,
    ) -> Result<Vec<PendingProcessingJob>, ApiError> {
        self.inner
            .database
            .list_pending_processing_jobs(limit)
            .await
            .map_err(internal_error)
    }

    pub async fn admin_processing_overview(&self) -> Result<AdminProcessingOverview, ApiError> {
        let pending_jobs = self.list_pending_processing_jobs(64).await?;
        let active_batches = self.list_active_batches().await?;
        let failed_jobs = self
            .inner
            .database
            .list_failed_processing_jobs(64)
            .await
            .map_err(internal_error)?;

        Ok(AdminProcessingOverview {
            pending_jobs: pending_jobs
                .into_iter()
                .map(|job| PendingJobSummary {
                    article_id: job.article_id,
                    agent_id: job.agent_id,
                    source_title: job.source_title,
                    title: job.title,
                    published_at: job.published_at,
                })
                .collect(),
            active_batches: active_batches
                .into_iter()
                .map(|batch| ActiveBatchSummary {
                    batch_name: batch.name,
                    agent_id: batch.agent_id,
                    article_count: batch.article_count,
                    updated_at: batch.updated_at,
                })
                .collect(),
            failed_jobs: failed_jobs
                .into_iter()
                .map(|job| FailedJobSummary {
                    article_id: job.article_id,
                    agent_id: job.agent_id,
                    source_title: job.source_title,
                    title: job.title,
                    attempts: job.attempts,
                    last_error: job.last_error,
                    last_batch_name: job.last_batch_name,
                    updated_at: job.updated_at,
                })
                .collect(),
        })
    }

    pub async fn mark_batch_started(
        &self,
        article_ids: &[Uuid],
        batch_name: &str,
    ) -> Result<(), ApiError> {
        self.inner
            .database
            .mark_batch_started(article_ids, batch_name)
            .await
            .map_err(internal_error)
    }

    pub async fn record_processing_failure(
        &self,
        article_ids: &[Uuid],
        error_message: &str,
        retry_limit: u32,
    ) -> Result<(), ApiError> {
        self.inner
            .database
            .record_processing_failure(article_ids, error_message, retry_limit)
            .await
            .map_err(internal_error)
    }

    pub async fn list_active_batches(&self) -> Result<Vec<ActiveBatch>, ApiError> {
        self.inner
            .database
            .list_active_batches()
            .await
            .map_err(internal_error)
    }

    pub async fn apply_batch_outputs(
        &self,
        batch_name: &str,
        outputs: &[BatchArticleOutput],
        retry_limit: u32,
    ) -> Result<(), ApiError> {
        self.inner
            .database
            .apply_batch_outputs(batch_name, outputs, retry_limit)
            .await
            .map_err(internal_error)
    }

    pub async fn fail_batch(
        &self,
        batch_name: &str,
        error_message: &str,
        retry_limit: u32,
    ) -> Result<(), ApiError> {
        self.inner
            .database
            .fail_batch(batch_name, error_message, retry_limit)
            .await
            .map_err(internal_error)
    }

    pub async fn retry_article_processing(&self, article_id: Uuid) -> Result<usize, ApiError> {
        let retried = self
            .inner
            .database
            .retry_article_processing(article_id)
            .await
            .map_err(internal_error)?;
        Ok(usize::from(retried))
    }

    pub async fn retry_batch_processing(&self, batch_name: &str) -> Result<usize, ApiError> {
        self.inner
            .database
            .retry_batch_processing(batch_name)
            .await
            .map_err(internal_error)
    }

    fn ensure_agent_exists(&self, assigned_agent_id: Option<&str>) -> Result<(), ApiError> {
        let Some(agent_id) = assigned_agent_id else {
            return Ok(());
        };

        if self
            .inner
            .config
            .llm
            .agents
            .iter()
            .any(|agent| agent.id == agent_id)
        {
            return Ok(());
        }

        Err(ApiError::Validation(format!(
            "assigned agent `{agent_id}` is not available"
        )))
    }
}

impl HttpFeedValidator {
    fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("towa/0.1.0")
            .build()
            .expect("http client should build");
        Self { client }
    }
}

#[async_trait]
impl FeedValidator for HttpFeedValidator {
    async fn validate(&self, feed_url: &str) -> Result<ValidatedFeed, ValidationError> {
        let parsed_url = Url::parse(feed_url).map_err(|_| ValidationError::InvalidUrl)?;
        let fallback_title = parsed_url
            .host_str()
            .map(str::to_string)
            .unwrap_or_else(|| "Untitled Feed".to_string());
        match parsed_url.scheme() {
            "http" | "https" => {}
            _ => return Err(ValidationError::InvalidUrl),
        }

        let response = self
            .client
            .get(parsed_url)
            .send()
            .await
            .map_err(|error| ValidationError::RequestFailed(error.to_string()))?;

        if !response.status().is_success() {
            return Err(ValidationError::RequestFailed(
                response.status().to_string(),
            ));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|error| ValidationError::RequestFailed(error.to_string()))?;
        let feed = parser::parse(bytes.as_ref())
            .map_err(|error| ValidationError::ParseFailed(error.to_string()))?;
        let body = String::from_utf8_lossy(bytes.as_ref()).to_ascii_lowercase();
        let feed_kind = infer_feed_kind(&body)?;
        let title = feed
            .title
            .map(|title| title.content)
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(fallback_title);

        Ok(ValidatedFeed { title, feed_kind })
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (
            status,
            Json(ErrorResponse {
                error: self.to_string(),
            }),
        )
            .into_response()
    }
}

fn infer_feed_kind(body: &str) -> Result<FeedKind, ValidationError> {
    if body.contains("<feed") {
        return Ok(FeedKind::Atom);
    }
    if body.contains("<rss") || body.contains("<rdf:rdf") {
        return Ok(FeedKind::Rss);
    }

    Err(ValidationError::UnsupportedFormat)
}

fn internal_error(error: DbError) -> ApiError {
    ApiError::Internal(error.to_string())
}
