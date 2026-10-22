use super::*;

async fn fixture() -> (EntityDeletionStore, Uuid) {
    let store = EntityDeletionStore {
        pool: PgPool::connect(&crate::test_support::database_url())
            .await
            .unwrap(),
    };
    let request = store
        .submit("demo:executor", "submitter", None)
        .await
        .unwrap();
    store
        .approve(
            request.id,
            "approver",
            request.inventory_digest.as_deref().unwrap(),
        )
        .await
        .unwrap();
    (store, request.id)
}

async fn claim(store: &EntityDeletionStore, id: Uuid, owner: &str) -> EntityLease {
    store
        .claim(id, owner, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap()
}

async fn snapshot(store: &EntityDeletionStore, id: Uuid) -> serde_json::Value {
    serde_json::to_value(store.inspect(id).await.unwrap()).unwrap()
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn concurrent_claimers_have_one_owner_and_unapproved_requests_are_not_claimed() {
    let (store, id) = fixture().await;
    let unapproved = store
        .submit("demo:unapproved", "submitter", None)
        .await
        .unwrap();
    assert!(store
        .claim(unapproved.id, "worker", Duration::from_secs(60))
        .await
        .unwrap()
        .is_none());
    let (a, b) = tokio::join!(
        store.claim(id, "worker-a", Duration::from_secs(60)),
        store.claim(id, "worker-b", Duration::from_secs(60)),
    );
    let claims: Vec<_> = [a.unwrap(), b.unwrap()].into_iter().flatten().collect();
    assert_eq!(claims.len(), 1);
    let lease = &claims[0];
    let inspection = store.inspect(id).await.unwrap();
    assert_eq!(inspection.request.status, "running");
    assert_eq!(
        inspection.execution.lease_owner.as_deref(),
        Some(lease.owner.as_str())
    );
    assert_eq!(inspection.execution.lease_generation, 1);
    assert!(store
        .claim(id, &lease.owner, Duration::from_secs(60))
        .await
        .unwrap()
        .is_none());
    let before = snapshot(&store, id).await;
    assert!(store.claim(id, "worker", Duration::ZERO).await.is_err());
    assert_eq!(snapshot(&store, id).await, before);
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn expired_and_superseded_workers_cannot_change_any_execution_state() {
    let (store, id) = fixture().await;
    let old = claim(&store, id, "worker-a").await;
    let stage = store.start_current(&old).await.unwrap();
    sqlx::query("UPDATE entity_deletion_requests SET lease_expires_at = clock_timestamp() - interval '1 second' WHERE id = $1")
        .bind(id).execute(&store.pool).await.unwrap();
    // Expiration alone rejects renewal, even before another worker claims.
    assert!(crate::deletion::is_stale_deletion_lease(
        &store
            .heartbeat(&old, Duration::from_secs(60))
            .await
            .unwrap_err()
    ));
    assert!(!store.release(&old).await.unwrap());
    let new = claim(&store, id, "worker-b").await;
    assert_eq!(new.generation, old.generation + 1);
    let resumed = store.start_current(&new).await.unwrap();
    assert_eq!(resumed.id, stage.id);
    assert_eq!(resumed.started_at, stage.started_at);
    let before = snapshot(&store, id).await;
    for error in [
        store.start_current(&old).await.unwrap_err(),
        store
            .heartbeat(&old, Duration::from_secs(60))
            .await
            .unwrap_err(),
        store
            .fail_stage(&old, stage.id, "stale failure", Duration::from_secs(30))
            .await
            .unwrap_err(),
        store
            .complete_stage_and_advance(&old, stage.id, serde_json::json!({}))
            .await
            .unwrap_err(),
    ] {
        assert!(crate::deletion::is_stale_deletion_lease(&error), "{error}");
    }
    assert!(!store.release(&old).await.unwrap());
    assert_eq!(snapshot(&store, id).await, before);
    store
        .complete_stage_and_advance(&new, stage.id, serde_json::json!({"dummy": true}))
        .await
        .unwrap();
    assert_eq!(store.inspect(id).await.unwrap().stage_history.len(), 2);
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn heartbeat_checks_authority_after_waiting_for_parent_lock() {
    let (store, id) = fixture().await;
    let lease = claim(&store, id, "worker").await;
    let before = store.inspect(id).await.unwrap().execution;
    store
        .heartbeat(&lease, Duration::from_secs(120))
        .await
        .unwrap();
    let renewed = store.inspect(id).await.unwrap().execution;
    assert!(renewed.lease_expires_at > before.lease_expires_at);
    assert!(renewed.heartbeat_at > before.heartbeat_at);

    let mut blocker = store.pool.begin().await.unwrap();
    sqlx::query("UPDATE entity_deletion_requests SET lease_expires_at = clock_timestamp() - interval '1 second' WHERE id = $1")
        .bind(id).execute(&mut *blocker).await.unwrap();
    let renewal = store.heartbeat(&lease, Duration::from_secs(60));
    tokio::pin!(renewal);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut renewal)
            .await
            .is_err()
    );
    blocker.commit().await.unwrap();
    assert!(crate::deletion::is_stale_deletion_lease(
        &renewal.await.unwrap_err()
    ));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn transition_rolls_back_completion_when_next_stage_insert_fails() {
    let (store, id) = fixture().await;
    let lease = claim(&store, id, "worker").await;
    let stage = store.start_current(&lease).await.unwrap();
    // Force a failure specifically after the current-row completion UPDATE.
    sqlx::query("ALTER TABLE entity_deletion_stage_history ADD CONSTRAINT reject_next_dummy_stage CHECK (stage <> 'purge_db') NOT VALID")
        .execute(&store.pool).await.unwrap();
    let before = snapshot(&store, id).await;
    assert!(store
        .complete_stage_and_advance(&lease, stage.id, serde_json::json!({"dummy": true}))
        .await
        .is_err());
    assert_eq!(snapshot(&store, id).await, before);
    sqlx::query(
        "ALTER TABLE entity_deletion_stage_history DROP CONSTRAINT reject_next_dummy_stage",
    )
    .execute(&store.pool)
    .await
    .unwrap();
    assert!(!store
        .complete_stage_and_advance(&lease, stage.id, serde_json::json!({"dummy": true}))
        .await
        .unwrap());
    let after = snapshot(&store, id).await;
    assert!(store
        .complete_stage_and_advance(&lease, stage.id, serde_json::json!({}))
        .await
        .is_err());
    assert!(store
        .fail_stage(&lease, stage.id, "late failure", Duration::from_secs(30))
        .await
        .is_err());
    assert_eq!(snapshot(&store, id).await, after);
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn failed_stage_is_durable_due_later_and_reuses_its_row() {
    let (store, id) = fixture().await;
    let lease = claim(&store, id, "worker-a").await;
    let stage = store.start_current(&lease).await.unwrap();
    store
        .fail_stage(&lease, stage.id, "injected", Duration::from_secs(30))
        .await
        .unwrap();
    let failed = store.inspect(id).await.unwrap();
    assert_eq!(failed.request.status, "retry_wait");
    assert_eq!(failed.execution.retry_count, 1);
    assert_eq!(failed.execution.last_error.as_deref(), Some("injected"));
    assert!(failed.execution.lease_owner.is_none());
    assert_eq!(failed.stage_history.len(), 1);
    assert_eq!(failed.stage_history[0].status, "failed");
    assert!(failed.stage_history[0].is_current);
    assert!(store
        .claim(id, "worker-b", Duration::from_secs(60))
        .await
        .unwrap()
        .is_none());
    sqlx::query("UPDATE entity_deletion_requests SET next_attempt_at = clock_timestamp() - interval '1 second' WHERE id = $1")
        .bind(id).execute(&store.pool).await.unwrap();
    let retry = claim(&store, id, "worker-b").await;
    let resumed = store.start_current(&retry).await.unwrap();
    assert_eq!(resumed.id, stage.id);
    assert_eq!(resumed.started_at, stage.started_at);
    store
        .complete_stage_and_advance(&retry, resumed.id, serde_json::json!({}))
        .await
        .unwrap();
    let after = store.inspect(id).await.unwrap();
    assert_eq!(after.execution.retry_count, 0);
    assert!(after.execution.next_attempt_at.is_none());
    assert!(after.execution.last_error.is_none());
    assert_eq!(after.stage_history.len(), 2);
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn execution_refuses_missing_approval_changed_inventory_and_unknown_stage() {
    let (store, id) = fixture().await;
    let lease = claim(&store, id, "worker").await;
    let stage = store.start_current(&lease).await.unwrap();
    let manifest = store.get(id).await.unwrap().inventory_manifest.unwrap();
    sqlx::query("UPDATE entity_deletion_requests SET inventory_manifest = '{}' WHERE id = $1")
        .bind(id)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(store
        .complete_stage_and_advance(&lease, stage.id, serde_json::json!({}))
        .await
        .is_err());
    sqlx::query("UPDATE entity_deletion_requests SET inventory_manifest = $2 WHERE id = $1")
        .bind(id)
        .bind(manifest)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM entity_deletion_approvals WHERE request_id = $1")
        .bind(id)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(store.start_current(&lease).await.is_err());
    assert!(store
        .complete_stage_and_advance(&lease, stage.id, serde_json::json!({}))
        .await
        .is_err());
    assert!(store.release(&lease).await.unwrap());
    assert!(store
        .claim(id, "worker", Duration::from_secs(60))
        .await
        .is_err());
    let digest = store.get(id).await.unwrap().inventory_digest.unwrap();
    sqlx::query("INSERT INTO entity_deletion_approvals (request_id, inventory_digest, approved_by) VALUES ($1, $2, 'approver')")
        .bind(id).bind(digest).execute(&store.pool).await.unwrap();
    sqlx::query("UPDATE entity_deletion_stage_history SET stage = 'unknown' WHERE id = $1")
        .bind(stage.id)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(store
        .claim(id, "worker", Duration::from_secs(60))
        .await
        .is_err());
    assert_eq!(store.inspect(id).await.unwrap().stage_history.len(), 1);
}
