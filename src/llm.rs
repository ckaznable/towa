use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::Utc;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time;
use tracing::debug;
use uuid::Uuid;

use crate::{
    config::LlmAgentConfig,
    db::{BatchArticleOutput, PendingProcessingJob},
    state::{ApiError, AppState},
};

#[derive(Clone)]
pub struct LlmWorker {
    state: AppState,
    provider: Arc<dyn BatchProvider>,
    batch_submit_size: usize,
    retry_limit: u32,
    poll_interval_seconds: u64,
}

#[derive(Debug, Error, Clone)]
pub enum LlmWorkerError {
    #[error("{0}")]
    Api(String),
    #[error("{0}")]
    Provider(String),
}

#[derive(Debug, Clone)]
pub enum BatchPollResult {
    Pending,
    Completed(Vec<BatchArticleOutput>),
    Failed(String),
}

#[async_trait]
pub trait BatchProvider: Send + Sync {
    async fn submit_batch(
        &self,
        agent: &LlmAgentConfig,
        jobs: &[PendingProcessingJob],
    ) -> Result<String, LlmWorkerError>;

    async fn poll_batch(&self, batch_name: &str) -> Result<BatchPollResult, LlmWorkerError>;
}

pub struct GeminiBatchClient {
    client: reqwest::Client,
    api_key: String,
}

impl LlmWorker {
    pub fn new(state: AppState) -> Option<Self> {
        let llm_config = state.llm_config();
        let api_key = llm_config.api_key.clone()?;

        Some(Self {
            state,
            provider: Arc::new(GeminiBatchClient::new(api_key)),
            batch_submit_size: llm_config.batch_submit_size,
            retry_limit: llm_config.retry_limit,
            poll_interval_seconds: llm_config.batch_poll_interval_seconds,
        })
    }

    #[cfg(test)]
    pub(crate) fn with_provider(state: AppState, provider: Arc<dyn BatchProvider>) -> Self {
        let llm_config = state.llm_config();
        Self {
            state,
            provider,
            batch_submit_size: llm_config.batch_submit_size,
            retry_limit: llm_config.retry_limit,
            poll_interval_seconds: llm_config.batch_poll_interval_seconds,
        }
    }

