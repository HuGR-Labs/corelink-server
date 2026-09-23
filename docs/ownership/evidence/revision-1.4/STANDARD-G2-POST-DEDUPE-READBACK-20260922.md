# Standard G2 post-dedup readback — 2026-09-22

The R3 cold-review note predates the consolidated deduplication registry. A
fresh readback confirms that the former 23 direct-hit rows and the remaining
no-hit rows now have explicit decisions in
`DEDUPE-DECISION-REGISTRY.json`: 105/105 packages, 92 `DISTINCT`, 4 `REUSE`,
9 `EXPAND`, 0 unresolved.

This resolves the specific “23 hits without decision” bookkeeping gap in G2,
but does not freeze the standard or pass publication. The per-item gates remain
`PENDING`, the immutable contract remains unfrozen, the campaign artifacts are
not integrated into observed `main`, and independent pilot/authority review is
still incomplete. The historical R3 verdict therefore remains
`BLOCKED_BEFORE_FREEZE`, with G2 narrowed rather than promoted to approval.
