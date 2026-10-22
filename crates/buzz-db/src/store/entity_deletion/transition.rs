//! The single transition entry point: initialize, resume, wait, advance, or finish.
//! Handlers save results first. A completed current row is durable pending work
//! for this transaction; clearing it and creating its successor commit together.

use super::{execution::*, workflow::*, *};

/// The only routing information the executor needs.
#[derive(Debug)]
pub enum NextStage {
    /// Full saved row, including handler inputs in metadata_json.inputs.
    Stage(EntityStage),
    /// Successful terminal status has been durably recorded.
    Finished,
    /// A handler failed and its persisted retry deadline has not arrived.
    WaitUntil(DateTime<Utc>),
}

impl EntityDeletionStore {
    /// Reload the saved position and return work or a terminal/wait outcome.
    ///
    /// `previous_stage_id` identifies the handler that just returned. On restart
    /// pass None: the current row is authoritative. Repeated calls cannot apply
    /// an old transition twice. This method never runs a business handler.
    pub async fn get_next_stage(
        &self,
        lease: &EntityLease,
        previous_stage_id: Option<Uuid>,
    ) -> Result<NextStage> {
        let mut tx = self.pool.begin().await?;
        let request = lock_owned(&mut tx, lease).await?;
        validate_approved_inventory(&mut tx, &request).await?;
        let inventory = Inventory::read(&request)?;
        let history: Vec<EntityStage> = sqlx::query_as(
            "SELECT id, request_id, target_community_id, stage, status, is_current,
                    started_at, finished_at, metadata_json, last_error, updated_at
             FROM entity_deletion_stage_history WHERE request_id = $1 LIMIT 10001",
        )
        .bind(request.id)
        .fetch_all(&mut *tx)
        .await?;
        if history.len() > 10000 {
            return Err(invalid("workflow exceeds 10000 reached stages"));
        }
        if let Some(id) = previous_stage_id {
            if !history.iter().any(|row| row.id == id) {
                return Err(invalid("previous stage does not belong to this request"));
            }
        }
        let mut progress = Progress::rebuild(&request, &inventory, &history)?;
        if request.status == "completed" {
            if !progress.terminal || progress.current.is_some() {
                return Err(invalid("completed request has inconsistent stage history"));
            }
            return Ok(NextStage::Finished);
        }
        let Some(current) = progress.current.take() else {
            if request.status != "approved" || !history.is_empty() {
                return Err(invalid(
                    "started request has no current stage or terminal outcome",
                ));
            }
            let row = insert_stage(
                &mut tx,
                &request,
                &inventory,
                None,
                Step {
                    name: INITIAL_STAGE,
                    community: None,
                },
            )
            .await?;
            sqlx::query("UPDATE entity_deletion_requests SET status = 'running', updated_at = clock_timestamp() WHERE id = $1")
                .bind(request.id).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(NextStage::Stage(row));
        };
        match current.status.as_str() {
            "pending" | "running" => {
                if request.status != "running" {
                    return Err(invalid("executable stage has inconsistent parent status"));
                }
                Ok(NextStage::Stage(current))
            }
            "failed" => {
                let retry: Option<(DateTime<Utc>, bool)> = sqlx::query_as(
                    "SELECT next_attempt_at, next_attempt_at > clock_timestamp()
                     FROM entity_deletion_requests WHERE id = $1 AND status = 'retry_wait' AND next_attempt_at IS NOT NULL",
                ).bind(request.id).fetch_optional(&mut *tx).await?;
                let (deadline, waiting) =
                    retry.ok_or_else(|| invalid("failed stage has no retry record"))?;
                if waiting {
                    Ok(NextStage::WaitUntil(deadline))
                } else {
                    Ok(NextStage::Stage(current))
                }
            }
            "completed" => {
                let outcome = decision(&current)?;
                let successor = progress.apply(&inventory, &current, outcome)?;
                sqlx::query(
                    "UPDATE entity_deletion_stage_history SET is_current = false,
                     metadata_json = jsonb_set(metadata_json, '{transition}', $2),
                     updated_at = clock_timestamp() WHERE id = $1",
                )
                .bind(current.id)
                .bind(serde_json::to_value(outcome)?)
                .execute(&mut *tx)
                .await?;
                let next = if let Some(step) = successor {
                    NextStage::Stage(
                        insert_stage(&mut tx, &request, &inventory, Some(current.id), step).await?,
                    )
                } else {
                    if progress.completed_communities.len() != inventory.communities.len() {
                        return Err(invalid("terminal transition leaves uncovered communities"));
                    }
                    NextStage::Finished
                };
                let finished = matches!(next, NextStage::Finished);
                sqlx::query(
                    "UPDATE entity_deletion_requests
                     SET status = CASE WHEN $2 THEN 'completed' ELSE 'running' END,
                         completed_at = CASE WHEN $2 THEN clock_timestamp() ELSE NULL END,
                         retry_count = 0, next_attempt_at = NULL, last_error = NULL,
                         updated_at = clock_timestamp() WHERE id = $1",
                )
                .bind(request.id)
                .bind(finished)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                Ok(next)
            }
            _ => Err(invalid("unknown current stage status")),
        }
    }
}

async fn insert_stage(
    tx: &mut Tx<'_>,
    request: &EntityRequest,
    inventory: &Inventory,
    previous: Option<Uuid>,
    step: Step,
) -> Result<EntityStage> {
    let metadata = serde_json::json!({
        "inputs": inventory.inputs(&request.target, step.community)?,
        "previous_stage_id": previous,
    });
    Ok(sqlx::query_as(
        "INSERT INTO entity_deletion_stage_history (request_id, target_community_id, stage, metadata_json)
         VALUES ($1, $2, $3, $4)
         RETURNING id, request_id, target_community_id, stage, status, is_current,
                   started_at, finished_at, metadata_json, last_error, updated_at",
    ).bind(request.id).bind(step.community).bind(step.name).bind(metadata).fetch_one(&mut **tx).await?)
}
