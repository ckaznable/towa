use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Html,
    routing::{get, post, put},
};
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use uuid::Uuid;

use crate::{
    domain::{
        AdminProcessingOverview, AgentListResponse, ArticleListResponse, ArticleQuery,
        AssignAgentRequest, CreateSourceRequest, FavoriteRequest, HealthResponse, ReadStateRequest,
        RetryBatchRequest, RetryResult, SourceListResponse, UpdateSourceRequest,
    },
    state::{ApiError, AppState},
};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/agents", get(list_agents))
        .route("/api/sources", get(list_sources).post(create_source))
        .route(
            "/api/sources/{id}",
            get(get_source).patch(update_source).delete(delete_source),
        )
        .route("/api/sources/{id}/agent", put(assign_agent))
        .route("/api/articles", get(list_articles))
        .route("/api/articles/{id}", get(get_article))
        .route("/api/articles/{id}/read", put(set_read_state))
        .route("/api/articles/{id}/favorite", put(set_favorite))
        .route("/api/articles/{id}/bookmark", put(set_bookmark))
        .route("/api/favorites", get(list_favorites))
        .route("/api/bookmarks", get(list_bookmarks))
        .route("/api/admin/processing", get(admin_processing))
        .route(
            "/api/admin/articles/{id}/retry",
            post(retry_article_processing),
        )
        .route("/api/admin/batches/retry", post(retry_batch_processing))
        .nest_service("/assets", ServeDir::new("web/dist/assets"))
        .route_service("/favicon.svg", ServeFile::new("web/dist/favicon.svg"))
        .route_service("/icons.svg", ServeFile::new("web/dist/icons.svg"))
        .fallback(get(frontend_index))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "towa-api",
    })
}

async fn list_agents(State(state): State<AppState>) -> Json<AgentListResponse> {
    Json(AgentListResponse {
        items: state.list_agents(),
    })
}

async fn list_sources(State(state): State<AppState>) -> Result<Json<SourceListResponse>, ApiError> {
    Ok(Json(SourceListResponse {
        items: state.list_sources().await?,
    }))
}

async fn create_source(
    State(state): State<AppState>,
    Json(request): Json<CreateSourceRequest>,
) -> Result<(StatusCode, Json<crate::domain::Source>), ApiError> {
    let source = state.create_source(request).await?;
    Ok((StatusCode::CREATED, Json(source)))
}

