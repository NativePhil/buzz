# Run and recover a dummy user-deletion workflow

This workflow tests orchestration using real PostgreSQL rows and dummy actions.
It never gates a real account or deletes user data. Submit still creates an
artificial inventory: B (participant), then C (former participant). Only `demo:`
targets are accepted. Owned communities and production fencing are future work.

## The execution contract

```text
approve: save approval, create no stages
run: claim parent request
    get_next_stage(lease, None) → full stage row
    handler(lease, row) → persist status and result
    get_next_stage(lease, Some(row.id)) → full next row, Finished, or WaitUntil
    repeat; release ownership on return
```

The worker owns claiming, heartbeats, shutdown, dispatch, and a bounded execution
budget. It does not inspect handler results, reconstruct community progress, or
choose branches. `get_next_stage` owns all those decisions. Specific rules live
in its private `workflow.rs` module; the transition transaction contains no
stage-name comparisons or two-community requirement.

The handler gets the whole row. `metadata_json.inputs` holds the target and
community input. The handler marks the row running and either saves success
under `metadata_json.result` or records a failure and retry deadline. Returning
success from the handler means its outcome was saved, including a retryable
failure. Failure to save propagates out of the worker.

### Durable state

| Record | State |
| --- | --- |
| `entity_deletion_requests` | Inventory/digest, overall lifecycle, lease owner/generation/expiry, heartbeat, retry deadline/error/count, completion time |
| `entity_deletion_approvals` | Operator, time, approved inventory digest; retained separately |
| `entity_deletion_stage_history` | Stage/community, current position, execution status, timestamps, inputs, result, transition decision, predecessor ID |

`is_current` means the workflow's durable position. It does not mean a handler
is presently running. In particular, **completed + current is valid**: the
handler finished, but the transition still needs to be committed.

The handler's success transaction sets `completed`, saves its result and finish
time, and leaves `is_current = true`. The next transition transaction checks the
lease, reloads that result, chooses an allowed branch, clears the old current
flag, saves its decision, and inserts the successor with its inputs. If there
is no successor, it records request completion instead. Those transition writes
commit together; an insertion failure leaves the completed old row current.

Every inserted stage stores `metadata_json.previous_stage_id` (null for the
initial stage). Reconstruction follows this chain, not timestamps or UUID sort
order. It checks inputs against the approved inventory, validates saved
transition decisions, and builds the set of completed communities. A persisted
`community_complete` decision covers a community even when a runtime branch
skipped a stage. Unknown stages, disconnected history, and missing results fail
without inventing progress. Reads are bounded to 10,000 reached stages and
inventories to 1,000 communities; these are resource limits, not a demo shape.

### Starting, branching, and finishing

Claiming saves ownership while preserving the lifecycle status. Thus a crash
between claim and initialization leaves an `approved` request with no stages.
The first `get_next_stage` creates the configured initial stage and sets the
parent to `running` in one transaction. A previously started request with no
current row and no terminal outcome is an error, not permission to create a
second gate.

The version-1 workflow rules are:

| Stage | Allowed outcome | Runtime choice |
| --- | --- | --- |
| `gate_user` | Enter communities | Select the first uncovered community |
| `purge_db` | `purge_agents` or community complete | Read saved boolean `result.has_agents` |
| `purge_agents` | Community complete | Mark this community covered |

After a community completes, select the first uncovered community in inventory
order. After all are covered, finish the request. Missing `has_agents` is an
error, not an implicit skip. The inventory's version selects the currently
supported rules; future versions must retain the rules needed by active requests.
No full execution plan is created upfront.

`previous_stage_id` is bookkeeping for uninterrupted execution. It must belong
to this request. After restart pass `None`; the database current row is the
source of truth. An old non-current stage is never transitioned twice.

| Restart point | Recovery |
| --- | --- |
| Before the first transition | Initialize an approved request with empty history |
| During a handler | Resume the unfinished current row after lease takeover |
| After a saved failure | Wait for the deadline, then retry the same row |
| After saved success, before transition | Process the completed current row's saved result; do not rerun its handler |
| After transition commit | Return the saved successor row |
| After terminal commit | A rerun returns success without changing saved state |

A lease lasts 60 seconds and handlers renew it every ten seconds. Handler and
renewal futures are polled independently in the same task, so a renewal waiting
for the handler's parent lock cannot stop the handler from committing. No
detached renewal task survives cancellation. Generation checks reject stale
workers' status, result, transition, heartbeat, and release writes.

This is one request at a time through an explicit CLI command. There is no relay
scheduler or automatic retry daemon. Real handlers will need idempotent or
reconciled external actions: a crash after an external action but before saving
its result may repeat that handler.

