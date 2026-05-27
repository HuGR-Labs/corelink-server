---
id: "AUDIT-2026-05-01-S04-WIP-FINDINGS"
type: "audit_report"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.1.0"
created: "2026-05-01"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "s04", "wip", "findings", "sprint-close"]
---

# S-04 in-flight findings (carry-over to sprint-close Sonnet review)

> **Sprint:** S-04 (Action Cache + Merkle + HKDF signing) · **Date:** 2026-05-01
> **Origin:** WI-S04-002 SEAL agent (id `a8d4d509e4d958468`) parallel-execution observation
> **Disposition:** Logged for Sonnet sprint-close adversarial review at S-04 close; not blocking subsequent WI SEALs.

---

## F-001 — `ACTION_RESULT_STASH` global state in WI-S04-001 handler

**Origin:** Commit `eca3c9c` (WI-S04-001).

**Location:** `crates/corelink-worker/src/reapi/ac/handler.rs:~1181`.

**Observation:**
`static ACTION_RESULT_STASH: LazyLock<Mutex<HashMap<String, ActionResult>>>` is a process-global sibling store keyed by `(region, tenant_prefix, action_digest)`. `persist_action_result` writes to the stash *before* `meta.upsert` runs. When `meta.upsert` rejects an `UpdateActionResult` with `ResultHashMismatch` (e.g. caller mutates `ActionResult` proto for an existing `(tenant, action_digest)`), the rejected proto is **already** in the global stash. Subsequent in-process operations (or tests) reusing the same key get back the stale-rejected bytes.

**Impact:**
- Test flakiness when integration tests share a key across test cases (observed during WI-S04-002 parallel-test run: `gherkin_get_action_result_hit_warm` + `update_then_get_happy_path`).
- Production-relevance is bounded because the production handler instance is single-tenant per request and the canonical metadata path is D1, not the stash. The stash is a host-side simulator artefact for WI-S04-001's `InMemoryAcEnvelopeStore`.
- Architectural smell: any process-global mutable state in `corelink-worker` violates the workspace's per-handler-instance discipline.

**Suggested remediation (sprint-close):**
1. Reorder `persist_action_result` so the stash write only happens *after* `meta.upsert` returns Ok.
2. Better: scope the stash to the handler instance (drop `LazyLock`; carry an `Arc<Mutex<HashMap>>` field on `ActionCacheHandlerImpl`). Eliminates the test-leak without re-architecting.
3. Best (sprint-close decision needed): drop the in-memory stash entirely; have `InMemoryAcEnvelopeStore` carry the `ActionResult` bytes directly so the simulator is actually self-contained.

**Owner:** Sonnet sprint-close adversarial reviewer to triage. If deemed P1, orchestrator opens a follow-up commit before sprint SEAL. If deemed P2, ship S-04 with the smell documented and revalidate at S-19 onboarding.

**Status:** ✅ **CLOSED — fixed in-flight at WI-S04-005 SEAL** (2026-05-01).

**Resolution:** Closure path #2 from this doc adopted (instance-scope). The process-global `static ACTION_RESULT_STASH: LazyLock<Mutex<HashMap>>` was replaced with a per-instance `action_result_stash: Arc<tokio::sync::Mutex<HashMap<String, ActionResult>>>` field on `ActionCacheHandlerImpl`. `persist_action_result` and `recover_action_result` now route through `&self.action_result_stash`, which is initialised per-handler in `ActionCacheHandlerImpl::new()`. The trailing crate-level `static` declaration + the `ActionResultStash` impl block (other than `canonical()` key generator, retained) were deleted. Tenant isolation by construction is preserved: the canonical key (`region/prefix/hex`) was already tenant-leftmost.

**Verification:**
- `cargo test -p corelink-worker --features tower-middleware --lib` (parallel default `--test-threads`) → 188 passed / 0 failed (was: 1 failed under contention, 0 failed serialised).
- `cargo test -p corelink-worker --features tower-middleware --tests` → 298 passed / 0 failed.
- `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` → clean.

---

**Reporter:** WI-S04-002 implementation agent (id `a8d4d509e4d958468`).
**Status when observed:** WI-S04-002 was completing in parallel with the orchestrator's WI-S04-002 SEAL at commit `08bc549`. Both agents converged on the same canonical artefacts; the finding above is incidental to the S-04 work but not in WI-S04-002 scope.

**Fim S-04 WIP findings v1.0.0.**