    pub async fn run_loop(self) -> Result<(), LlmWorkerError> {
        let mut interval = time::interval(Duration::from_secs(self.poll_interval_seconds));
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            let work = self.run_once().await?;
            if work > 0 {
                tracing::info!("llm worker processed {work} batch step(s)");
            }
        }
    }

    pub async fn run_once(&self) -> Result<usize, LlmWorkerError> {
        let active_batches = self.state.list_active_batches().await.map_err(api_error)?;
        if !active_batches.is_empty() {
            tracing::info!(
                active_batch_count = active_batches.len(),
                "llm worker found active batch(es) to poll"
            );
        }
        let mut processed = 0usize;
        processed += self.submit_pending_jobs().await?;
        processed += self.poll_active_batches(active_batches).await?;
        Ok(processed)
    }

    async fn poll_active_batches(
        &self,
        active_batches: Vec<crate::db::ActiveBatch>,
    ) -> Result<usize, LlmWorkerError> {
        let mut processed = 0usize;

        for batch in active_batches {
            tracing::info!(batch_name = %batch.name, "polling llm batch");
            match self.provider.poll_batch(&batch.name).await {
                Ok(BatchPollResult::Pending) => {
                    tracing::debug!(batch_name = %batch.name, "llm batch still running");
                }
                Ok(BatchPollResult::Completed(outputs)) => {
                    let success_count = outputs
                        .iter()
                        .filter(|output| output.summary.is_some() && output.error.is_none())
                        .count();
                    let error_count = outputs
                        .iter()
                        .filter(|output| output.error.is_some())
                        .count();
                    let empty_output_count = outputs
                        .iter()
                        .filter(|output| output.summary.is_none() && output.error.is_none())
                        .count();
                    self.state
                        .apply_batch_outputs(&batch.name, &outputs, self.retry_limit)
                        .await
                        .map_err(api_error)?;
                    tracing::info!(
                        batch_name = %batch.name,
                        output_count = outputs.len(),
                        success_count,
                        error_count,
                        empty_output_count,
                        "llm batch completed"
                    );
                    processed += 1;
                }
                Ok(BatchPollResult::Failed(error_message)) => {
                    self.state
                        .fail_batch(&batch.name, &error_message, self.retry_limit)
                        .await
                        .map_err(api_error)?;
                    tracing::warn!(
                        batch_name = %batch.name,
                        error = %error_message,
                        "llm batch failed"
                    );
                    processed += 1;
                }
                Err(error) => {
                    tracing::warn!(batch_name = %batch.name, error = %error, "llm batch poll failed");
                }
            }
        }

        Ok(processed)
    }

    async fn submit_pending_jobs(&self) -> Result<usize, LlmWorkerError> {
        let pending_jobs = self
            .state
            .list_pending_processing_jobs(self.batch_submit_size.saturating_mul(8).max(1))
            .await
            .map_err(api_error)?;
        if pending_jobs.is_empty() {
            return Ok(0);
        }
        let pending_job_count = pending_jobs.len();

        let mut grouped: BTreeMap<String, Vec<PendingProcessingJob>> = BTreeMap::new();
        for job in pending_jobs {
            grouped.entry(job.agent_id.clone()).or_default().push(job);
        }

        tracing::info!(
            pending_job_count,
            agent_group_count = grouped.len(),
            "llm worker found pending article(s)"
        );

        let mut processed = 0usize;
        for (agent_id, mut jobs) in grouped {
            jobs.truncate(self.batch_submit_size);
            let article_ids = jobs.iter().map(|job| job.article_id).collect::<Vec<_>>();
            let source_titles = jobs
                .iter()
                .map(|job| job.source_title.as_str())
                .collect::<Vec<_>>();
            let Some(agent) = self.state.find_agent(&agent_id) else {
                self.state
                    .record_processing_failure(
                        &article_ids,
                        "assigned agent is no longer available in config.toml",
                        self.retry_limit,
                    )
                    .await
                    .map_err(api_error)?;
                tracing::warn!(
                    agent_id = %agent_id,
                    article_count = article_ids.len(),
                    source_titles = ?source_titles,
                    "assigned agent missing; pending jobs marked for retry/failure"
                );
                processed += 1;
                continue;
            };

            tracing::info!(
                agent_id = %agent.id,
                agent_label = %agent.label,
                model = %agent.model,
                article_count = article_ids.len(),
                source_titles = ?source_titles,
                "submitting llm batch"
            );
            match self.provider.submit_batch(&agent, &jobs).await {
                Ok(batch_name) => {
                    self.state
                        .mark_batch_started(&article_ids, &batch_name)
                        .await
                        .map_err(api_error)?;
                    tracing::info!(
                        agent_id = %agent.id,
                        batch_name = %batch_name,
                        article_count = article_ids.len(),
                        "llm batch submitted"
                    );
                    processed += 1;
                }
                Err(error) => {
                    self.state
                        .record_processing_failure(
                            &article_ids,
                            &error.to_string(),
                            self.retry_limit,
                        )
                        .await
                        .map_err(api_error)?;
                    tracing::warn!(
                        agent_id = %agent.id,
                        article_count = article_ids.len(),
                        error = %error,
                        "llm batch submission failed"
                    );
                    processed += 1;
                }
            }
        }

        Ok(processed)
    }
}

impl GeminiBatchClient {
    fn new(api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("towa-llm/0.1.0")
            .build()
            .expect("llm http client should build");
        Self { client, api_key }
    }
}

#[async_trait]
impl BatchProvider for GeminiBatchClient {
    async fn submit_batch(
        &self,
        agent: &LlmAgentConfig,
        jobs: &[PendingProcessingJob],
    ) -> Result<String, LlmWorkerError> {
        let request = GeminiBatchSubmitRequest {
            batch: GeminiBatchRequest {
                display_name: format!("towa-{}-{}", agent.id, Utc::now().timestamp()),
                input_config: GeminiBatchInputConfig {
                    requests: GeminiBatchRequests {
                        requests: jobs
                            .iter()
                            .map(|job| GeminiBatchRequestItem {
                                request: GeminiGenerateContentRequest {
                                    system_instruction: agent.system_prompt.as_ref().map(
                                        |prompt| GeminiInstruction {
                                            parts: vec![GeminiPart {
                                                text: Some(prompt.clone()),
                                            }],
                                        },
                                    ),
                                    contents: vec![GeminiContent {
                                        role: "user".to_string(),
                                        parts: vec![GeminiPart {
                                            text: Some(article_prompt(job)),
                                        }],
                                    }],
                                },
                                metadata: GeminiRequestMetadata {
                                    key: job.article_id.to_string(),
                                },
                            })
                            .collect(),
                    },
                },
            },
        };

        let response = self
            .client
            .post(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:batchGenerateContent",
                agent.model
            ))
            .header("x-goog-api-key", &self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(provider_error)?;

        if !response.status().is_success() {
            return Err(LlmWorkerError::Provider(http_error(response).await));
        }

        let batch = parse_json_response::<GeminiBatchCreateResponse>(
            response,
            "gemini batch submit response",
        )
        .await?;
        if batch.name.trim().is_empty() {
            return Err(LlmWorkerError::Provider(
                "gemini batch create response did not include an operation name".to_string(),
            ));
        }

        Ok(batch.name)
    }

