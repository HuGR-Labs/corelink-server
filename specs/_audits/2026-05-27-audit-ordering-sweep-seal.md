---
id: "AUDIT-2026-05-27-AUDIT-ORDERING-SWEEP"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER", "ordering", "sweep", "seal"]
references:
  - "specs/_audits/2026-05-27-charter-strict-audit-post-w36.md"
  - "specs/_audits/2026-05-27-drata-fail-closed-fix.md"
---

# Audit ordering sweep SEAL (extends charter §L2.6 spot-check)

## §1 Scope

Comprehensive workspace-wide sweep of all `audit.emit` /
`emit_audit` / `AuditEmitter::emit` callsites against
INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (audit emission MUST precede any
state-mutating call in the same logical block).

Extends the charter audit §L2.6 spot-sample (5 callsites, 1 finding)
and the already-landed drata fail-CLOSED fix
(`specs/_audits/2026-05-27-drata-fail-closed-fix.md`).

## §2 Methodology

1. Discovery grep: 399 callsites across 84 source files under
   `crates/*/src/` (tests / fuzz / examples / benches excluded).
2. Heuristic scanner (Python AST-aware over the linear file): for
   each emit callsite, walk back to the enclosing `fn` and flag any
   state-mutating method calls between fn-start and emit. Mutation
   pattern set: `.record(`, `.insert(`, `.update(`, `.delete(`,
   `.persist(`, `.commit(`, `.write(`, `.write_all(`, `.push(`,
   `.store(`, `.save(`, `.put(`, `.set(`, `.execute(`, `.send(`,
   `.publish(`. Pure reads (`.fetch_one`, `.fetch_optional`,
   `.fetch_all`, `.bind`, `sqlx::query`) excluded.
3. 78 raw flags across 20 files. Each manually verified by reading
   the surrounding ~40 lines.

## §3 Findings

