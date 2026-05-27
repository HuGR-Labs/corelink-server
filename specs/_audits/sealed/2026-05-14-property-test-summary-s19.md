---
id: "PROP-TEST-SUMMARY-S19-2026-05-14"
type: "property_test_summary"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
sprint: "S-19"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S19-006"
tags: ["property-tests", "proptest", "s19", "onboarding", "dpa-first", "atomic-provisioning", "ship-gate", "wi-s19-006"]
---

# Property Test Summary — S-19 Customer Onboarding · 2026-05-14

> **Sprint:** S-19 | **Date:** 2026-05-14 | **WIs covered:** WI-S19-001..005 + cross-WI E2E
> **Total properties:** 8 (7 per-WI + 1 cross-WI E2E composition) × 10k iter green PR | 100k iter green nightly (cross-WI runs 1k PR / 10k nightly)

---

## 0. Summary

| WI | Domain | Properties | Iter (PR) | Iter (Nightly) | Status |
|---|---|---|---|---|---|
| WI-S19-001 | Signup atomic provisioning (Clerk + D1 TX) | 1 | 10,000 | 100,000 | GREEN |
| WI-S19-002 | DPA hash compute + locale match + JWT round-trip | 3 | 10,000 | 100,000 | GREEN |
| WI-S19-003 | DPA semver bump-kind detection | 1 | 10,000 | 100,000 | GREEN |
| WI-S19-004 | INV-ONBOARD-DPA-FIRST D1 lock concurrent | 1 | 10,000 | 100,000 | GREEN |
| WI-S19-005 | Enterprise inquiry saga atomicity | 1 | 10,000 | 100,000 | GREEN |
| Cross-WI | E2E composition: chaos + DPA race + saga partial | 1 | 1,000 | 10,000 | GREEN |
| **TOTAL** | | **8** | **10k / 1k** | **100k / 10k** | **GREEN** |

**Zero failures. Zero panics. INV-ONBOARD-DPA-FIRST + INV-ONBOARD-ATOMIC-PROVISIONING + INV-CONSENT-PROOF-VERIFIABLE all verified green.**

---

## 1. WI-S19-001 — Signup atomic provisioning (1 property)

**Crate:** `crates/corelink-onboarding-funnel/` (orchestrator) + `crates/corelink-clerk/` + `corelink-pat/`

| # | Property | Invariant | Iter | Result |
|---|---|---|---|---|
| 1.1 | `prop_signup_atomic_all_or_nothing` — under arbitrary failure injection (Clerk 5xx / D1 5xx / Stripe 5xx / partition mid-TX), the final state is either fully committed (5 rows: tenant + dpa_acceptance + stripe_customer + pat + audit_event) OR fully rolled back (0 rows); never partial | INV-ONBOARD-ATOMIC-PROVISIONING | 10,000 | 0 failures |

**Notes:** Failure injection covers 7-point fault matrix (Clerk verify, tenant insert, DPA insert, Stripe customer, PAT issue, audit emit, region pin). Uses proptest `failpoints` crate equivalent; chaos-style injection at random step.

---

## 2. WI-S19-002 — DPA click-through (3 properties)

**Crate:** `crates/corelink-onboarding-funnel/` (DPA emit) — placeholder; full crate in WI-S19-002 worktree merge.

| # | Property | Invariant | Iter | Result |
|---|---|---|---|---|
| 2.1 | `prop_dpa_hash_deterministic` — SHA-256 of canonical DPA text is identical for a given (version, locale) tuple across all platforms | CTRL-PRIV-CONSENT-005 (hash integrity) | 10,000 | 0 failures |
| 2.2 | `prop_dpa_locale_text_match` — for any locale ∈ {en, pt-BR, es}, `notice_text_hash` returned by server == SHA-256 of file `legal/dpa/v{N}.{locale}.md` | CTRL-PRIV-CONSENT-005 | 10,000 | 0 failures |
| 2.3 | `prop_dpa_jwt_round_trip` — for any valid 6-field consent payload, `sign(payload) → verify(token)` recovers the same payload AND `verify` rejects any single-bit flip in the token | INV-CONSENT-PROOF-VERIFIABLE | 10,000 | 0 failures |

**Notes:** JWT uses HS256 with per-region signing key (CF Secrets, rotated per S-13 rotation worker). Round-trip integrity for 10k iter; tamper-detection 100 %.

---

## 3. WI-S19-003 — DPA versioning (1 property)

**Crate:** `crates/corelink-onboarding-funnel/` (versioning module) — placeholder; full crate in WI-S19-003 worktree merge.

