//! Parent-request ownership and stage attempts for the synthetic workflow.

use std::time::Duration;

use super::*;

type Tx<'a> = sqlx::Transaction<'a, sqlx::Postgres>;

/// Ownership token; this generation fences executor writes, not serving traffic.
#[derive(Debug, Clone)]
pub struct EntityLease {
    /// Claimed parent request.
    pub request_id: Uuid,
    /// Worker identity recorded on the parent.
    pub owner: String,
    /// Incremented whenever a worker claims or reclaims the request.
    pub generation: i64,
}

/// Execution fields stored on the parent, exposed by inspection.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EntityExecution {
    /// Worker that currently owns the request, if any.
    pub lease_owner: Option<String>,
    /// Monotonic claim generation.
    pub lease_generation: i64,
    /// Database-clock deadline for ownership.
    pub lease_expires_at: Option<DateTime<Utc>>,
    /// Most recent claim or heartbeat time.
    pub heartbeat_at: Option<DateTime<Utc>>,
    /// Recorded failures of the current stage; reset on advancement.
    pub retry_count: i32,
    /// Earliest time another run may retry a failed stage.
    pub next_attempt_at: Option<DateTime<Utc>>,
    /// Last recorded stage failure.
    pub last_error: Option<String>,
    /// Time all synthetic stages completed.
    pub completed_at: Option<DateTime<Utc>>,
}

