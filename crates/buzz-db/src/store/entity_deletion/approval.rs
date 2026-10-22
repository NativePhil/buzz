//! Approval records authorization; the transition function initializes execution.

use super::*;

/// A reached stage. Retries will update the same row, not append attempts.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EntityStage {
    /// Stable stage-history row identifier.
    pub id: Uuid,
    /// Parent workflow; application transactions enforce this relationship.
    pub request_id: Uuid,
    /// Community provenance, or null for a request-wide stage.
    pub target_community_id: Option<Uuid>,
    /// Action to dispatch.
    pub stage: String,
    /// `pending`, `running`, `failed`, or `completed`.
    pub status: String,
    /// Whether this is the workflow's resume position.
    pub is_current: bool,
    /// First execution time; pending stages have not started.
    pub started_at: Option<DateTime<Utc>>,
    /// Successful completion time.
    pub finished_at: Option<DateTime<Utc>>,
    /// Stage-specific saved progress.
    pub metadata_json: serde_json::Value,
    /// Most recent failure, if any.
    pub last_error: Option<String>,
    /// Last stage update.
    pub updated_at: DateTime<Utc>,
}

/// One retained approval of a particular request inventory.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EntityApproval {
    /// Stable identifier; identical approval retries retain it.
    pub id: Uuid,
    /// Parent request, checked and locked by the approval transaction.
    pub request_id: Uuid,
    /// SHA-256 of the inventory this operator approved.
    pub inventory_digest: String,
    /// Self-asserted demo identity, not authenticated real-deletion authorization.
    pub approved_by: String,
    /// Time this approval was first recorded.
    pub approved_at: DateTime<Utc>,
}

/// A consistent snapshot of a request, its approvals, and reached stages.
#[derive(Debug, Serialize)]
pub struct EntityInspection {
    /// Request fields remain at the top level of CLI JSON.
    #[serde(flatten)]
    pub request: EntityRequest,
    /// Parent ownership, heartbeat, and retry state.
    pub execution: EntityExecution,
    /// Retained approvals, ordered by approval time and identifier.
    pub approvals: Vec<EntityApproval>,
    /// Reached stages, including the current resume position after execution starts.
    pub stage_history: Vec<EntityStage>,
}

impl EntityDeletionStore {
    /// Inspect the parent and its history from one database snapshot.
    pub async fn inspect(&self, id: Uuid) -> Result<EntityInspection> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let request = sqlx::query_as(
            "SELECT id, entity_type, target, status, requested_by, reason, created_at, updated_at,
                    inventory_manifest, inventory_digest
             FROM entity_deletion_requests WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("entity deletion request {id}")))?;
        let execution = execution::read_execution(&mut tx, id).await?;
        let approvals = sqlx::query_as(
            "SELECT id, request_id, inventory_digest, approved_by, approved_at
             FROM entity_deletion_approvals WHERE request_id = $1
             ORDER BY approved_at, id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        let stage_history = sqlx::query_as(
            "SELECT id, request_id, target_community_id, stage, status, is_current,
                    started_at, finished_at, metadata_json, last_error, updated_at
             FROM entity_deletion_stage_history WHERE request_id = $1
             ORDER BY started_at NULLS LAST, id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(EntityInspection {
            request,
            execution,
            approvals,
            stage_history,
        })
    }

    /// Record approval of the saved inventory without creating or executing stages.
    ///
    /// An identical retry is read-only. Another operator adds retained evidence
    /// without restarting the workflow. The operator identity is self-asserted and
    /// this demo-only boundary does not authorize real user deletion.
    pub async fn approve(
        &self,
        id: Uuid,
        approved_by: &str,
        expected_inventory_digest: &str,
    ) -> Result<EntityRequest> {
        validate_operator(approved_by)?;
        let mut tx = self.pool.begin().await?;
        let request: EntityRequest = sqlx::query_as(
            "SELECT id, entity_type, target, status, requested_by, reason, created_at, updated_at,
                    inventory_manifest, inventory_digest
             FROM entity_deletion_requests WHERE id = $1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("entity deletion request {id}")))?;
        let digest = workflow::validated_inventory_digest(&request)?;
        if expected_inventory_digest != digest {
            return Err(DbError::DeletionSafety(
                "approval does not match saved inventory".into(),
            ));
        }
        let has_history: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM entity_deletion_stage_history WHERE request_id = $1)",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if has_history {
            return Err(DbError::DeletionSafety(
                "approval cannot change a started workflow".into(),
            ));
        }
        if request.status == "approved" {
            let has_approval: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM entity_deletion_approvals
                 WHERE request_id = $1 AND inventory_digest = $2)",
            )
            .bind(id)
            .bind(&digest)
            .fetch_one(&mut *tx)
            .await?;
            if !has_approval {
                return Err(DbError::DeletionSafety(
                    "approved request has no approval for its saved inventory".into(),
                ));
            }
            save_approval(&mut tx, id, approved_by, &digest).await?;
            tx.commit().await?;
            return Ok(request);
        }
        if request.status != "inventoried" {
            return Err(DbError::DeletionSafety("request is not inventoried".into()));
        }
        let request = sqlx::query_as(
            "UPDATE entity_deletion_requests
             SET status = 'approved', updated_at = now()
             WHERE id = $1
             RETURNING id, entity_type, target, status, requested_by, reason, created_at,
                       updated_at, inventory_manifest, inventory_digest",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        save_approval(&mut tx, id, approved_by, &digest).await?;
        tx.commit().await?;
        Ok(request)
    }
}

// The caller holds the parent row lock and has validated inventory and lifecycle.
async fn save_approval(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    request_id: Uuid,
    approved_by: &str,
    inventory_digest: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO entity_deletion_approvals (request_id, inventory_digest, approved_by)
         VALUES ($1, $2, $3)
         ON CONFLICT (request_id, inventory_digest, approved_by) DO NOTHING",
    )
    .bind(request_id)
    .bind(inventory_digest)
    .bind(approved_by)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
