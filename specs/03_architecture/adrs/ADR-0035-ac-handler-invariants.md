---
id: "ADR-0035"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
title: "AC Handler Invariants (REAPI v2 GetActionResult / UpdateActionResult Compliance)"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
deciders: ["Gustavo Schneiter (Owner)", "Architect (TBD)"]
context_links:
  - "specs/04_sprints/_sealed/S04/work_items/WI-S04-001-reapi-actioncache-handlers.md"
tags: ["adr", "s04", "ac", "handlers", "reapi", "invariants"]
---

# ADR-0035 — AC Handler Invariants (REAPI v2 Compliance)

## Context

S-04 introduces handlers for REAPI v2 ActionCache: `GetActionResult` (read) + `UpdateActionResult` (write). Spec must lock down invariants that handlers MUST preserve regardless of optimization (caching, batching, async).

## Decision

The following 8 invariants are CANONICAL for AC handlers. Validation via property tests (10k iter PR; 100k nightly) + chaos suite (≥10) + REAPI v2 conformance test set (10 enumerated tests minimum; see WI-S04-006 §6.1.1).

### Handler Invariant H-1: GET handler MUST validate signature on every read

`verify_full(envelope, sig_verifier)` runs before envelope returned to client. NEVER serve unverified bytes. Any sig failure → 422 + `COR_AC_SIG_INVALID` + audit emit + negative cache write (60s TTL).

### Handler Invariant H-2: UPDATE handler MUST validate signature pre-INSERT

UpdateActionResult writes to D1 + R2 only after `verify_full` passes. Reject untrusted writes before persistence.

### Handler Invariant H-3: tenant_prefix from D1 (NOT recomputed from TDK at GET time)

`GET` reads `tenant_prefix` from `ac_meta` materialized BLOB(16) column. Path reconstruction via `r2::get(ac-<region>/<tenant_prefix>/<action_digest>.json)` uses the persisted prefix. **DOES NOT** recompute from TDK at GET time. Trust boundary: TDK access required only at INSERT time (UPDATE handler) and at sig verify time (GET + UPDATE handlers).

### Handler Invariant H-4: TenantCtx-only enforcement

Handlers MUST extract tenant_id from `TenantCtx` (Tower middleware S-03 WI-S03-003) — NEVER from request body, query params, or headers. Multi-tenant injection attacks blocked at middleware.

### Handler Invariant H-5: Audit fail-closed atomicity

D1 batch (`INSERT ac_meta + INSERT audit_outbox`) is atomic. If audit_outbox INSERT fails, ac_meta INSERT rolls back. Negative cache invalidation (KV.delete) runs AFTER batch commits (post-step-8). Audit miss is impossible under successful handler completion.

### Handler Invariant H-6: Negative cache window 60s (KV TTL)

After `UpdateActionResult` success, prior `GetActionResult` 404 entries are invalidated via KV.delete; KV native TTL caps neg-cache lifetime at 60s. `INV-NEG-CACHE-MONOTONIC` (registry §3.6).

### Handler Invariant H-7: Output reachability check (1% sampled)

`outputs_check::warn_if_missing` runs on 1% of GETs (sampled; Lote 10.4bis P0 fix); validates that `output_files[*].digest` exist in `blob_meta`. Drift → SEV-3 informational (deferred to S-06 reconcile diário); does NOT fail the GET.

### Handler Invariant H-8: Idempotent UpdateActionResult

Same `(tenant_id, action_digest, result_hash, sig)` tuple on retry → idempotent ON CONFLICT (Lote 10.4bis WI-S04-002 schema fix); no duplicate audit emit; client receives 200 OK with existing row.

## Consequences

### Positive
- Handlers are testable: each invariant maps to a property test + chaos scenario.
- REAPI v2 conformance gate verifies subset of these via 10 enumerated tests (WI-S04-006 §6.1.1).
- Mass refactor (e.g., switching from sqlx prepared to a different driver) preserves behavior if invariants pass.

### Negative
- H-7 sampling rate (1%) is arbitrary; documented in WI-S04-001 §1; revisit at S-06 reconcile lessons.

### Neutral
- 8 invariants is moderate; matches REAPI v2 spec surface for ActionCache.

## References

- bazelbuild/remote-apis (REAPI v2 spec).
- WI-S04-001 (handler implementation).
- WI-S04-006 (PRR ship gate; conformance verification).

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.4-tris P1-R5-016 ADR-creation) | ADR file created; 8 invariants enumerated; cross-ref to WI-S04-001/006 + invariant_registry.md §3.15. |
