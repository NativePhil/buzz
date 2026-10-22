//! The one place that completes a stage and chooses its successor.

use super::{execution::*, *};

#[derive(Debug, PartialEq)]
pub(super) struct Step {
    stage: &'static str,
    community: Option<Uuid>,
}

pub(super) fn next_step(request: &EntityRequest, current: &EntityStage) -> Result<Option<Step>> {
    let inventory = request
        .inventory_manifest
        .as_ref()
        .ok_or_else(|| DbError::DeletionSafety("missing demo inventory".into()))?;
    let communities = inventory["communities"]
        .as_array()
        .ok_or_else(|| DbError::DeletionSafety("missing demo communities".into()))?;
    if communities.len() != 2 {
        return Err(DbError::DeletionSafety(
            "dummy workflow requires two communities".into(),
        ));
    }
    let mut steps = vec![Step {
        stage: "gate_user",
        community: None,
    }];
    for community in communities {
        let id = community["community_id"]
            .as_str()
            .ok_or_else(|| DbError::DeletionSafety("missing demo community id".into()))?;
        let id = Uuid::parse_str(id).map_err(|e| DbError::InvalidData(e.to_string()))?;
        for stage in ["purge_db", "purge_agents"] {
            steps.push(Step {
                stage,
                community: Some(id),
            });
        }
    }
    let position = steps
        .iter()
        .position(|step| {
            step.stage == current.stage && step.community == current.target_community_id
        })
        .ok_or_else(|| {
            DbError::DeletionSafety("current stage is outside the approved dummy workflow".into())
        })?;
    Ok(steps.into_iter().nth(position + 1))
}

impl EntityDeletionStore {
    /// Atomically finish the supplied current stage and save its successor.
    ///
    /// Handlers call this only after their work succeeds. The executor never
    /// chooses the next community or independently marks a stage completed.
    /// Returns true when this transition completes the entire request.
    pub async fn complete_stage_and_advance(
        &self,
        lease: &EntityLease,
        stage_id: Uuid,
        metadata: serde_json::Value,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let request = lock_owned(&mut tx, lease).await?;
        validate_approved_inventory(&mut tx, &request).await?;
        let current = require_running_stage(&mut tx, lease, stage_id).await?;
        let next = next_step(&request, &current)?;
        sqlx::query(
            "UPDATE entity_deletion_stage_history
             SET status = 'completed', is_current = false, finished_at = clock_timestamp(),
                 metadata_json = $2, last_error = NULL, updated_at = clock_timestamp()
             WHERE id = $1",
        )
        .bind(stage_id)
        .bind(metadata)
        .execute(&mut *tx)
        .await?;
        if let Some(ref next) = next {
            sqlx::query(
                "INSERT INTO entity_deletion_stage_history (request_id, target_community_id, stage)
                 VALUES ($1, $2, $3)",
            )
            .bind(lease.request_id)
            .bind(next.community)
            .bind(next.stage)
            .execute(&mut *tx)
            .await?;
        }
        let completed = next.is_none();
        sqlx::query(
            "UPDATE entity_deletion_requests
             SET status = CASE WHEN $2 THEN 'completed' ELSE 'running' END,
                 completed_at = CASE WHEN $2 THEN clock_timestamp() ELSE NULL END,
                 lease_owner = CASE WHEN $2 THEN NULL ELSE lease_owner END,
                 lease_expires_at = CASE WHEN $2 THEN NULL ELSE lease_expires_at END,
                 retry_count = 0, next_attempt_at = NULL, last_error = NULL,
                 updated_at = clock_timestamp()
             WHERE id = $1",
        )
        .bind(lease.request_id)
        .bind(completed)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(completed)
    }
}
