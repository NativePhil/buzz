//! Ownership and dispatch only; get_next_stage owns all workflow routing.

use std::time::Duration;

use anyhow::{bail, Result};
use buzz_db::entity_deletion::{EntityDeletionStore, EntityLease, EntityStage, NextStage};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{StageOutcome, DEFAULT_LEASE_DURATION, HEARTBEAT_INTERVAL, RETRY_DELAY};

/// Controls for a synthetic run. Delays and failures never invoke real deletion.
pub(super) struct RunOptions {
    pub max_stages: u16,
    pub fail_stage: Option<String>,
    pub stage_delay: Duration,
    pub no_agents_in: Vec<Uuid>,
    pub pause_after_handler: bool,
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
    let mut previous_stage_id = None;
    let mut executed = 0;
    loop {
        if shutdown.is_cancelled() {
            return Ok(0);
        }
        match store.get_next_stage(lease, previous_stage_id).await? {
            NextStage::Finished => return Ok(0),
            NextStage::WaitUntil(deadline) => {
                eprintln!("request {} waits until {deadline}", lease.request_id);
                return Ok(1);
            }
            NextStage::Stage(row) => {
                if executed >= options.max_stages {
                    return Ok(0);
                }
                match run_stage_with_heartbeat(
                    store,
                    lease,
                    &row,
                    options,
                    shutdown,
                    HEARTBEAT_INTERVAL,
                )
                .await
                {
                    StageOutcome::Completed => {}
                    StageOutcome::Shutdown => return Ok(0),
                    StageOutcome::Failed(error) => return Err(error),
                }
                previous_stage_id = Some(row.id);
                executed += 1;
                if options.pause_after_handler {
                    return Ok(0);
                }
            }
        }
    }
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
    // Poll renewal alongside the handler. Awaiting renewal inside a selected
    // timer branch would stop polling a handler that may hold the parent lock.
    // Both futures live here, so cancellation leaves no detached heartbeat task.
    let renewal = async {
        loop {
            interval.tick().await;
            if let Err(error) = store.heartbeat(lease, DEFAULT_LEASE_DURATION).await {
                return error;
            }
        }
    };
    tokio::select! {
        biased;
        result = crate::await_stage(&mut handler, shutdown, &heartbeat_lost) => result,
        error = renewal => StageOutcome::Failed(error.into()),
    }
}

mod handlers;

#[cfg(test)]
mod postgres_tests;
