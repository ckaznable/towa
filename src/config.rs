use std::{collections::BTreeSet, env, fs, path::PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::domain::AgentSummary;

const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-flash";
const DEFAULT_BATCH_POLL_INTERVAL_SECONDS: u64 = 30;
const DEFAULT_BATCH_SUBMIT_SIZE: usize = 16;
const DEFAULT_RETRY_LIMIT: u32 = 3;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub config_path: PathBuf,
    pub database_path: PathBuf,
    pub llm: LlmConfig,
}

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub api_key: Option<String>,
    pub batch_poll_interval_seconds: u64,
    pub batch_submit_size: usize,
    pub retry_limit: u32,
    pub agents: Vec<LlmAgentConfig>,
}

#[derive(Debug, Clone)]
pub struct LlmAgentConfig {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub batch_enabled: bool,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to resolve XDG config directory")]
    MissingConfigDir,
    #[error("failed to resolve XDG data directory")]
    MissingDataDir,
    #[error("failed to read config file `{path}`: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config file `{path}`: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("config validation failed: {0}")]
    Validation(String),
}

#[derive(Debug, Deserialize, Default)]
struct RawConfig {
    llm: Option<RawLlmConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct RawLlmConfig {
    api_key: Option<String>,
    #[serde(default = "default_batch_poll_interval_seconds")]
    batch_poll_interval_seconds: u64,
    #[serde(default = "default_batch_submit_size")]
    batch_submit_size: usize,
    #[serde(default = "default_retry_limit")]
    retry_limit: u32,
    agents: Vec<RawAgentConfig>,
}

#[derive(Debug, Deserialize)]
struct RawAgentConfig {
    id: String,
    label: String,
    provider: String,
    model: Option<String>,
    system_prompt: Option<String>,
    #[serde(default = "default_batch_enabled")]
    batch_enabled: bool,
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let config_root = dirs::config_dir().ok_or(ConfigError::MissingConfigDir)?;
        let data_root = dirs::data_local_dir().ok_or(ConfigError::MissingDataDir)?;
        let config_path = env::var_os("TOWA_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| config_root.join("towa").join("config.toml"));
        let database_path = env::var_os("TOWA_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_root.join("towa").join("towa.db"));

        let raw = if config_path.exists() {
            let content = fs::read_to_string(&config_path).map_err(|source| ConfigError::Read {
                path: config_path.clone(),
                source,
            })?;
            toml::from_str::<RawConfig>(&content).map_err(|source| ConfigError::Parse {
                path: config_path.clone(),
                source,
            })?
        } else {
            RawConfig::default()
        };

        let llm = parse_llm(raw)?;

        Ok(Self {
            config_path,
            database_path,
            llm,
        })
    }

    pub fn agent_summaries(&self) -> Vec<AgentSummary> {
        self.llm
            .agents
            .iter()
            .map(LlmAgentConfig::summary)
            .collect()
    }
}

impl LlmAgentConfig {
    pub fn summary(&self) -> AgentSummary {
        AgentSummary {
            id: self.id.clone(),
            label: self.label.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            batch_enabled: self.batch_enabled,
        }
    }
}

fn parse_llm(raw: RawConfig) -> Result<LlmConfig, ConfigError> {
    let mut llm = raw.llm.unwrap_or_default();
    let agents = parse_agents(std::mem::take(&mut llm.agents))?;

    if llm.batch_poll_interval_seconds == 0 {
        return Err(ConfigError::Validation(
            "llm.batch_poll_interval_seconds must be greater than 0".to_string(),
        ));
    }
    if llm.batch_submit_size == 0 {
        return Err(ConfigError::Validation(
            "llm.batch_submit_size must be greater than 0".to_string(),
        ));
    }
    if llm.retry_limit == 0 {
        return Err(ConfigError::Validation(
            "llm.retry_limit must be greater than 0".to_string(),
        ));
    }

    let api_key = env::var("GEMINI_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or(llm.api_key.filter(|value| !value.trim().is_empty()));

    Ok(LlmConfig {
        api_key,
        batch_poll_interval_seconds: llm.batch_poll_interval_seconds,
        batch_submit_size: llm.batch_submit_size,
        retry_limit: llm.retry_limit,
        agents,
    })
}

fn parse_agents(raw_agents: Vec<RawAgentConfig>) -> Result<Vec<LlmAgentConfig>, ConfigError> {
    let candidates = if raw_agents.is_empty() {
        default_agents()
    } else {
        raw_agents
            .into_iter()
            .map(|agent| LlmAgentConfig {
                id: agent.id,
                label: agent.label,
                provider: agent.provider,
                model: agent
                    .model
                    .unwrap_or_else(|| DEFAULT_GEMINI_MODEL.to_string()),
                system_prompt: agent.system_prompt,
                batch_enabled: agent.batch_enabled,
            })
            .collect()
    };

    let mut seen_ids = BTreeSet::new();
    let mut agents = Vec::with_capacity(candidates.len());
    for agent in candidates {
        if agent.id.trim().is_empty() {
            return Err(ConfigError::Validation(
                "agent id must not be empty".to_string(),
            ));
        }
        if agent.label.trim().is_empty() {
            return Err(ConfigError::Validation(format!(
                "agent `{}` label must not be empty",
                agent.id
            )));
        }
        if !seen_ids.insert(agent.id.clone()) {
            return Err(ConfigError::Validation(format!(
                "duplicate agent id `{}`",
                agent.id
            )));
        }
        if agent.provider != "gemini" {
            return Err(ConfigError::Validation(format!(
                "agent `{}` uses unsupported provider `{}`",
                agent.id, agent.provider
            )));
        }
        if agent.model.trim().is_empty() {
            return Err(ConfigError::Validation(format!(
                "agent `{}` model must not be empty",
                agent.id
            )));
        }
        agents.push(agent);
    }

    Ok(agents)
}

fn default_agents() -> Vec<LlmAgentConfig> {
    vec![
        LlmAgentConfig {
            id: "gemini-brief".to_string(),
            label: "Gemini Brief".to_string(),
            provider: "gemini".to_string(),
            model: DEFAULT_GEMINI_MODEL.to_string(),
            system_prompt: Some(
                "Write a concise 3-5 sentence summary for a reader and preserve proper nouns."
                    .to_string(),
            ),
            batch_enabled: true,
        },
        LlmAgentConfig {
            id: "gemini-deep-tech".to_string(),
            label: "Gemini Deep Tech".to_string(),
            provider: "gemini".to_string(),
            model: DEFAULT_GEMINI_MODEL.to_string(),
            system_prompt: Some(
                "Explain the technical points, risks, and practical next actions for the article."
                    .to_string(),
            ),
            batch_enabled: true,
        },
    ]
}

fn default_batch_enabled() -> bool {
    true
}

fn default_batch_poll_interval_seconds() -> u64 {
    DEFAULT_BATCH_POLL_INTERVAL_SECONDS
}

fn default_batch_submit_size() -> usize {
    DEFAULT_BATCH_SUBMIT_SIZE
}

fn default_retry_limit() -> u32 {
    DEFAULT_RETRY_LIMIT
}
