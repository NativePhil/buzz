# Run the dummy user-deletion workflow

This extends the [intake and approval walkthrough](operator-user-deletion-intake.md).
The CLI worker uses PostgreSQL only. It prints dummy actions to stderr, saves
progress, and prints the final inspection as JSON to stdout. It never gates a
real user or deletes database rows, agents, storage, or communities.

Use a fresh local database for this unmerged PR's revised migrations. Keep older
demo databases intact; do not edit their sqlx migration checksums. Follow the
intake guide's container setup, build `buzz-admin`, run `buzz-admin migrate`, and
export that database's `DATABASE_URL`. The examples below use its container name
`buzz-user-deletion-intake` and database name `buzz`.

## Submit and approve

Run these in the same terminal:

```bash
TARGET="demo:workflow-$(date +%s)"
REQUEST_JSON=$(./target/debug/buzz-admin user-deletions submit \
  --target "$TARGET" --requested-by operator-a)
REQUEST_ID=$(printf '%s\n' "$REQUEST_JSON" | jq -r '.id')
INVENTORY_DIGEST=$(printf '%s\n' "$REQUEST_JSON" | jq -r '.inventory_digest')
./target/debug/buzz-admin user-deletions approve "$REQUEST_ID" \
  --approved-by operator-b --inventory-digest "$INVENTORY_DIGEST"
```

Expected: the parent is `approved`, one approval is saved, and one current
`gate_user` row is `pending` with no community.

Define a helper to see the saved state after each command:

```bash
show_rows() {
  docker exec buzz-user-deletion-intake psql -X -U buzz -d buzz -c "
    SELECT id, status, lease_owner, lease_generation, lease_expires_at,
           heartbeat_at, retry_count, next_attempt_at, last_error, completed_at
    FROM entity_deletion_requests WHERE id = '$REQUEST_ID';
    SELECT id, target_community_id, stage, status, is_current,
           started_at, finished_at, metadata_json, last_error
    FROM entity_deletion_stage_history WHERE request_id = '$REQUEST_ID'
    ORDER BY started_at NULLS LAST, id;
    SELECT id, approved_by, approved_at
    FROM entity_deletion_approvals WHERE request_id = '$REQUEST_ID'
    ORDER BY approved_at, id;"
}
show_rows
```

## Execute one stage at a time

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" \
  --executor-id worker-a --max-stages 1 | jq .
show_rows
```

The worker prints `would gate user`. The parent is `running`, generation is 1,
and the lease is released because the requested single stage finished. History
contains a completed `gate_user` row and a pending current `purge_db` row for B.

Repeat the same command and `show_rows` four more times. Each run claims a new
generation and starts from the saved current row:

| Run | Completed action | Next current row | Parent status |
| --- | --- | --- | --- |
| 1 | `gate_user` | B: `purge_db` | `running` |
| 2 | B: `purge_db` | B: `purge_agents` | `running` |
| 3 | B: `purge_agents` | C: `purge_db` | `running` |
| 4 | C: `purge_db` | C: `purge_agents` | `running` |
| 5 | C: `purge_agents` | None | `completed` |

B is `00000000-0000-0000-0000-00000000000b` (participant); C is
`00000000-0000-0000-0000-00000000000c` (former participant). At the end there are
five completed rows, no current row, and a parent completion timestamp. Approval
evidence is unchanged. Re-running a completed request is read-only.

To finish all remaining stages in one run, omit `--max-stages`:

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" --executor-id worker-b
```

## Try failure and retry

Submit and approve a new label using the first block, then run:

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" \
  --executor-id worker-a --fail-stage purge_db
show_rows
```

Expected exit code: 1. Gate is completed. B's `purge_db` is failed and still
current. The parent is `retry_wait`, `retry_count` is 1, `last_error` describes
the injected failure, and `next_attempt_at` is 30 seconds later. Ownership is
released. An immediate retry is refused without changing the rows.

After `next_attempt_at`, rerun without failure injection:

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" --executor-id worker-b
show_rows
```

B's `purge_db` reuses its row ID and original start time. The request finishes
with exactly five history rows. Successful advancement clears the parent retry
state. Runs are bounded to five stages and stop at the first failure; this CLI
does not poll or retry forever. Another invocation is required after the delay.

## Observe ownership, shutdown, and crash recovery

On a new approved request, delay each dummy stage so it can be inspected:

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" \
  --executor-id worker-a --stage-delay-ms 60000
```

From a second terminal using the same database and request ID, run `show_rows`
and attempt another `run`. The second worker is refused while worker A owns
the request. `heartbeat_at` and lease expiry advance every ten seconds.

Ctrl-C or SIGTERM stops the handler and releases its lease. The current row may
still say `running`; that describes the interrupted stage, not live ownership.
Run again to resume it. If the process is killed without cleanup, wait until
`lease_expires_at` before retrying. The new claim increments the generation,
and the old generation cannot heartbeat, fail, complete, or release that claim.

## How the code owns state

- The parent stores overall status, ownership/generation, heartbeat, retry
  scheduling, and completion time. Inspection exposes these fields under
  `execution`; list and submit retain their existing request shape.
- The current history row stores stage, community, execution status, times,
  error, and metadata. A retry updates that row, not a new attempt row.
- Each dummy handler calls `complete_stage_and_advance` after its action.
  That transaction locks the parent, validates the live claim and approved
  inventory, completes the expected current row, and inserts its successor or
  completes the parent. There is no separate switch job or completion write.
- The transition function owns the stage/community sequence. The executor
  simply rereads the saved current row and dispatches it.
- Existing community-deletion execution stays unchanged. The user path reuses
  the shared database connection, shutdown and stage-cancellation helpers, and
  lease timing, with user-specific SQL and a small explicit handler dispatch.

No relay scheduler, production user fencing, owned-community handling, or real
deletion handlers are enabled by this PR. The lease generation protects workflow
ownership only. Real database handlers will need a transaction-aware completion
interface so their mutations and advancement can commit together; external
handlers will need idempotent reconciliation before recording completion.