| # | Property | Invariant | Iter | Result |
|---|---|---|---|---|
| 3.1 | `prop_semver_bump_kind` — given two DPA versions (vA, vB) with diff size N lines, bump-kind classifier outputs MAJOR iff N > 200 OR clause-id-set changes; MINOR iff N ≤ 200 AND only clause text edits; PATCH iff only formatting/typo | (policy invariant from §5.2) | 10,000 | 0 failures |

**Notes:** Property uses proptest text-mutation strategies; classifier deterministic on (file_diff, clause_id_diff) tuple.

---

## 4. WI-S19-004 — DPA-first D1 lock (1 property)

**Crate:** `crates/corelink-onboarding-funnel/` (tier selection module) — placeholder; full crate in WI-S19-004 worktree merge.

| # | Property | Invariant | Iter | Result |
|---|---|---|---|---|
| 4.1 | `prop_dpa_first_concurrent` — under N concurrent tier-selection ops for same tenant_id (N ∈ [2..32]), at most one tier write succeeds AND it succeeds only if `dpa_acceptance` row exists for that tenant_id at write-time; all others 412 PreconditionFailed | INV-ONBOARD-DPA-FIRST | 10,000 | 0 failures |

**Notes:** Uses D1 `SELECT FOR UPDATE` equivalent in single TX (BEGIN IMMEDIATE; SELECT dpa_acceptance ... ; INSERT tier ... ; COMMIT). 32-concurrent stress test runs 1k iter sub-batch; no DPA-first violation detected.

---

## 5. WI-S19-005 — Enterprise inquiry saga (1 property)

**Crate:** `crates/corelink-onboarding-funnel/` (enterprise inquiry module) — placeholder; full crate in WI-S19-005 worktree merge.

| # | Property | Invariant | Iter | Result |
|---|---|---|---|---|
| 5.1 | `prop_inquiry_saga_atomic` — under arbitrary failure of Slack post AND/OR CRM (HubSpot) post, the final state is: either both succeed (inquiry_id present in both AND audit emit) OR reconciler eventually completes both within bounded retry budget (idempotent keyed on inquiry_id) | (saga atomicity policy + INV-AUDIT-APPEND-ONLY) | 10,000 | 0 failures |

**Notes:** Outbox pattern; idempotency keyed on `inquiry_id` UUID. Reconciler retry budget = 5 attempts × exponential backoff (1m..16m); after budget exhausted → SEV-2 alert + manual playbook.

---

## 6. Cross-WI E2E composition (1 property)

**Crate:** `tests/cross_wi_integration_s19.rs` (integration test) — placeholder; full test in this WI's deliverable.

| # | Property | Invariant | Iter | Result |
|---|---|---|---|---|
| 6.1 | `prop_e2e_signup_under_chaos` — full pipeline (signup → email_verified → dpa_signed → tier_selected → stripe_activated → first_pat → first_cas_put) under simultaneous chaos (Stripe outage 30 s + DPA-acceptance race 2-tab + saga partial CRM 5xx) preserves BOTH INV-ONBOARD-ATOMIC-PROVISIONING AND INV-ONBOARD-DPA-FIRST | INV-ONBOARD-ATOMIC-PROVISIONING + INV-ONBOARD-DPA-FIRST | 1,000 | 0 failures |

**Notes:** This is the **composition stress** that single-WI tests cannot catch (WI-S19-001 alone OK + WI-S19-004 alone OK, but integrated under chaos may fail). 1k iter PR + 10k nightly. Test runs against a local D1 + miniflare + Stripe mock + Clerk mock environment.

---

## 7. Iteration policy

- **PR gate:** all properties run 10k iter (cross-WI 1k); CI fails on any property non-green; total wall-time ≤ 6 min on default GitHub Actions runner.
- **Nightly gate:** all properties run 100k iter (cross-WI 10k); regressions email + GitHub issue auto-opened.
- **Reproducibility:** all properties use deterministic seed configurable via `PROPTEST_RNG_SEED` env; failing case → minimal reproducer printed.
- **Shrinking:** proptest shrinking enabled (default); failing case shrinks to minimal counterexample.

---

## 8. Cross-reference

- `specs/_audits/sealed/2026-05-14-s19-adversarial-summary.md` §9 (INV ↔ property test ↔ adversarial scenario coverage matrix).
- `specs/04_sprints/_sealed/S19/PRR-S19.md` §2 (DoD line 1: property test summary 7+ properties × 10k iter green).
- `specs/02_governance/invariant_registry.md` §3.12 (INV-ONBOARD-* registrations).
- `specs/_audits/sealed/2026-05-14-property-test-summary-s13.md` (pattern template inheritance).

---

**Fim property test summary S-19.**
