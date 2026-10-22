//! Bounded, PostgreSQL-only execution of the five-stage dummy workflow.

use std::time::Duration;

use anyhow::{bail, Result};
use buzz_db::entity_deletion::{EntityDeletionStore, EntityLease, EntityStage};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{StageOutcome, DEFAULT_LEASE_DURATION, HEARTBEAT_INTERVAL, RETRY_DELAY};

/// Controls for a synthetic run. Delays and failures never invoke real deletion.
pub(super) struct RunOptions {
    pub max_stages: u16,
    pub fail_stage: Option<String>,
    pub stage_delay: Duration,
}

pub(super) async fn run(
    store: &EntityDeletionStore,
    id: Uuid,
    owner: &str,
    options: &RunOptions,
    shutdown: &CancellationToken,
) -> Result<i32> {
    let Some(lease) = store.claim(id, owner, DEFAULT_LEASE_DURATION).await? else {
        if store.get(id).await?.status == "completed" {
            return Ok(0);
        }
        bail!("request is unapproved, not yet due for retry, or leased by another worker");
    };
    let result = execute_claim(store, &lease, options, shutdown).await;
    // This generation-checked release is harmless after completion, a recorded
    // failure, or takeover. Never leave a background heartbeat running on return.
    let released = store.release(&lease).await;
    match result {
        Err(error) => {
            released?;
            Err(error)
        }
        Ok(code) => {
            released?;
            Ok(code)
        }
    }
}

async fn execute_claim(
    store: &EntityDeletionStore,
    lease: &EntityLease,
    options: &RunOptions,
    shutdown: &CancellationToken,
) -> Result<i32> {
    // One invocation stops on its first failure and never spins on a retry.
    // The CLI bounds max_stages to the five steps in this fixed dummy workflow.
    for _ in 0..options.max_stages {
        if shutdown.is_cancelled() {
            return Ok(0);
        }
        let stage = store.start_current(lease).await?;
        let outcome =
            run_stage_with_heartbeat(store, lease, &stage, options, shutdown, HEARTBEAT_INTERVAL)
                .await;
        match outcome {
            StageOutcome::Completed => {
                if store.get(lease.request_id).await?.status == "completed" {
                    return Ok(0);
                }
            }
            StageOutcome::Shutdown => return Ok(0),
            StageOutcome::Failed(error) => {
                eprintln!("dummy stage {} failed: {error:#}", stage.stage);
                // An uncertain completion response can arrive after its commit.
                // Never mark a successor stage failed using the previous row id.
                if store.get(lease.request_id).await?.status == "completed" {
                    return Ok(0);
                }
                store
                    .fail_stage(lease, stage.id, &format!("{error:#}"), RETRY_DELAY)
                    .await?;
                return Ok(1);
            }
        }
    }
    Ok(0)
}

async fn run_stage_with_heartbeat(
    store: &EntityDeletionStore,
    lease: &EntityLease,
    stage: &EntityStage,
    options: &RunOptions,
    shutdown: &CancellationToken,
    heartbeat_period: Duration,
) -> StageOutcome {
    let handler = handlers::dispatch(store, lease, stage, options);
    tokio::pin!(handler);
    let mut interval = tokio::time::interval(heartbeat_period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let heartbeat_lost = CancellationToken::new();
    loop {
        // Reuse the community executor's cancellation/outcome helper. Keeping
        // renewal in this task avoids a detached heartbeat when a run is dropped.
        tokio::select! {
            biased;
            result = crate::await_stage(&mut handler, shutdown, &heartbeat_lost) => return result,
            _ = interval.tick() => {
                if let Err(error) = store.heartbeat(lease, DEFAULT_LEASE_DURATION).await {
                    return StageOutcome::Failed(error.into());
                }
            }
        }
    }
}

mod handlers {
    use super::*;

    pub(super) async fn dispatch(
        store: &EntityDeletionStore,
        lease: &EntityLease,
        stage: &EntityStage,
        options: &RunOptions,
    ) -> Result<()> {
        match stage.stage.as_str() {
            "gate_user" => gate_user(store, lease, stage, options).await,
            "purge_db" => purge_db(store, lease, stage, options).await,
            "purge_agents" => purge_agents(store, lease, stage, options).await,
            name => bail!("unknown dummy stage {name}"),
        }
    }

    async fn gate_user(
        store: &EntityDeletionStore,
        lease: &EntityLease,
        stage: &EntityStage,
        options: &RunOptions,
    ) -> Result<()> {
        dummy_action(store, lease, stage, options, "would gate user").await
    }

    async fn purge_db(
        store: &EntityDeletionStore,
        lease: &EntityLease,
        stage: &EntityStage,
        options: &RunOptions,
    ) -> Result<()> {
        dummy_action(
            store,
            lease,
            stage,
            options,
            "would purge user database rows",
        )
        .await
    }

    async fn purge_agents(
        store: &EntityDeletionStore,
        lease: &EntityLease,
        stage: &EntityStage,
        options: &RunOptions,
    ) -> Result<()> {
        dummy_action(store, lease, stage, options, "would purge user agents").await
    }

    async fn dummy_action(
        store: &EntityDeletionStore,
        lease: &EntityLease,
        stage: &EntityStage,
        options: &RunOptions,
        message: &str,
    ) -> Result<()> {
        eprintln!(
            "DUMMY request={} stage={} community={:?}: {message}",
            lease.request_id, stage.stage, stage.target_community_id
        );
        tokio::time::sleep(options.stage_delay).await;
        if options.fail_stage.as_deref() == Some(stage.stage.as_str()) {
            bail!("injected dummy failure for {}", stage.stage);
        }
        // The stage initiates completion. The store owns the single transaction
        // and decides both the next stage and the next community.
        store
            .complete_stage_and_advance(
                lease,
                stage.id,
                serde_json::json!({
                    "dummy": true,
                    "message": message,
                }),
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod postgres_tests;