| File | Line | Status | Verdict |
|---|---|---|---|
| `corelink-billing/src/quota/cas/cas.rs:258` | emit `CasCheckPassed` | emit → mutate (metrics) | **CORRECT** — `record_check` is metrics observability, not state. Real state mutation `state.try_commit_delta` is correctly preceded by emit at line 359. |
| `corelink-billing/src/quota/cas/cas.rs:319, 330, 359, 379, 414` | denial / pass / commit / race arms | emit → mutate | **CORRECT** — same pattern; explicit inline comments cite fail-closed envelope. |
| `corelink-billing/src/quota/core/check.rs:360, 397, 424` | read / deny429 / reserve arms | emit → mutate | **CORRECT** — `reservations.insert` (line 433) is correctly preceded by emit at 424. |
| `corelink-billing-emit/src/emitter.rs:238, 259, 272, 286` | UsageEmitted / SinkFailure / DuplicateRejected / IdempotencyCollision | mutate (`idempotency.insert`) → emit | **ESCALATE** — the audit variant depends on the `idempotency.insert` return value (Accepted / DuplicateRejected / Collision); restructuring requires deeper design. |
| `corelink-byok/src/byok_*/real.rs` (33 callsites across aws/azure/gcp/vault) | wrap_dek / unwrap_dek / check_access | `.send().await` (external KMS call) → emit | **CORRECT** — `.send()` is external HTTP/SDK call to KMS provider, not OUR state. Audits report outcome of external call (canonical observability of external API). |
| `corelink-dsr-statuspage-scheduler/src/scheduler.rs:284, 305, 342` | failure / publish / publish-failure | flagged mutate at 263 (`cron_log.record`) | **CORRECT** — `cron_log.record` at line 263 is inside the empty-window short-circuit branch that returns before reaching 284/305/342. Flow-insensitive false positive. |
| `corelink-dsr/src/endpoint.rs:406` | `ReceiptIssued` emit | `store.insert(ticket)` → emit | **LOW-RISK FIXED INLINE** — receipt was already issued at line 392 (pure value computation); audit moved to BEFORE store.insert with INV citation. |
| `corelink-enterprise-inquiry/src/ledger.rs:393, 420` | AtomicOk / AutoReplySent | flagged `g.inquiries.insert` at 310 | **CORRECT** — the line-310 insert was audited at line 287 (`Received` event). Each subsequent mutation block has its own preceding audit. AtomicOk follows two external saga legs (Slack + CRM); AutoReplySent precedes local replied_ms mutation at line 428. |
| `corelink-gc/src/mark.rs:924` | `PhaseTransitioned` (Mark) | `candidates.insert_candidate` (loop, line 887) + `runs.checkpoint` (line 909) → emit | **ESCALATE** — audit relies on aggregate state (candidates_count, reachable_count, duration_ms) computed FROM the mutations. Restructuring requires splitting into phase-start + phase-end audits. |
| `corelink-gc/src/physical_delete.rs:938` | `PhysicalDeleted` | `r2.delete` (line 930) → emit | **ESCALATE** — intentional design per inline comment ("R2 DeleteObject FIRST (Lote 10.6bis P0-2 ordering)"). The R2 delete-first pattern with D1 batch rollback is a documented architectural decision; charter clarification needed on whether external R2 mutation counts as L2.6 "state". |
| `corelink-audit-chain/src/neon_shadow/real.rs:605, 621, 650, 666, 685` | shadow sync failure / success | SQL `executor.execute(...)` → emit | **ESCALATE** — txn-bracketed SQL writes (BEGIN → SET → INSERT → COMMIT) cannot logically have failure audits emit BEFORE the call they observe a failure of. Success-path emit at 685 IS after COMMIT; reordering requires moving emit_audit (itself a SQL INSERT) inside the same txn, changing commit semantics. |
| `corelink-byok/src/byok_aws/real.rs:303, 313, 327, 498, 556, 566, 611, 626, 629, 632, 635` | KMS error observability | `.send()` → emit | **CORRECT** — already covered under byok_* group; external KMS observability pattern. |
| `corelink-ops/src/drata/runner.rs:166, 190, 205` | Skipped / Sent / Failed | mixed | **CORRECT** — already fixed in `specs/_audits/2026-05-27-drata-fail-closed-fix.md` (commit `4d4fb8f6`). Heuristic-flagged `drata.push` is external API call; ledger.record (the only OWN state mutation) is correctly preceded by audit emit. |
| `corelink-privacy-erasure-worker/src/orchestrator.rs:439, 454` | VerificationPassed/Failed / Completed | flagged `completions.push(...)` | **CORRECT** — `completions` is a local `Vec<BackendCompletion>` accumulator (not persisted state). Real state mutation is the `dsr_tickets.status` flip downstream of the decision. Audit correctly precedes that. |
| `corelink-replica-worker/src/replication.rs:195, 222` | ReplicationCompleted / sli-fail | `r2.insert` → emit | **CORRECT** — Started+Completed pattern. `ReplicationStarted` emit at line 153 IS before `r2.insert` and carries the canonical intent. `ReplicationCompleted` semantically requires completion to have occurred (Started covers the fail-closed envelope; mirrors drata Sent arm post-fix). |
| `corelink-worker/src/reapi/ac/handler/methods.rs:408, 430` | UpdateResultMismatch / UpdateOk | `envelope_store.put` (line 374) + `meta.upsert` (line 389) → emit | **ESCALATE** — intentional design per inline comment "Step [7] — envelope put (R2-first per WI §9.5; orphan recoverable via S-06 reconcile)". The R2-first / D1-upsert sequence with reconcile backstop is the documented WI §9.5 architecture; charter clarification needed. |
| `corelink-slack-real/src/http.rs:151, 178` | Sent / Failed | `http.post(...).send()` → emit | **CORRECT** — external Slack webhook HTTP call; audit reports outcome of external send. Same external-API observability pattern. |
| `corelink-statuspage-real/src/http.rs:217, 239, 255` | Published / AuthFailed / Failed | `http.post(...).send()` → emit | **CORRECT** — external Statuspage HTTP call; audit reports outcome of external send. |
| `corelink-statuspage-real/src/memory.rs:142` | Published | `g.push(recorded)` → emit | **LOW-RISK FIXED INLINE** — the in-memory backend's `recorded: Vec<RecordedPublish>` IS the canonical state mutation of this fake. Audit moved to BEFORE the push with INV citation. |

**Totals:**

| Category | Count |
|---|---|
| Total callsites scanned | 399 |
| Files containing emits | 84 |
| Heuristic-flagged suspects | 78 (across 20 files) |
| Already-correct (false positives after manual review) | 70 |
| LOW-RISK inline fixes applied | **2** |
| HIGH-RISK escalations | **5** |
| Already fixed (drata) | 1 (out of scope; cited as reference) |

## §4 Inline fixes applied

### Fix 1 — `crates/corelink-dsr/src/endpoint.rs`

`DsrAuditEventType::ReceiptIssued` emit was firing AFTER
`self.store.insert(ticket)?`. Receipt was already issued at line 392
(pure value computation upstream), so the audit captures canonical
intent and is safely movable. Reordered: emit now precedes
`store.insert`. Inline comment added citing
INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER and the failure-mode trade-off
(if `store.insert` fails after `audit.emit`, audit log carries the
receipt-issued intent and downstream reconciliation handles the
ticket-side miss). Mirrors the drata `runner.rs:190` pattern.

### Fix 2 — `crates/corelink-statuspage-real/src/memory.rs`

