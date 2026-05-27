---
id: "AUDIT-2026-05-27-DRATA-FAIL-CLOSED-FIX"
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
tags: ["audit", "fail-closed", "drata-sync", "INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER", "seal"]
references:
  - "specs/_audits/2026-05-27-charter-strict-audit-post-w36.md"
---

# Drata runner fail-CLOSED ordering fix SEAL

## §1 Scope

Closes charter audit §L2.6 finding (per `specs/_audits/2026-05-27-charter-strict-audit-post-w36.md`): the `Ok(receipt)` arm of `corelink-ops/src/drata/runner.rs:188-198` called `self.ledger.record(&entry)?` BEFORE `self.audit.emit(&SyncAuditEvent { ... })?`. This sequencing violated **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** — the charter mandates that audit emission precede state mutation so the audit log captures intent even when subsequent mutation fails.

## §2 Fix

Swapped the order of the two operations. Audit `emit()` now precedes `ledger.record()`. Added an inline comment citing the invariant + explaining the trade-off: if `ledger.record` fails after `audit.emit`, the audit log carries the canonical intent and downstream reconciliation handles the ledger-side miss.

The audit event payload is unchanged. Only line ordering swapped. The `Err(e)` arm of the same `match` was inspected and was already correct (audit emit precedes any mutation in that branch).

## §3 Verification

- `cargo build -p corelink-ops` → Finished in 2.55s (cached) — GREEN
- `cargo clippy -p corelink-ops --tests -- -D warnings` → Finished in 2m44s — GREEN
- `cargo test -p corelink-ops --no-run` → all test binaries produced (drata-related: `drata_sync_*`, `runner_*`) — GREEN

## §4 Recovery note

Agent `a9ab070f58c5b9d24` applied the source fix correctly but premature-exited on backgrounded `cargo build` (known pattern per memory `feedback-synchronous-agents`). Orchestrator verified gates via separate background task (`bf1pvngg2`, exit 0) and completed the SEAL audit + commit.

## §5 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of drata fail-CLOSED fix SEAL.**
