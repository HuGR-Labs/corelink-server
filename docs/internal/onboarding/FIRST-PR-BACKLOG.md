# First PR Backlog (curated, refreshed quarterly)

> **Curator:** Engineering Manager (rotates).
> **Last refresh:** 2026-05-15.
> **Next refresh:** 2026-08-15.
> **Rule:** every entry is XS-sized (≤ 4 hours from `git checkout` to
> PR opened, exclusive of CI). Each entry teaches exactly one thing.
> Items are deleted when shipped — if you grab one, mark it
> `[CLAIMED by <handle>, <date>]` in a PR before starting so two new
> hires don't collide.

Recommended pairing: pick one from the same domain you chose for
Week 2 if possible — the learning compounds. If your domain has zero
free items, grab a "general" item.

## How to use this list

1. Browse with your buddy.
2. Pick one. Claim it with a 1-line PR to this file.
3. Read the linked files **first** (the "Touches" column tells you
   where).
4. Open the PR, mention the backlog item by row number.
5. Once merged, open a follow-up 1-line PR removing the row.

## General (works for any domain background)

| # | Title | Effort | Touches | What you learn |
|---|---|---|---|---|
| 1 | Fix typo / wording in `slo_catalog.md` for any SLO description | XS (15 min) | `specs/03_architecture/slo_catalog.md` | Frontmatter, validate_specs, PR mechanics |
| 2 | Add a missing rustdoc summary to one public fn in `corelink-rate-headers::HeaderBag` | XS (30 min) | `crates/corelink-rate-headers/src/lib.rs` | clippy `missing_docs`, rustdoc conventions |
| 3 | Add a property test for `validate_specs.py` edge case (e.g. empty frontmatter list value) | XS (90 min) | `scripts/validate_specs.py`, `tests/spec_validator/` | Python tests, frontmatter schema |
| 4 | Add `#[non_exhaustive]` to any public enum that's missing it (run charter audit script to find candidates) | XS (45 min) | varies; CI charter audit reports | Charter constraints, non-breaking-change pattern |
| 5 | Backfill a missing `tracing::instrument` on one public async fn in `corelink-cli` | XS (30 min) | `crates/corelink-cli/src/` | Tracing conventions, observability model |
| 6 | Update `README.md` "Estrutura" tree to match current `crates/` layout | XS (60 min) | `README.md` | Repo geography, doc hygiene |

## BYOK

| # | Title | Effort | Touches | What you learn |
|---|---|---|---|---|
| 7 | Add a unit test that fails if DEK-cache TTL const is raised above 5 min | XS (60 min) | `crates/corelink-byok/src/cache.rs` (likely) | DEK invariant enforcement |
| 8 | Add a CLI flag `--print-provider` to `corelink-byok` (smoke) that prints the configured provider name | XS (90 min) | `crates/corelink-byok/src/lib.rs`, CLI tests | Trait + impl wiring, fakes vs real |
| 9 | Backfill rustdoc on `corelink-erasure-attestation` public API | XS (45 min) | `crates/corelink-erasure-attestation/src/lib.rs` | Cryptography doc conventions |

## Audit chain

| # | Title | Effort | Touches | What you learn |
|---|---|---|---|---|
| 10 | Add a property test confirming JCS canonicalization is idempotent (`jcs(jcs(x)) == jcs(x)`) | XS (60 min) | `crates/corelink-jcs/tests/` | JCS / RFC 8785, proptest patterns |
| 11 | Add an assertion that audit-emit precedes user-visible return in one specific handler that's currently unchecked | XS (90 min) | one handler in `crates/corelink-admin-api/` or `crates/corelink-billing-stripe/` | Audit-fail-CLOSED ordering |
| 12 | Add a CLI subcommand `corelink-client-verify head --date YYYY-MM-DD` to fetch + print today's published head | XS (2h) | `crates/corelink-client-verify/src/cli.rs` | Public verifier API, customer perspective |

## Billing

| # | Title | Effort | Touches | What you learn |
|---|---|---|---|---|
| 13 | Add a newtype `MeterRecordCount(u64)` if currently raw `u64` in aggregator | XS (60 min) | `crates/corelink-billing-aggregator/src/` | Newtype patterns, type-safety in money paths |
| 14 | Backfill missing rustdoc on `corelink-billing-reconcile::drift_threshold_bps` | XS (15 min) | `crates/corelink-billing-reconcile/src/lib.rs` | Reconcile semantics |
| 15 | Add a test for idempotency-key derivation collision (same input twice → same key) | XS (90 min) | `crates/corelink-billing-emit/tests/` | Idempotency invariant |

## Privacy

| # | Title | Effort | Touches | What you learn |
|---|---|---|---|---|
| 16 | Add the four `Region` variants to a debug `Display` impl that's currently `Debug`-only | XS (30 min) | `crates/corelink-residency-policy/` (or wherever `Region` lives) | Residency enum surface |
| 17 | Add a test that constructs `Region::WEUR` and asserts it does not equal `Region::WNAM` (trivial — but verifies enum is `PartialEq`) | XS (15 min) | residency test module | Residency invariant baseline |
| 18 | Update `RB-DPA-CHANGE.md` step ordering if the script paths reference an older `scripts/` path | XS (45 min) | `specs/_runbooks/RB-DPA-CHANGE.md` | Runbook hygiene, DPA pipeline |

## Ops

| # | Title | Effort | Touches | What you learn |
|---|---|---|---|---|
| 19 | Add a missing on-call contact to `ONCALL-ESCALATION-MATRIX.md` for one tier | XS (15 min) | `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` | Escalation mechanics |
| 20 | Add a chaos catalog entry for a small new fault (e.g. R2 read latency injection capped at 200ms) | XS (90 min) | `specs/_runbooks/RB-CHAOS-CATALOG.md`, possibly `corelink-chaos-scheduler` config | Chaos bounding, catalog format |
| 21 | Convert one `eprintln!` in a runbook helper script under `scripts/` to structured logging | XS (45 min) | one script under `scripts/` | Log conventions for ops tooling |

---

## Refresh policy

This list is curated by the Engineering Manager. Quarterly:

1. Delete items that have shipped (search closed PRs referencing the
   row).
2. Add ≥ 5 new items per domain.
3. Verify all "Touches" paths still exist (rename detection).
4. Verify effort estimates against actual time-to-merge from the last
   batch (recalibrate if consistently off by > 2×).