## Prepare a local test

Follow [the intake guide](operator-user-deletion-intake.md) to build the binary
and migrate a fresh disposable database. The commands below use its container
`buzz-user-deletion-intake` and database `buzz`. Keep all commands in one shell.
These commands need PostgreSQL only, not a relay, Redis, or storage credentials.

```bash
TARGET="demo:workflow-$(date +%s)"
REQUEST_JSON=$(./target/debug/buzz-admin user-deletions submit \
  --target "$TARGET" --requested-by operator-a)
REQUEST_ID=$(printf '%s\n' "$REQUEST_JSON" | jq -r '.id')
INVENTORY_DIGEST=$(printf '%s\n' "$REQUEST_JSON" | jq -r '.inventory_digest')
./target/debug/buzz-admin user-deletions approve "$REQUEST_ID" \
  --approved-by operator-b --inventory-digest "$INVENTORY_DIGEST"
```

Expected after approval: parent `approved`, one approval, **zero stage rows**.

Define a helper to inspect the actual tables after each command:

```bash
show_rows() {
  docker exec buzz-user-deletion-intake psql -X -U buzz -d buzz -c "
    SELECT target, status, lease_owner, lease_generation, lease_expires_at,
           heartbeat_at, retry_count, next_attempt_at, last_error, completed_at
    FROM entity_deletion_requests WHERE id = '$REQUEST_ID';
    SELECT id, stage, target_community_id, status, is_current,
           started_at, finished_at, metadata_json, last_error
    FROM entity_deletion_stage_history WHERE request_id = '$REQUEST_ID'
    ORDER BY started_at NULLS LAST, id;
    SELECT approved_by, approved_at, inventory_digest
    FROM entity_deletion_approvals WHERE request_id = '$REQUEST_ID'
    ORDER BY approved_at, id;"
}
show_rows
```

## Observe recovery before and after the transition

Save the gate handler's result, then pause **before** processing its transition:

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" \
  --executor-id worker-a --pause-after-handler
show_rows
```

Expected: one `gate_user` row, `completed`, `is_current = true`, with inputs and
result but no transition decision. Parent `running`; ownership released.

Resume in a new process and execute one handler:

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" \
  --executor-id worker-b --max-stages 1
show_rows
```

The transition processes the saved gate result without repeating the gate.
B's `purge_db` runs, saves `has_agents: true`, and transitions. Expected history:
completed non-current gate, completed non-current B `purge_db`, pending current
B `purge_agents`. Gate's result and finish time are unchanged.

Resume again to finish:

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" --executor-id worker-c
show_rows
```

Expected: parent completed; five completed history rows; no current row; no owner.
Re-running a completed request is read-only. `--max-stages` limits handler calls
(default 100, maximum 1,000); the following transition is processed before
pausing. `--pause-after-handler` intentionally pauses before that transition.

## Test runtime branching

Submit and approve a new request using the first block. Then:

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" \
  --executor-id worker-a \
  --demo-no-agents-in 00000000-0000-0000-0000-00000000000c
show_rows
```

Expected: four completed rows: gate, B DB, B agents, C DB. C's DB result contains
`has_agents: false`; its saved transition is `community_complete`. There is no
C agents row. The flag simulates a runtime observation in the dummy handler; it
does not change the saved inventory or transition map. It may be repeated.

To prove the result survives restart, on another request pause after the gate,
then pause after B DB with `--demo-no-agents-in ...000b --pause-after-handler`.
Resume without the flag: B agents is still skipped because routing reads B DB's
saved result, not the new process's options.

## Test failure and retry

On a new approved request:

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" \
  --executor-id worker-a --fail-stage purge_db
echo "Exit code: $?"
show_rows
```

Expected exit 1. Gate has transitioned; B DB is failed and current. The parent
is `retry_wait`, with error, retry count 1, and `next_attempt_at` 30 seconds later.
The handler saved this outcome; `get_next_stage` returned `WaitUntil`, and the
worker released ownership. An immediate retry is refused.

After the saved deadline:

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" --executor-id worker-b
show_rows
```

The same B DB row resumes with its original first start time. The request
finishes. Successful transition clears the parent retry state.

## Observe heartbeat and process interruption

On a new approved request:

```bash
./target/debug/buzz-admin user-deletions run "$REQUEST_ID" \
  --executor-id worker-a --stage-delay-ms 60000
```

Inspect from another terminal using the same database/request. Heartbeat and
expiry advance every ten seconds. A competing worker is refused. Ctrl-C stops
execution and releases ownership; the interrupted row remains current. After
an abrupt process kill, wait for the saved lease expiry before running another
worker. It resumes the same unfinished row with a higher generation.