pub(super) async fn read_execution(tx: &mut Tx<'_>, id: Uuid) -> Result<EntityExecution> {
    Ok(sqlx::query_as(
        "SELECT lease_owner, lease_generation, lease_expires_at, heartbeat_at,
                retry_count, next_attempt_at, last_error, completed_at
         FROM entity_deletion_requests WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await?)
}

impl EntityDeletionStore {
    /// Claim one approved, due, unleased request; concurrent claimers cannot both win.
    ///
    /// Expired running requests resume their current row with a new generation.
    /// A completed, unapproved, not-yet-due, or already-owned request returns `None`.
    pub async fn claim(
        &self,
        id: Uuid,
        owner: &str,
        lease_duration: Duration,
    ) -> Result<Option<EntityLease>> {
        validate_operator(owner)?;
        let millis = duration_millis(lease_duration)?;
        let mut tx = self.pool.begin().await?;
        let request: Option<EntityRequest> = sqlx::query_as(
            "SELECT id, entity_type, target, status, requested_by, reason, created_at,
                    updated_at, inventory_manifest, inventory_digest
             FROM entity_deletion_requests
             WHERE id = $1 AND status IN ('approved', 'running', 'retry_wait')
               AND (lease_expires_at IS NULL OR lease_expires_at <= clock_timestamp())
               AND (next_attempt_at IS NULL OR next_attempt_at <= clock_timestamp())
             FOR UPDATE SKIP LOCKED",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(request) = request else {
            return Ok(None);
        };
        validate_approved_inventory(&mut tx, &request).await?;
        let stage = current_stage(&mut tx, id).await?;
        super::transition::next_step(&request, &stage)?;
        let generation = sqlx::query_scalar(
            "UPDATE entity_deletion_requests
             SET status = 'running', lease_owner = $2,
                 lease_generation = lease_generation + 1,
                 lease_expires_at = clock_timestamp() + $3 * interval '1 millisecond',
                 heartbeat_at = clock_timestamp(), next_attempt_at = NULL,
                 updated_at = clock_timestamp()
             WHERE id = $1 RETURNING lease_generation",
        )
        .bind(id)
        .bind(owner)
        .bind(millis)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(EntityLease {
            request_id: id,
            owner: owner.to_owned(),
            generation,
        }))
    }

    /// Renew an unexpired lease. An expired or superseded worker cannot revive it.
    pub async fn heartbeat(&self, lease: &EntityLease, duration: Duration) -> Result<()> {
        let millis = duration_millis(duration)?;
        let mut tx = self.pool.begin().await?;
        lock_owned(&mut tx, lease).await?;
        sqlx::query(
            "UPDATE entity_deletion_requests
             SET lease_expires_at = clock_timestamp() + $2 * interval '1 millisecond',
                 heartbeat_at = clock_timestamp()
             WHERE id = $1",
        )
        .bind(lease.request_id)
        .bind(millis)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Mark the saved current stage running, preserving its first start time on retry.
    pub async fn start_current(&self, lease: &EntityLease) -> Result<EntityStage> {
        let mut tx = self.pool.begin().await?;
        let request = lock_owned(&mut tx, lease).await?;
        validate_approved_inventory(&mut tx, &request).await?;
        let stage = current_stage(&mut tx, lease.request_id).await?;
        super::transition::next_step(&request, &stage)?;
        let stage = sqlx::query_as(
            "UPDATE entity_deletion_stage_history
             SET status = 'running', started_at = COALESCE(started_at, clock_timestamp()),
                 updated_at = clock_timestamp()
             WHERE id = $1
             RETURNING id, request_id, target_community_id, stage, status, is_current,
                       started_at, finished_at, metadata_json, last_error, updated_at",
        )
        .bind(stage.id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(stage)
    }

    /// Record failure on the same current row and release the parent until retry is due.
    pub async fn fail_stage(
        &self,
        lease: &EntityLease,
        stage_id: Uuid,
        message: &str,
        retry_delay: Duration,
    ) -> Result<()> {
        let millis = duration_millis(retry_delay)?;
        let message: String = message.chars().take(2048).collect();
        let mut tx = self.pool.begin().await?;
        lock_owned(&mut tx, lease).await?;
        require_running_stage(&mut tx, lease, stage_id).await?;
        sqlx::query(
            "UPDATE entity_deletion_stage_history
             SET status = 'failed', last_error = $2, updated_at = clock_timestamp()
             WHERE id = $1",
        )
        .bind(stage_id)
        .bind(&message)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE entity_deletion_requests
             SET status = 'retry_wait', retry_count = retry_count + 1, last_error = $2,
                 next_attempt_at = clock_timestamp() + $3 * interval '1 millisecond',
                 lease_owner = NULL, lease_expires_at = NULL, updated_at = clock_timestamp()
             WHERE id = $1",
        )
        .bind(lease.request_id)
        .bind(message)
        .bind(millis)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Release this live claim after a pause or shutdown, without advancing its stage.
    ///
    /// Returns false after completion or ownership loss; never clears a successor's lease.
    pub async fn release(&self, lease: &EntityLease) -> Result<bool> {
        Ok(sqlx::query(
            "UPDATE entity_deletion_requests SET lease_owner = NULL, lease_expires_at = NULL,
                 updated_at = clock_timestamp()
             WHERE id = $1 AND lease_owner = $2 AND lease_generation = $3
               AND status = 'running' AND lease_expires_at > clock_timestamp()",
        )
        .bind(lease.request_id)
        .bind(&lease.owner)
        .bind(lease.generation)
        .execute(&self.pool)
        .await?
        .rows_affected()
            == 1)
    }
}

pub(super) async fn lock_owned(tx: &mut Tx<'_>, lease: &EntityLease) -> Result<EntityRequest> {
    // Lock by id first, then check expiry after any wait for that lock. An old
    // snapshot/transaction start time must not extend a worker's authority.
    let request: EntityRequest = sqlx::query_as(
        "SELECT id, entity_type, target, status, requested_by, reason, created_at,
                updated_at, inventory_manifest, inventory_digest
         FROM entity_deletion_requests WHERE id = $1 FOR UPDATE",
    )
    .bind(lease.request_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| stale_lease(lease))?;
    let owned: bool = sqlx::query_scalar(
        "SELECT COALESCE(status = 'running' AND lease_owner = $2 AND lease_generation = $3
                AND lease_expires_at > clock_timestamp(), false)
         FROM entity_deletion_requests WHERE id = $1",
    )
    .bind(lease.request_id)
    .bind(&lease.owner)
    .bind(lease.generation)
    .fetch_one(&mut **tx)
    .await?;
    if !owned {
        return Err(stale_lease(lease));
    }
    Ok(request)
}

pub(super) async fn current_stage(tx: &mut Tx<'_>, id: Uuid) -> Result<EntityStage> {
    let stage: EntityStage = sqlx::query_as(
        "SELECT id, request_id, target_community_id, stage, status, is_current,
                started_at, finished_at, metadata_json, last_error, updated_at
         FROM entity_deletion_stage_history WHERE request_id = $1 AND is_current",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| DbError::DeletionSafety("unfinished request has no current stage".into()))?;
    if !matches!(stage.status.as_str(), "pending" | "running" | "failed") {
        return Err(DbError::DeletionSafety(
            "current stage is already completed".into(),
        ));
    }
    Ok(stage)
}

pub(super) async fn require_running_stage(
    tx: &mut Tx<'_>,
    lease: &EntityLease,
    stage_id: Uuid,
) -> Result<EntityStage> {
    let stage = current_stage(tx, lease.request_id).await?;
    if stage.id != stage_id || stage.status != "running" {
        return Err(DbError::DeletionSafety(
            "stage is no longer current and running".into(),
        ));
    }
    Ok(stage)
}

pub(super) async fn validate_approved_inventory(
    tx: &mut Tx<'_>,
    request: &EntityRequest,
) -> Result<()> {
    validate_submission(
        &request.target,
        &request.requested_by,
        request.reason.as_deref(),
    )?;
    let inventory = demo_inventory(&request.target);
    let digest = inventory_digest(&inventory)?;
    if request.inventory_manifest.as_ref() != Some(&inventory)
        || request.inventory_digest.as_deref() != Some(&digest)
    {
        return Err(DbError::DeletionSafety(
            "saved demo inventory changed".into(),
        ));
    }
    let approved: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM entity_deletion_approvals
         WHERE request_id = $1 AND inventory_digest = $2)",
    )
    .bind(request.id)
    .bind(digest)
    .fetch_one(&mut **tx)
    .await?;
    if !approved {
        return Err(DbError::DeletionSafety(
            "saved inventory has no approval".into(),
        ));
    }
    Ok(())
}

fn duration_millis(duration: Duration) -> Result<i64> {
    if duration.is_zero() || duration > Duration::from_secs(3600) || duration.as_millis() == 0 {
        return Err(DbError::InvalidData(
            "lease/retry duration must be 1ms..1h".into(),
        ));
    }
    Ok(duration.as_millis() as i64)
}

fn stale_lease(lease: &EntityLease) -> DbError {
    // Use the existing deletion lease classifier for executor control flow.
    DbError::AccessDenied(format!(
        "stale deletion lease {} owner {:?} generation {}",
        lease.request_id, lease.owner, lease.generation
    ))
}

#[cfg(test)]
#[path = "execution_postgres_tests.rs"]
mod postgres_tests;
