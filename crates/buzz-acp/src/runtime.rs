//! Shared agent capabilities. Intake and scheduling belong to each entry point;
//! environment, credentials, adapter settings and prompt equipment belong here.
use crate::{
    build_mcp_servers, config::Config, current_working_directory, git, observer, pool, relay,
    resolve_agent_owner, PromptContext,
};
use anyhow::Result;
use std::{collections::HashMap, time::Duration};
use uuid::Uuid;

pub(crate) enum SessionMode {
    Conversation,
    Task,
}

/// Own shared launch resources until all adapter work has been drained.
/// Add new runtime capabilities here, not separately to the CLI entry points.
pub(crate) struct AgentRuntime {
    config: Config,
    _git_environment: git::GitEnvironment,
}

impl AgentRuntime {
    pub(crate) fn prepare(mut config: Config) -> Result<Self> {
        let git_environment = git::GitEnvironment::for_config(&mut config)?;
        Ok(Self {
            config,
            _git_environment: git_environment,
        })
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) fn startup(&self, observer: Option<observer::ObserverHandle>) -> PoolStartup {
        PoolStartup::from_config(&self.config, observer)
    }

    pub(crate) fn prompt_context(
        &self,
        rest: relay::RestClient,
        channels: HashMap<Uuid, relay::ChannelInfo>,
        mode: SessionMode,
    ) -> Result<PromptContext> {
        make_prompt_context(&self.config, rest, channels, mode)
    }
}

#[derive(Clone)]
pub(crate) struct PoolStartup {
    pub(crate) agents: u32,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) extra_env: Vec<(String, String)>,
    /// Per-worker OpenCode data-isolation base dir. `None` for non-OpenCode
    /// agents. Worker leaf dirs (`<base>/w<i>`) are carved at spawn time.
    pub(crate) opencode_data_base: Option<std::path::PathBuf>,
    pub(crate) has_generated_codex_config: bool,
    pub(crate) model: Option<String>,
    pub(crate) effort_level: Option<String>,
    pub(crate) observer: Option<observer::ObserverHandle>,
}

impl PoolStartup {
    fn from_config(config: &Config, observer: Option<observer::ObserverHandle>) -> Self {
        // OpenCode keeps every instance's state (sessions, sqlite db, WAL)
        // under `$XDG_DATA_HOME/opencode`. Without isolation all managed
        // agents on the host contend on one sqlite database alongside any
        // interactive OpenCode session — observed as silent mid-turn stalls
        // (tool result in hand, next model call never sent) and as startup
        // hangs against orphaned daemons. Isolation happens per pool WORKER
        // (see `opencode_worker_env`), because every worker of an agent is
        // its own OpenCode process: worker leaf dirs are carved from this
        // base at spawn time. `XDG_CONFIG_HOME` stays shared so the provider
        // configuration (inline API keys, model catalog) is still read.
        let opencode_data_base = config.opencode_data_base();
        if config.opencode_data_base().is_none()
            && crate::config::normalize_agent_command_identity(&config.agent_command) == "opencode"
        {
            tracing::warn!(
                "opencode data isolation: neither XDG_DATA_HOME nor HOME is set; \
                 workers share the default data home"
            );
        }
        Self {
            agents: config.agents,
            command: config.agent_command.clone(),
            args: config.agent_args.clone(),
            extra_env: config.persona_env_vars.clone(),
            opencode_data_base,
            has_generated_codex_config: config.has_generated_codex_config,
            model: config.model.clone(),
            effort_level: config.effort_level.clone(),
            observer,
        }
    }
}

/// Per-worker OpenCode data isolation. Each pool worker is its own OpenCode
/// process; giving all workers of an agent one shared data dir re-creates the
/// cross-agent sqlite contention this isolation exists to prevent (workers
/// booting or turning concurrently stall each other in ep_poll). Worker
/// `i` gets `<base>/w<i>` as its `XDG_DATA_HOME`. Returns `env` unchanged
/// when `base` is `None` (non-OpenCode agents) or the leaf dir cannot be
/// created (degrades to the shared agent data home with a warning).
pub(crate) fn opencode_worker_env(
    base: Option<&std::path::Path>,
    agent_index: usize,
    mut env: Vec<(String, String)>,
) -> Vec<(String, String)> {
    let Some(base) = base else {
        return env;
    };
    let dir = base.join(format!("w{agent_index}"));
    match std::fs::create_dir_all(&dir) {
        Ok(()) => {
            tracing::info!(
                dir = %dir.display(),
                agent = agent_index,
                "opencode data isolation: per-worker XDG_DATA_HOME"
            );
            env.push((
                "XDG_DATA_HOME".to_string(),
                dir.to_string_lossy().into_owned(),
            ));
        }
        Err(e) => {
            tracing::warn!(
                dir = %dir.display(),
                error = %e,
                "opencode data isolation: could not create per-worker data dir; \
                 worker shares the agent data home"
            );
        }
    }
    env
}

fn make_prompt_context(
    config: &Config,
    rest_client: relay::RestClient,
    channels: HashMap<Uuid, relay::ChannelInfo>,
    mode: SessionMode,
) -> Result<PromptContext> {
    let base_prompt_content = config.base_prompt_content.as_ref();
    let cwd = current_working_directory()?;
    Ok(PromptContext {
        mcp_servers: build_mcp_servers(config),
        initial_message: config.initial_message.clone(),
        idle_timeout: Duration::from_secs(config.idle_timeout_secs),
        max_turn_duration: Duration::from_secs(config.max_turn_duration_secs),
        turn_liveness_interval: Duration::from_secs(config.turn_liveness_secs),
        dedup_mode: config.dedup_mode,
        system_prompt: config.system_prompt.clone(),
        session_title: config.session_title.clone(),
        team_instructions: config.team_instructions.clone(),
        base_prompt: if config.no_base_prompt {
            None
        } else {
            // Build standing context once under the configured policy, before
            // any session/new. Both modern ACP and legacy first-turn framing
            // consume this same assembled base (including custom base files).
            let base = base_prompt_content
                .map(String::as_str)
                .unwrap_or(include_str!("base_prompt.md"));
            Some(if matches!(mode, SessionMode::Task) {
                format!("{base}\n\n{}", include_str!("session_model_task.md"))
            } else {
                config.session_policy.append_session_model(base)
            })
        },
        heartbeat_prompt: config.heartbeat_prompt.clone(),
        cwd,
        rest_client: rest_client.clone(),
        channel_info: pool::ChannelInfoResolver::new(channels, rest_client),
        context_message_limit: config.context_message_limit,
        max_turns_per_session: config.max_turns_per_session,
        permission_mode: config.permission_mode,
        agent_keys: config.keys.clone(),
        agent_owner_pubkey: resolve_agent_owner(config)
            .as_deref()
            .and_then(|hex| nostr::PublicKey::from_hex(hex).ok()),
        memory_enabled: config.memory_enabled,
        harness_name: crate::config::normalize_agent_command_identity(&config.agent_command),
        relay_url: config.relay_url.clone(),
    })
}
