mod app;
mod config;
mod db;
mod domain;
mod llm;
mod scheduler;
mod state;

use std::{env, net::SocketAddr};

use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{app::build_router, llm::LlmWorker, scheduler::Scheduler, state::AppState};

const SCHEDULER_TICK_SECONDS: u64 = 30;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);
    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let address = format!("{host}:{port}")
        .parse::<SocketAddr>()
        .map_err(|error| format!("invalid bind address `{host}:{port}`: {error}"))?;

    let state = AppState::new().await?;
    let config_path = state.config_path().display().to_string();
    let database_path = state.database_path().display().to_string();
    let llm_runtime_config = state.llm_config();
    let scheduler = Scheduler::new(state.clone());
    let llm_worker = LlmWorker::new(state.clone());
    let router = build_router(state);
    let listener = TcpListener::bind(address).await?;

    tracing::info!(
        config_path = %config_path,
        database_path = %database_path,
        "application configuration loaded"
    );
    tracing::info!("listening on http://{address}");
    tracing::info!(
        scheduler_tick_seconds = SCHEDULER_TICK_SECONDS,
        "scheduler worker starting"
    );

    tokio::spawn(async move {
        if let Err(error) = scheduler.run_loop().await {
            tracing::error!("scheduler stopped: {error}");
        }
    });

    if let Some(llm_worker) = llm_worker {
        tracing::info!(
            batch_submit_size = llm_runtime_config.batch_submit_size,
            poll_interval_seconds = llm_runtime_config.batch_poll_interval_seconds,
            retry_limit = llm_runtime_config.retry_limit,
            agent_count = llm_runtime_config.agents.len(),
            "llm worker starting"
        );
        tokio::spawn(async move {
            if let Err(error) = llm_worker.run_loop().await {
                tracing::error!("llm worker stopped: {error}");
            }
        });
    } else {
        tracing::warn!("GEMINI_API_KEY not configured; llm worker is disabled");
    }

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn init_tracing() {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,towa=debug"));
    let log_format = env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());

    let registry = tracing_subscriber::registry().with(env_filter);
    if log_format.eq_ignore_ascii_case("json") {
        registry
            .with(fmt::layer().json().flatten_event(true))
            .init();
    } else {
        registry.with(fmt::layer()).init();
    }
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!("failed to install ctrl+c signal handler: {error}");
    }
}