    async fn poll_batch(&self, batch_name: &str) -> Result<BatchPollResult, LlmWorkerError> {
        let response = self
            .client
            .get(format!(
                "https://generativelanguage.googleapis.com/v1beta/{batch_name}"
            ))
            .header("x-goog-api-key", &self.api_key)
            .send()
            .await
            .map_err(provider_error)?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(BatchPollResult::Failed(format!(
                "gemini batch `{batch_name}` was not found"
            )));
        }
        if !response.status().is_success() {
            return Err(LlmWorkerError::Provider(http_error(response).await));
        }

        let operation =
            parse_json_response::<GeminiOperation>(response, "gemini batch poll response").await?;
        if !operation.is_done() {
            return Ok(BatchPollResult::Pending);
        }
        if let Some(error) = operation.error {
            return Ok(BatchPollResult::Failed(error.message));
        }

        let response = operation.response.ok_or_else(|| {
            LlmWorkerError::Provider(
                "gemini batch completed without a response payload".to_string(),
            )
        })?;
        let inline = response
            .inlined_responses
            .ok_or_else(|| {
                LlmWorkerError::Provider(
                    "gemini batch completed without inline responses".to_string(),
                )
            })?
            .inlined_responses;

        let outputs = inline
            .into_iter()
            .map(|item| {
                let article_id = item
                    .metadata
                    .as_ref()
                    .and_then(|metadata| Uuid::parse_str(&metadata.key).ok())
                    .ok_or_else(|| {
                        LlmWorkerError::Provider(
                            "gemini inline response is missing metadata.key".to_string(),
                        )
                    })?;

                if let Some(error) = item.error {
                    return Ok(BatchArticleOutput {
                        article_id,
                        summary: None,
                        error: Some(error.message),
                    });
                }

                let summary = item
                    .response
                    .as_ref()
                    .map(extract_summary)
                    .filter(|value| !value.trim().is_empty());

                Ok(BatchArticleOutput {
                    article_id,
                    summary,
                    error: None,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(BatchPollResult::Completed(outputs))
    }
}

fn article_prompt(job: &PendingProcessingJob) -> String {
    let published_at = job
        .published_at
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "unknown".to_string());

    format!(
        "Source: {}\nPublished At: {}\nTitle: {}\nURL: {}\nSummary:\n{}\n\nPlease produce a concise reader-facing summary.",
        job.source_title, published_at, job.title, job.url, job.summary
    )
}

fn extract_summary(response: &GeminiGenerateContentResponse) -> String {
    response
        .candidates
        .iter()
        .filter_map(|candidate| candidate.content.as_ref())
        .flat_map(|content| content.parts.iter())
        .filter_map(|part| part.text.as_deref())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn provider_error(error: reqwest::Error) -> LlmWorkerError {
    LlmWorkerError::Provider(error.to_string())
}

fn api_error(error: ApiError) -> LlmWorkerError {
    LlmWorkerError::Api(error.to_string())
}

async fn parse_json_response<T>(
    response: reqwest::Response,
    context: &'static str,
) -> Result<T, LlmWorkerError>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status();
    let body = response.text().await.map_err(provider_error)?;
    serde_json::from_str::<T>(&body).map_err(|error| {
        debug!(
            status = %status,
            response_context = context,
            raw_body = %body,
            decode_error = %error,
            "failed to decode gemini response body"
        );
        LlmWorkerError::Provider(format!("{context}: error decoding response body: {error}"))
    })
}

async fn http_error(response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if body.trim().is_empty() {
        format!("gemini request failed with status {status}")
    } else {
        format!("gemini request failed with status {status}: {body}")
    }
}

#[derive(Debug, Serialize)]
struct GeminiBatchSubmitRequest {
    batch: GeminiBatchRequest,
}

#[derive(Debug, Serialize)]
struct GeminiBatchRequest {
    display_name: String,
    input_config: GeminiBatchInputConfig,
}

#[derive(Debug, Serialize)]
struct GeminiBatchInputConfig {
    requests: GeminiBatchRequests,
}

#[derive(Debug, Serialize)]
struct GeminiBatchRequests {
    requests: Vec<GeminiBatchRequestItem>,
}

#[derive(Debug, Serialize)]
struct GeminiBatchRequestItem {
    request: GeminiGenerateContentRequest,
    metadata: GeminiRequestMetadata,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GeminiRequestMetadata {
    key: String,
}

#[derive(Debug, Serialize)]
struct GeminiGenerateContentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiInstruction>,
    contents: Vec<GeminiContent>,
}

#[derive(Debug, Serialize)]
struct GeminiInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiBatchCreateResponse {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GeminiOperation {
    #[serde(default)]
    done: bool,
    error: Option<GoogleStatus>,
    response: Option<GeminiOperationResponse>,
    metadata: Option<GeminiBatchMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiOperationResponse {
    #[serde(alias = "inlinedResponses")]
    inlined_responses: Option<GeminiInlineResponses>,
}

#[derive(Debug, Deserialize)]
struct GeminiInlineResponses {
    #[serde(default, alias = "inlinedResponses")]
    inlined_responses: Vec<GeminiInlineResponse>,
}

#[derive(Debug, Deserialize)]
struct GeminiInlineResponse {
    metadata: Option<GeminiRequestMetadata>,
    error: Option<GoogleStatus>,
    response: Option<GeminiGenerateContentResponse>,
}

#[derive(Debug, Deserialize)]
struct GeminiBatchMetadata {
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiGenerateContentResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiOutputContent>,
}

#[derive(Debug, Deserialize)]
struct GeminiOutputContent {
    #[serde(default)]
    parts: Vec<GeminiTextPart>,
}

#[derive(Debug, Deserialize)]
struct GeminiTextPart {
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleStatus {
    message: String,
}

impl GeminiOperation {
    fn is_done(&self) -> bool {
        self.done
            || self
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.state.as_deref())
                .is_some_and(is_terminal_batch_state)
    }
}

fn is_terminal_batch_state(state: &str) -> bool {
    matches!(
        state,
        "JOB_STATE_SUCCEEDED"
            | "JOB_STATE_FAILED"
            | "JOB_STATE_CANCELLED"
            | "JOB_STATE_EXPIRED"
            | "BATCH_STATE_SUCCEEDED"
            | "BATCH_STATE_FAILED"
            | "BATCH_STATE_CANCELLED"
            | "BATCH_STATE_EXPIRED"
    )
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Arc};

    use async_trait::async_trait;
    use chrono::Utc;
    use tempfile::TempDir;
    use tokio::sync::Mutex;

    use super::*;
    use crate::{
        config::{AppConfig, LlmAgentConfig, LlmConfig},
        db::FetchedArticleInput,
        domain::{CreateSourceRequest, FeedKind},
        state::{AppState, FeedValidator, ValidatedFeed, ValidationError},
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

    struct FakeBatchProvider {
        submit_results: Mutex<VecDeque<Result<String, LlmWorkerError>>>,
        poll_results: Mutex<VecDeque<Result<BatchPollResult, LlmWorkerError>>>,
    }

    #[async_trait]
    impl BatchProvider for FakeBatchProvider {
        async fn submit_batch(
            &self,
            _agent: &LlmAgentConfig,
            _jobs: &[PendingProcessingJob],
        ) -> Result<String, LlmWorkerError> {
            self.submit_results
                .lock()
                .await
                .pop_front()
                .expect("submit result should exist")
        }

        async fn poll_batch(&self, _batch_name: &str) -> Result<BatchPollResult, LlmWorkerError> {
            self.poll_results
                .lock()
                .await
                .pop_front()
                .expect("poll result should exist")
        }
    }

    #[tokio::test]
    async fn worker_completes_pending_articles_via_batch_flow() {
        let (_temp_dir, state) = test_state(3).await;
        let source = state
            .create_source(CreateSourceRequest {
                title: Some("LLM Feed".to_string()),
                feed_url: "https://example.com/llm.xml".to_string(),
                enabled: Some(true),
                assigned_agent_id: Some("gemini-brief".to_string()),
            })
            .await
            .unwrap();

        let article = state
            .upsert_fetched_article(FetchedArticleInput {
                source_id: source.id,
                dedupe_key: "article-1".to_string(),
                title: "First article".to_string(),
                summary: "Important details".to_string(),
                url: "https://example.com/articles/1".to_string(),
                published_at: Some(Utc::now()),
                fetched_at: Utc::now(),
            })
            .await
            .unwrap();
        assert_eq!(article.llm_status, crate::domain::ProcessingStatus::Pending);

        let worker = LlmWorker::with_provider(
            state.clone(),
            Arc::new(FakeBatchProvider {
                submit_results: Mutex::new(VecDeque::from([Ok("operations/batch-1".to_string())])),
                poll_results: Mutex::new(VecDeque::from([Ok(BatchPollResult::Completed(vec![
                    BatchArticleOutput {
                        article_id: article.id,
                        summary: Some("Summarized output".to_string()),
                        error: None,
                    },
                ]))])),
            }),
        );

        assert_eq!(worker.run_once().await.unwrap(), 1);
        let processing = state.get_article(article.id).await.unwrap();
        assert_eq!(
            processing.llm_status,
            crate::domain::ProcessingStatus::Processing
        );

        assert_eq!(worker.run_once().await.unwrap(), 1);
        let processed = state.get_article(article.id).await.unwrap();
        assert_eq!(processed.llm_status, crate::domain::ProcessingStatus::Done);
        assert_eq!(processed.llm_summary.as_deref(), Some("Summarized output"));
        assert_eq!(processed.llm_error, None);
    }

    #[tokio::test]
    async fn worker_retries_batch_failures_until_retry_limit() {
        let (_temp_dir, state) = test_state(2).await;
        let source = state
            .create_source(CreateSourceRequest {
                title: Some("Retry Feed".to_string()),
                feed_url: "https://example.com/retry.xml".to_string(),
                enabled: Some(true),
                assigned_agent_id: Some("gemini-brief".to_string()),
            })
            .await
            .unwrap();

        let article = state
            .upsert_fetched_article(FetchedArticleInput {
                source_id: source.id,
                dedupe_key: "article-retry".to_string(),
                title: "Retry article".to_string(),
                summary: "Needs retry".to_string(),
                url: "https://example.com/articles/retry".to_string(),
                published_at: Some(Utc::now()),
                fetched_at: Utc::now(),
            })
            .await
            .unwrap();

        let worker = LlmWorker::with_provider(
            state.clone(),
            Arc::new(FakeBatchProvider {
                submit_results: Mutex::new(VecDeque::from([
                    Ok("operations/batch-1".to_string()),
                    Ok("operations/batch-2".to_string()),
                ])),
                poll_results: Mutex::new(VecDeque::from([
                    Ok(BatchPollResult::Failed("quota exceeded".to_string())),
                    Ok(BatchPollResult::Failed("quota exceeded again".to_string())),
                ])),
            }),
        );

        assert_eq!(worker.run_once().await.unwrap(), 1);
        assert_eq!(worker.run_once().await.unwrap(), 1);
        let first_failure = state.get_article(article.id).await.unwrap();
        assert_eq!(
            first_failure.llm_status,
            crate::domain::ProcessingStatus::Pending
        );
        assert_eq!(first_failure.llm_error.as_deref(), Some("quota exceeded"));

        assert_eq!(worker.run_once().await.unwrap(), 1);
        assert_eq!(worker.run_once().await.unwrap(), 1);
        let terminal_failure = state.get_article(article.id).await.unwrap();
        assert_eq!(
            terminal_failure.llm_status,
            crate::domain::ProcessingStatus::Failed
        );
        assert_eq!(
            terminal_failure.llm_error.as_deref(),
            Some("quota exceeded again")
        );
    }

    #[test]
    fn parses_gemini_batch_create_response_without_done_field() {
        let payload = r#"{
          "name": "batches/example-batch",
          "metadata": {
            "state": "BATCH_STATE_PENDING"
          }
        }"#;

        let parsed: GeminiBatchCreateResponse = serde_json::from_str(payload).unwrap();
        assert_eq!(parsed.name, "batches/example-batch");
    }

    #[test]
    fn treats_terminal_metadata_state_as_done() {
        let payload = r#"{
          "name": "batches/example-batch",
          "metadata": {
            "state": "JOB_STATE_SUCCEEDED"
          },
          "response": {
            "inlinedResponses": {
              "inlinedResponses": []
            }
          }
        }"#;

        let parsed: GeminiOperation = serde_json::from_str(payload).unwrap();
        assert!(parsed.is_done());
    }

    async fn test_state(retry_limit: u32) -> (TempDir, AppState) {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = AppConfig {
            config_path: temp_dir.path().join("config.toml"),
            database_path: temp_dir.path().join("llm-test.db"),
            llm: LlmConfig {
                api_key: Some("test-key".to_string()),
                batch_poll_interval_seconds: 1,
                batch_submit_size: 8,
                retry_limit,
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
}