async fn get_source(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::domain::Source>, ApiError> {
    Ok(Json(state.get_source(id).await?))
}

async fn update_source(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateSourceRequest>,
) -> Result<Json<crate::domain::Source>, ApiError> {
    Ok(Json(state.update_source(id, request).await?))
}

async fn delete_source(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.delete_source(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn assign_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<AssignAgentRequest>,
) -> Result<Json<crate::domain::Source>, ApiError> {
    Ok(Json(state.assign_agent(id, request).await?))
}

async fn list_articles(
    State(state): State<AppState>,
    Query(query): Query<ArticleQuery>,
) -> Result<Json<ArticleListResponse>, ApiError> {
    Ok(Json(ArticleListResponse {
        items: state.list_articles(query).await?,
    }))
}

async fn get_article(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::domain::ArticleDetail>, ApiError> {
    Ok(Json(state.get_article(id).await?))
}

async fn set_bookmark(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<FavoriteRequest>,
) -> Result<Json<crate::domain::ArticleDetail>, ApiError> {
    let favorited = request.favorite_state().ok_or_else(|| {
        ApiError::Validation("favorite request must include `favorited`".to_string())
    })?;
    Ok(Json(state.set_bookmark(id, favorited).await?))
}

async fn set_favorite(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<FavoriteRequest>,
) -> Result<Json<crate::domain::ArticleDetail>, ApiError> {
    set_favorite_inner(state, id, request).await.map(Json)
}

async fn set_read_state(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<ReadStateRequest>,
) -> Result<Json<crate::domain::ArticleDetail>, ApiError> {
    Ok(Json(state.set_read_state(id, request.read).await?))
}

async fn list_bookmarks(
    State(state): State<AppState>,
) -> Result<Json<ArticleListResponse>, ApiError> {
    list_favorites(State(state)).await
}

async fn list_favorites(
    State(state): State<AppState>,
) -> Result<Json<ArticleListResponse>, ApiError> {
    Ok(Json(ArticleListResponse {
        items: state.list_favorites().await?,
    }))
}

async fn admin_processing(
    State(state): State<AppState>,
) -> Result<Json<AdminProcessingOverview>, ApiError> {
    Ok(Json(state.admin_processing_overview().await?))
}

async fn retry_article_processing(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RetryResult>, ApiError> {
    Ok(Json(RetryResult {
        retried: state.retry_article_processing(id).await?,
    }))
}

async fn retry_batch_processing(
    State(state): State<AppState>,
    Json(request): Json<RetryBatchRequest>,
) -> Result<Json<RetryResult>, ApiError> {
    Ok(Json(RetryResult {
        retried: state.retry_batch_processing(&request.batch_name).await?,
    }))
}

async fn frontend_index() -> Result<Html<String>, StatusCode> {
    let html = tokio::fs::read_to_string("web/dist/index.html")
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Html(html))
}

async fn set_favorite_inner(
    state: AppState,
    id: Uuid,
    request: FavoriteRequest,
) -> Result<crate::domain::ArticleDetail, ApiError> {
    let favorited = request.favorite_state().ok_or_else(|| {
        ApiError::Validation("favorite request must include `favorited`".to_string())
    })?;
    state.set_favorite(id, favorited).await
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Arc};

    use async_trait::async_trait;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use tempfile::TempDir;
    use tokio::sync::Mutex;
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::{
        config::{AppConfig, LlmAgentConfig, LlmConfig},
        db::FetchedArticleInput,
        domain::{
            AdminProcessingOverview, Article, ArticleDetail, ArticleListResponse,
            CreateSourceRequest, FeedKind, ProcessingStatus, RetryResult, Source,
            SourceListResponse,
        },
        llm::{BatchPollResult, BatchProvider, LlmWorker, LlmWorkerError},
        scheduler::{
            FeedFetcher, FetchRequest, FetchResponse, FetchStatus, Scheduler, SchedulerError,
        },
        state::{AppState, FeedValidator, ValidatedFeed, ValidationError},
    };

    use super::build_router;

    struct StubFeedValidator;
    struct FakeFetcher {
        responses: Mutex<VecDeque<Result<FetchResponse, SchedulerError>>>,
    }
    struct FakeBatchProvider {
        submit_results: Mutex<VecDeque<Result<String, LlmWorkerError>>>,
        poll_results: Mutex<VecDeque<Result<BatchPollResult, LlmWorkerError>>>,
    }

    #[async_trait]
    impl FeedValidator for StubFeedValidator {
        async fn validate(&self, feed_url: &str) -> Result<ValidatedFeed, ValidationError> {
            if !feed_url.contains("example.com") && !feed_url.contains("github.com") {
                return Err(ValidationError::UnsupportedFormat);
            }

            if feed_url.contains("github.com") {
                return Ok(ValidatedFeed {
                    title: "GitHub Releases".to_string(),
                    feed_kind: FeedKind::Atom,
                });
            }

            Ok(ValidatedFeed {
                title: "Example Feed".to_string(),
                feed_kind: FeedKind::Rss,
            })
        }
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

    #[async_trait]
    impl BatchProvider for FakeBatchProvider {
        async fn submit_batch(
            &self,
            _agent: &LlmAgentConfig,
            _jobs: &[crate::db::PendingProcessingJob],
        ) -> Result<String, LlmWorkerError> {
            self.submit_results
                .lock()
                .await
                .pop_front()
                .expect("fake batch provider should have a submit result")
        }

        async fn poll_batch(&self, _batch_name: &str) -> Result<BatchPollResult, LlmWorkerError> {
            self.poll_results
                .lock()
                .await
                .pop_front()
                .expect("fake batch provider should have a poll result")
        }
    }

    #[tokio::test]
    async fn creates_and_lists_sources() {
        let (_temp_dir, state) = test_state().await;
        let app = build_router(state);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sources")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"feed_url":"https://example.com/feed.xml","assigned_agent_id":"gemini-brief"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let list_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/sources")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(list_response.status(), StatusCode::OK);
        let bytes = to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: SourceListResponse = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(payload.items.len(), 1);
        assert_eq!(payload.items[0].feed_kind, FeedKind::Rss);
    }

    #[tokio::test]
    async fn normalizes_github_repo_url_to_releases_atom() {
        let (_temp_dir, state) = test_state().await;
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sources")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"feed_url":"https://github.com/octocat/Hello-World"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Source = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            payload.feed_url,
            "https://github.com/octocat/Hello-World/releases.atom"
        );
        assert_eq!(payload.feed_kind, FeedKind::Atom);
    }

    #[tokio::test]
    async fn assigns_agent_to_source() {
        let (_temp_dir, state) = test_state().await;
        let source = state
            .create_source(CreateSourceRequest {
                title: Some("Rust Blog".to_string()),
                feed_url: "https://example.com/feed.xml".to_string(),
                enabled: Some(true),
                assigned_agent_id: None,
            })
            .await
            .unwrap();
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/sources/{}/agent", source.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"assigned_agent_id":"gemini-deep-tech"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Source = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            payload.assigned_agent_id.as_deref(),
            Some("gemini-deep-tech")
        );
    }

    #[tokio::test]
    async fn toggles_favorites_and_lists_favorites_collection() {
        let (_temp_dir, state) = test_state().await;
        let source = state
            .create_source(CreateSourceRequest {
                title: Some("Rust Blog".to_string()),
                feed_url: "https://example.com/feed.xml".to_string(),
                enabled: Some(true),
                assigned_agent_id: None,
            })
            .await
            .unwrap();
        let article_id = Uuid::new_v4();
        state
            .insert_article(Article {
                id: article_id,
                source_id: source.id,
                title: "Tokio 2 planning notes".to_string(),
                summary: "A preview of async runtime changes.".to_string(),
                content: "A preview of async runtime changes.".to_string(),
                url: "https://example.com/articles/tokio-2".to_string(),
                published_at: Some(Utc::now()),
                fetched_at: Utc::now(),
                read_at: None,
                ignored: false,
                bookmarked: false,
                llm_status: ProcessingStatus::Pending,
                llm_title: None,
                llm_summary: None,
                llm_error: None,
            })
            .await
            .unwrap();
        let app = build_router(state);

        let favorite_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/articles/{article_id}/favorite"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"favorited":true}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(favorite_response.status(), StatusCode::OK);
        let favorite_bytes = to_bytes(favorite_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let favorited_article: ArticleDetail = serde_json::from_slice(&favorite_bytes).unwrap();
        assert!(favorited_article.favorited);
        assert!(favorited_article.bookmarked);

        let favorites_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/favorites")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(favorites_response.status(), StatusCode::OK);
        let favorites_bytes = to_bytes(favorites_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let favorites: ArticleListResponse = serde_json::from_slice(&favorites_bytes).unwrap();
        assert_eq!(favorites.items.len(), 1);
        assert_eq!(favorites.items[0].id, article_id);
        assert!(favorites.items[0].favorited);
    }

    #[tokio::test]
    async fn source_fetch_to_llm_flow_is_visible_via_api() {
        let (_temp_dir, state) = test_state().await;
        let app = build_router(state.clone());

        let source_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sources")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"title":"AI Feed","feed_url":"https://example.com/ai.xml","assigned_agent_id":"gemini-brief"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(source_response.status(), StatusCode::CREATED);
        let source_bytes = to_bytes(source_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let source: Source = serde_json::from_slice(&source_bytes).unwrap();

        let scheduler = Scheduler::with_fetcher(
            state.clone(),
            Arc::new(FakeFetcher {
                responses: Mutex::new(VecDeque::from([Ok(FetchResponse {
                    status: FetchStatus::Modified,
                    body: Some(sample_rss().into_bytes()),
                    etag: Some("\"v1\"".to_string()),
                    last_modified: Some("Wed, 01 Apr 2026 09:00:00 GMT".to_string()),
                    cache_control: Some("max-age=60".to_string()),
                    expires: None,
                })])),
            }),
        );
        assert_eq!(scheduler.run_once().await.unwrap(), 1);

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/articles?source_id={}", source.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_bytes = to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let list_payload: ArticleListResponse = serde_json::from_slice(&list_bytes).unwrap();
        assert_eq!(list_payload.items.len(), 1);
        assert_eq!(list_payload.items[0].llm_status, ProcessingStatus::Pending);

        let worker = LlmWorker::with_provider(
            state.clone(),
            Arc::new(FakeBatchProvider {
                submit_results: Mutex::new(VecDeque::from([Ok(
                    "operations/front-visible".to_string()
                )])),
                poll_results: Mutex::new(VecDeque::from([Ok(BatchPollResult::Completed(vec![
                    crate::db::BatchArticleOutput {
                        article_id: list_payload.items[0].id,
                        title: None,
                        summary: Some("LLM summary visible through the API.".to_string()),
                        error: None,
                    },
                ]))])),
            }),
        );
        assert_eq!(worker.run_once().await.unwrap(), 1);
        assert_eq!(worker.run_once().await.unwrap(), 1);

        let detail_response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/articles/{}", list_payload.items[0].id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail_response.status(), StatusCode::OK);
        let detail_bytes = to_bytes(detail_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let detail_payload: ArticleDetail = serde_json::from_slice(&detail_bytes).unwrap();
        assert_eq!(detail_payload.llm_status, ProcessingStatus::Done);
        assert_eq!(
            detail_payload.llm_summary.as_deref(),
            Some("LLM summary visible through the API."),
        );
        assert!(detail_payload.available_at >= detail_payload.fetched_at);
    }

    #[tokio::test]
    async fn retention_worker_keeps_bookmarked_articles_visible_via_api() {
        let (_temp_dir, state) = test_state().await;
        let app = build_router(state.clone());
        let source = state
            .create_source(CreateSourceRequest {
                title: Some("Retention Flow".to_string()),
                feed_url: "https://example.com/retention-flow.xml".to_string(),
                enabled: Some(true),
                assigned_agent_id: None,
            })
            .await
            .unwrap();
        let now = Utc::now();
        state
            .apply_source_fetch_update(
                source.id,
                crate::db::SourceFetchUpdate {
                    last_fetch_at: Some(now),
                    next_fetch_at: now + chrono::Duration::hours(12),
                    etag: None,
                    last_modified: None,
                    validation_status: "validated".to_string(),
                },
            )
            .await
            .unwrap();

        let old_kept_id = Uuid::new_v4();
        let old_removed_id = Uuid::new_v4();

        state
            .insert_article(Article {
                id: old_kept_id,
                source_id: source.id,
                title: "Saved article".to_string(),
                summary: "Should survive cleanup".to_string(),
                content: "Should survive cleanup".to_string(),
                url: "https://example.com/articles/kept".to_string(),
                published_at: Some(now - chrono::Duration::days(40)),
                fetched_at: now - chrono::Duration::days(40),
                read_at: None,
                ignored: false,
                bookmarked: false,
                llm_status: ProcessingStatus::Done,
                llm_title: None,
                llm_summary: Some("Already summarized".to_string()),
                llm_error: None,
            })
            .await
            .unwrap();
        state
            .insert_article(Article {
                id: old_removed_id,
                source_id: source.id,
                title: "Expired article".to_string(),
                summary: "Should be deleted".to_string(),
                content: "Should be deleted".to_string(),
                url: "https://example.com/articles/removed".to_string(),
                published_at: Some(now - chrono::Duration::days(40)),
                fetched_at: now - chrono::Duration::days(40),
                read_at: None,
                ignored: false,
                bookmarked: false,
                llm_status: ProcessingStatus::Done,
                llm_title: None,
                llm_summary: Some("Already summarized".to_string()),
                llm_error: None,
            })
            .await
            .unwrap();

        let bookmark_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/articles/{old_kept_id}/bookmark"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"bookmarked":true}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bookmark_response.status(), StatusCode::OK);

        let scheduler = Scheduler::with_fetcher(
            state.clone(),
            Arc::new(FakeFetcher {
                responses: Mutex::new(VecDeque::new()),
            }),
        );
        assert_eq!(scheduler.run_once().await.unwrap(), 0);

        let list_response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/articles?source_id={}", source.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_bytes = to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let list_payload: ArticleListResponse = serde_json::from_slice(&list_bytes).unwrap();
        assert_eq!(list_payload.items.len(), 1);
        assert_eq!(list_payload.items[0].id, old_kept_id);
        assert!(list_payload.items[0].bookmarked);
    }

    #[tokio::test]
    async fn unfavorited_article_returns_to_normal_retention_rules() {
        let (_temp_dir, state) = test_state().await;
        let app = build_router(state.clone());
        let source = state
            .create_source(CreateSourceRequest {
                title: Some("Favorites Retention".to_string()),
                feed_url: "https://example.com/favorites-retention.xml".to_string(),
                enabled: Some(true),
                assigned_agent_id: None,
            })
            .await
            .unwrap();
        let old_article_id = Uuid::new_v4();
        let now = Utc::now();
        state
            .apply_source_fetch_update(
                source.id,
                crate::db::SourceFetchUpdate {
                    last_fetch_at: Some(now),
                    next_fetch_at: now + chrono::Duration::hours(12),
                    etag: None,
                    last_modified: None,
                    validation_status: "validated".to_string(),
                },
            )
            .await
            .unwrap();

        state
            .insert_article(Article {
                id: old_article_id,
                source_id: source.id,
                title: "Old favorite".to_string(),
                summary: "Saved, then removed".to_string(),
                content: "Saved, then removed".to_string(),
                url: "https://example.com/articles/old-favorite".to_string(),
                published_at: Some(now - chrono::Duration::days(40)),
                fetched_at: now - chrono::Duration::days(40),
                read_at: None,
                ignored: false,
                bookmarked: true,
                llm_status: ProcessingStatus::Done,
                llm_title: None,
                llm_summary: Some("Still summarized".to_string()),
                llm_error: None,
            })
            .await
            .unwrap();

        let unfavorite_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/articles/{old_article_id}/favorite"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"favorited":false}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unfavorite_response.status(), StatusCode::OK);

        let scheduler = Scheduler::with_fetcher(
            state.clone(),
            Arc::new(FakeFetcher {
                responses: Mutex::new(VecDeque::new()),
            }),
        );
        assert_eq!(scheduler.run_once().await.unwrap(), 0);

        let detail_response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/articles/{old_article_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail_response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn favorite_operations_preserve_llm_state_and_output() {
        let (_temp_dir, state) = test_state().await;
        let app = build_router(state.clone());
        let source = state
            .create_source(CreateSourceRequest {
                title: Some("LLM Integrity".to_string()),
                feed_url: "https://example.com/llm-integrity.xml".to_string(),
                enabled: Some(true),
                assigned_agent_id: None,
            })
            .await
            .unwrap();
        let article_id = Uuid::new_v4();
        state
            .insert_article(Article {
                id: article_id,
                source_id: source.id,
                title: "Preserve state".to_string(),
                summary: "Favorite should not reset analysis.".to_string(),
                content: "Favorite should not reset analysis.".to_string(),
                url: "https://example.com/articles/preserve-state".to_string(),
                published_at: Some(Utc::now()),
                fetched_at: Utc::now(),
                read_at: None,
                ignored: false,
                bookmarked: false,
                llm_status: ProcessingStatus::Done,
                llm_title: None,
                llm_summary: Some("Existing summary".to_string()),
                llm_error: Some("historic error".to_string()),
            })
            .await
            .unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/articles/{article_id}/favorite"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"favorited":true}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: ArticleDetail = serde_json::from_slice(&bytes).unwrap();
        assert!(payload.favorited);
        assert_eq!(payload.llm_status, ProcessingStatus::Done);
        assert_eq!(payload.llm_summary.as_deref(), Some("Existing summary"));
        assert_eq!(payload.llm_error.as_deref(), Some("historic error"));
    }

    #[tokio::test]
    async fn read_state_is_persisted_via_api() {
        let (_temp_dir, state) = test_state().await;
        let source = state
            .create_source(CreateSourceRequest {
                title: Some("Read Feed".to_string()),
                feed_url: "https://example.com/read.xml".to_string(),
                enabled: Some(true),
                assigned_agent_id: None,
            })
            .await
            .unwrap();
        let article_id = Uuid::new_v4();
        state
            .insert_article(Article {
                id: article_id,
                source_id: source.id,
                title: "Unread item".to_string(),
                summary: "Will become read".to_string(),
                content: "Will become read".to_string(),
                url: "https://example.com/articles/read".to_string(),
                published_at: Some(Utc::now()),
                fetched_at: Utc::now(),
                read_at: None,
                ignored: false,
                bookmarked: false,
                llm_status: ProcessingStatus::Done,
                llm_title: None,
                llm_summary: Some("Summarized".to_string()),
                llm_error: None,
            })
            .await
            .unwrap();

        let app = build_router(state.clone());
        let mark_read_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/articles/{article_id}/read"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"read":true}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(mark_read_response.status(), StatusCode::OK);
        let bytes = to_bytes(mark_read_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: ArticleDetail = serde_json::from_slice(&bytes).unwrap();
        assert!(payload.read);
        assert!(payload.read_at.is_some());

        let list_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/articles")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let list_bytes = to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let list_payload: ArticleListResponse = serde_json::from_slice(&list_bytes).unwrap();
        assert!(list_payload.items[0].read);
        assert!(list_payload.items[0].read_at.is_some());
    }

    #[tokio::test]
    async fn admin_processing_api_lists_and_retries_jobs() {
        let (_temp_dir, state) = test_state().await;
        let source = state
            .create_source(CreateSourceRequest {
                title: Some("Ops Feed".to_string()),
                feed_url: "https://example.com/ops.xml".to_string(),
                enabled: Some(true),
                assigned_agent_id: Some("gemini-brief".to_string()),
            })
            .await
            .unwrap();

        let article = state
            .upsert_fetched_article(FetchedArticleInput {
                source_id: source.id,
                dedupe_key: "ops-1".to_string(),
                title: "Batch retry candidate".to_string(),
                summary: "Needs a retry".to_string(),
                content: "Needs a retry".to_string(),
                url: "https://example.com/ops/1".to_string(),
                published_at: Some(Utc::now()),
                fetched_at: Utc::now(),
                ignored: false,
            })
            .await
            .unwrap();
        state
            .mark_batch_started(&[article.id], "operations/test-batch")
            .await
            .unwrap();
        state
            .fail_batch("operations/test-batch", "upstream provider failure", 1)
            .await
            .unwrap();

        let app = build_router(state.clone());

        let overview_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/admin/processing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(overview_response.status(), StatusCode::OK);
        let overview_bytes = to_bytes(overview_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let overview: AdminProcessingOverview = serde_json::from_slice(&overview_bytes).unwrap();
        assert_eq!(overview.failed_jobs.len(), 1);
        assert_eq!(
            overview.failed_jobs[0].last_batch_name.as_deref(),
            Some("operations/test-batch")
        );

        let retry_batch_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/admin/batches/retry")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"batch_name":"operations/test-batch"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(retry_batch_response.status(), StatusCode::OK);
        let retry_batch_bytes = to_bytes(retry_batch_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let retry_batch: RetryResult = serde_json::from_slice(&retry_batch_bytes).unwrap();
        assert_eq!(retry_batch.retried, 1);

        state
            .mark_batch_started(&[article.id], "operations/test-batch")
            .await
            .unwrap();
        state
            .fail_batch("operations/test-batch", "upstream provider failure", 1)
            .await
            .unwrap();

        let retry_article_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/admin/articles/{}/retry", article.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(retry_article_response.status(), StatusCode::OK);
        let retry_article_bytes = to_bytes(retry_article_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let retry_article: RetryResult = serde_json::from_slice(&retry_article_bytes).unwrap();
        assert_eq!(retry_article.retried, 1);

        let overview_after_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/admin/processing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let overview_after_bytes = to_bytes(overview_after_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let overview_after: AdminProcessingOverview =
            serde_json::from_slice(&overview_after_bytes).unwrap();
        assert_eq!(overview_after.failed_jobs.len(), 0);
        assert_eq!(overview_after.pending_jobs.len(), 1);
        assert_eq!(overview_after.pending_jobs[0].article_id, article.id);
    }

    #[tokio::test]
    async fn frontend_routes_fallback_to_index_html() {
        let (_temp_dir, state) = test_state().await;
        let app = build_router(state);

        let root_response = app
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(root_response.status(), StatusCode::OK);

        let nested_response = app
            .oneshot(
                Request::builder()
                    .uri("/reader/bookmarks")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(nested_response.status(), StatusCode::OK);
    }

    async fn test_state() -> (TempDir, AppState) {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = AppConfig {
            config_path: temp_dir.path().join("config.toml"),
            database_path: temp_dir.path().join("towa-test.db"),
            llm: LlmConfig {
                api_key: Some("test-key".to_string()),
                batch_poll_interval_seconds: 1,
                batch_submit_size: 8,
                retry_limit: 3,
                agents: vec![
                    LlmAgentConfig {
                        id: "gemini-brief".to_string(),
                        label: "Gemini Brief".to_string(),
                        provider: "gemini".to_string(),
                        model: "gemini-2.5-flash".to_string(),
                        system_prompt: Some("Summarize the article.".to_string()),
                        batch_enabled: true,
                    },
                    LlmAgentConfig {
                        id: "gemini-deep-tech".to_string(),
                        label: "Gemini Deep Tech".to_string(),
                        provider: "gemini".to_string(),
                        model: "gemini-2.5-flash".to_string(),
                        system_prompt: Some("Explain the technical details.".to_string()),
                        batch_enabled: true,
                    },
                ],
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
      <title>Front visible article</title>
      <link>https://example.com/articles/1</link>
      <description>Hello world</description>
      <pubDate>Wed, 01 Apr 2026 09:00:00 GMT</pubDate>
    </item>
  </channel>
</rss>"#
            .to_string()
    }
}
