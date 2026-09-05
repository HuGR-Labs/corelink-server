# D-2 follow-up — residency is a three-state full-population proof

The audit residency control is now `scripts/verify_audit_residency.py`. It uses
`audit_outbox LEFT JOIN tenant`, not the former inner-join-only predicate, and
reports every audit row as exactly one of `satisfied`, `violated`, or
`unevaluable`.

The result is compliant only when the full non-empty population is satisfied.
Known cross-region mismatches and rows that cannot be proved are both failures.
Missing credentials, timeout, malformed D1 output, an empty population, a
broken partition, or an empty production DSR-erasure control are
`INDETERMINATE`, not a clean zero.

The output retains the DSR-erasure, unexplained-orphan, and `weur` controls:
the DSR-retained evidence must stay retained, and neither it nor any unexplained
or `weur` orphan may leave the denominator. See
`scripts/RB-AUDIT-RESIDENCY-THIRD-STATE.md` for the production procedure.

## Writer-side closure and API impact

Migration `0107_audit_outbox_tenant_residency_guard.sql` adds a second,
additive `BEFORE INSERT` trigger. For every customer-attributed row, the tenant
must exist and its `primary_region` must equal `audit_outbox.region`; a missing
tenant now aborts the write with `residency_unprovable`. The prior 0023 trigger
remains in place, so this closes SQLite's `!= NULL` gap without rewriting any
historical evidence. The explicit `_public` namespace is pinned to `wnam` and
continues to support public revocation. Deterministic `INSERT OR IGNORE`
retries of already-retained rows remain idempotent after erasure.

The customer-facing HTTP API is unchanged: no route, status, or response shape
was added. A new invalid writer receives the existing fail-closed audit error
path (503 where the caller exposes sink failure); the residency verifier is a
read-only operator control with exit states `COMPLIANT`, `FAILED`, and
`INDETERMINATE`.
