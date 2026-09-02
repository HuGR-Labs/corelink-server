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