`StatuspageAuditOutcome::Published` emit was firing AFTER
`g.push(recorded)` (the in-memory backend's canonical state
mutation). Audit event payload does not depend on `recorded`; uses
report fields only. Reordered: emit now precedes the recorded push.
Inline comment added citing INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER and
the fail-closed semantics (if `audit.emit` fails, the recorded slot
stays empty and the caller observes the audit error first).

## §5 Escalations (HIGH-RISK; orchestrator decision required)

### Escalation 1 — `crates/corelink-billing-emit/src/emitter.rs:238, 259, 272, 286`

The four `audit.emit` calls inside the `match self.idempotency.insert(...)`
expression require the insert's return value to discriminate the
audit event variant (Accepted vs DuplicateRejected vs
IdempotencyCollision). Reordering would require:
- pre-computing the audit variant (impossible without the insert's
  decision), OR
- splitting the call into two phases (`peek` then `commit`), which
  semantically changes the idempotency contract.

The inline comment at line 230 acknowledges "audit emit BEFORE R2
mutation" for the success arm but the idempotency.insert (line 228)
already happened. **Charter clarification needed** on whether
idempotency-tracker insert qualifies as "state mutation" in the L2.6
sense or as an "intent commit" that the audit is observing.

### Escalation 2 — `crates/corelink-gc/src/mark.rs:924`

`GcEventType::PhaseTransitioned` emit fires AFTER:
- `candidates.insert_candidate(...)` (loop body line 887, N iterations)
- `runs.checkpoint(...)` (line 909)
- `metrics.record_phase_duration_ms(...)` (line 921)

The audit payload depends on aggregates computed FROM those
mutations (`reachable_count`, `candidates_count`, `duration_ms`,
`phase_end_now`). Splitting into a `PhaseStarted` (before
mutations) + `PhaseCompleted` (after) audit pair is the canonical
mitigation but requires GC spec amendment + downstream consumer
update.

### Escalation 3 — `crates/corelink-gc/src/physical_delete.rs:938`

Audit fires AFTER `r2.delete(...)`. Per inline comment (lines 925-
927): "R2 DeleteObject FIRST (Lote 10.6bis P0-2 ordering;
PAT-RETRY-IDEMPOTENT-001 semantics — both `Deleted` and `NotFound`
are successes)". The doc comment at line 933 explicitly says
"Production wiring atomically rolls back the D1 batch (DELETE
blob_meta + DELETE gc_candidate + INSERT audit_outbox) on emit
failure" — i.e. the fail-closed envelope is the D1 transaction, NOT
audit-precedes-mutation.

**Charter clarification needed** on whether external R2
DeleteObject counts as "state" in the L2.6 sense or as an external
side-effect with reconcile-as-backstop.

### Escalation 4 — `crates/corelink-worker/src/reapi/ac/handler/methods.rs:408, 430`

Same R2-first pattern. Inline comment line 369: "Step [7] —
envelope put (R2-first per WI §9.5; orphan recoverable via S-06
reconcile)". Audit emit at line 430 fires after the envelope.put +
persist_action_result + meta.upsert + neg_cache.invalidate chain.

The `UpdateResultMismatch` audit at line 408 specifically observes
the upsert's `ResultHashMismatch` return variant; cannot pre-emit
without the decision (same shape as Escalation 1).

**Charter clarification needed** on R2-first per WI §9.5.

### Escalation 5 — `crates/corelink-audit-chain/src/neon_shadow/real.rs:605, 621, 650, 666, 685`

Shadow-sync function bracketed by SQL `BEGIN → SET (RLS GUC) →
INSERT (per row) → COMMIT`. Failure-arm audits at 605/621/650/666
fire after the `executor.execute(...)` call they observe a failure
of (cannot logically precede). Success-arm audit at 685 fires after
COMMIT. Reordering the success emit to inside the txn requires
moving `emit_audit` (itself a SQL INSERT into a `shadow_audit` table
per the trait impl) into the same transaction, changing commit
semantics for the shadow sync.

**Charter clarification needed** — this is the canonical
"txn-bracketed audit" pattern that L2.6 may or may not contemplate.

## §6 Verification

Acceptance gates run on the two crates touched by inline fixes:

- `cargo build -p corelink-dsr` → **GREEN** (23.03s)
- `cargo build -p corelink-statuspage-real` → **GREEN** (1m 17s)
- `cargo clippy -p corelink-dsr -p corelink-statuspage-real --tests -- -D warnings` → **GREEN** (1m 17s; no warnings)
- `cargo test -p corelink-dsr -p corelink-statuspage-real` → **GREEN** (152 tests passing: 94 + 19 + 29 + 4 + 4 + 2; 0 failures, 0 ignored)
- `cargo test --workspace --no-run` → **GREEN** (background task `b8e5nyzrp` exit 0; all test binaries built)
- Merge-conflict marker grep → **CLEAN** (no `<<<<<<<` / `>>>>>>>` in `crates/`)

## §7 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of audit ordering sweep SEAL.**
