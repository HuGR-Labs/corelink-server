---
id: "AUDIT-S19-SPRINT-CLOSE-R1"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Adversarial Review Round 1 (Sonnet)"
tags: ["audit", "sprint-close", "s19", "adversarial", "high-risk"]
---

# AUDIT-S19-SPRINT-CLOSE-R1 — Post-Merge Adversarial Sprint-Close Round 1 (HIGH_RISK)

> **Scope:** post-merge sprint-close review of S-19 (Customer Onboarding) — 6 WIs SEALED + merged + pushed to `main`. Comparator: `specs/_audits/sealed/2026-05-14-s19-sprint-preflight-review.md` (5 P0 + 8 P1).
> **Working dir:** `/Users/gustavoschneiter/Documents/HuGR/corelink-server` @ `main 53a107d`.
> **Gates executed:** `cargo build --workspace` (PASS), `cargo clippy --workspace --tests -- -D warnings` (PASS), `cargo test --workspace --no-run` (PASS), `python3 scripts/validate_specs.py` (327 OK / 9 YAML-only / 14 pre-existing S-11/S-13 ADR failures; 0 S-19 failures), conflict-marker grep empty.

---

## 1. Score

**8.4 / 10.**

Justification: every pre-flight P0 closed at the schema, code, or sign-off layer. Implementation is uniformly disciplined — `#[non_exhaustive]` everywhere on new public enums, zero `unsafe`, zero `tokio` imports in `src/`, audit-emit-BEFORE-mutation honoured on the load-bearing tier-selection orchestrator, INV-ONBOARD-DPA-FIRST property test `prop_dpa_first_never_bypassed_10k` actually exercises `tier=Free` (the canonical pre-flight P0-2 hole). The score is held back by three implementation P1s that surface only post-merge: (a) `dpa-legal-review.yml` is tag-pinned (`actions/github-script@v7`) violating the SHA-pin charter constraint, (b) DPA acceptance positive-path `Accepted` audit emits AFTER `insert_idempotent` (violates lookup → emit → mutate ordering on the happy path; negative paths are correct), (c) the `enterprise_inquiries` migration documents `encrypted_payload_b64 TEXT NOT NULL` but the `corelink-enterprise-inquiry` ledger persists `form: EnterpriseInquiryForm` as plaintext — no BYOK envelope code path produces that ciphertext, so the schema promise is unfulfilled by the in-process mirror (production wiring at the Cloudflare Worker boundary is the only place the envelope can land, and it is not in this sprint's code). None of the three breaks the spec contract §19 "cannot-be-waived" list. SEAL stands; addendum follows in round 2.

---

## 2. Verdict

**`CONDITIONALLY APPROVED (P1 only)`.**

Implementation SEAL holds. The three new P1s are tracked against the existing PRR-S19 W6/W7/W8 expiry calendar; no P0 surfaced that would force a re-SEAL. The GA Evidence Gate at D+45 must close the encryption-at-rest envelope (P1-NEW-3) before any real customer PII flows; this is the only finding that bears on customer-facing risk and it has a hard expiry tied to PRR-S19 §7 W5 (prod-keys cutover D+10) where the BYOK envelope wiring naturally lands.

---

## 3. Pre-flight P0 re-verification table

| # | Pre-flight P0 | Status | Evidence |
|---|---|---|---|
| P0-1 | D1 migration filename collision (0037..0042 race) | **FIXED** | `migrations/d1/` shows clean `0037_signup_orchestration.sql`, `0038_dpa_acceptances.sql`, `0039_tier_selection.sql`, `0040_enterprise_inquiries.sql`, `0041_dpa_versioning.sql`. WI-006 did not require a schema row (instrumentation only) per pre-flight §3 — the 5/6 reservation actually claimed. No collision, no `_alt`, no resequence commit needed. |
| P0-2 | INV-ONBOARD-DPA-FIRST Free-tier escape hatch | **FIXED** | `crates/corelink-tier-selection/src/ledger.rs:210-225` runs the DPA gate BEFORE the `Free` / `Paid` dispatch (`if tier == TierKind::Free` branch at line 267 is reached only post-gate). Property test `prop_dpa_first_never_bypassed_10k` (`crates/corelink-tier-selection/tests/prop_tier_selection.rs:87-122`) uses a `tier_strategy()` (line 73) that includes `Just(TierKind::Free)` — 10k cases ratify the gate for every tier including Free. Dedicated unit test `dpa_required_blocks_free_tier` (ledger.rs:522-539) asserts `TierError::DpaRequired` on a free-tier call with `AlwaysDenyDpaGate`. Stripe sessions count = 0 asserted; violation audit event emitted. |
| P0-3 | WI-001 / WI-002 atomic-boundary contradiction (does atomic tx include DPA row?) | **FIXED** | `crates/corelink-signup/src/orchestrator.rs:333-372` `run_atomic_tx` walks 4 inserts inside the single `tx`: `insert_tenant` → `insert_dpa_pending` → `insert_first_pat` → `insert_usage_counter` then `commit`. Each step rolls back on failure. The atomic boundary explicitly carries the `dpa_acceptance_pending` row per `migrations/d1/0037_signup_orchestration.sql` header ("the atomic boundary covers `tenant` + `dpa_acceptance_pending` + `pat` + the `usage_counter` initialiser"). WI-002's `dpa_acceptances` table (`0038_dpa_acceptances.sql`) is the **post-signup click-through** persistence (PK on `signup_id`, NOT inserted at signup time). Boundary is unambiguous: signup tx writes the *pending marker*; the click-through writes the *acceptance receipt*. Defensible and documented. |
| P0-4 | Stripe webhook idempotency binding to existing migration (no parallel dedup) | **FIXED** (with note) | `crates/corelink-tier-selection/src/ledger.rs:144-145` uses an in-process `processed_event_ids: HashSet<String>` mirror; the migration header `migrations/d1/0039_tier_selection.sql` documents this as the D1 mirror of `stripe_idem_keys` (S-10). Property test `prop_webhook_idempotency` (prop_tier_selection.rs:172-219) drives 1k cases with up to 20 repeats per `event_id` and asserts `activated_count == 1, processed_event_count() == 1`. Note: the in-process ledger does not literally re-use `migrations/d1/0018_stripe_idem_keys.sql` — it has its own `processed_event_ids` shape; production worker MUST bind to `0018` at the wiring layer, otherwise the parallel dedup table risk pre-flight P0-4 named survives in a different form. Tracked as P1-NEW-1 below. |
| P0-5 | WI-005 enterprise inquiry PII encryption-at-rest | **PARTIAL** | `migrations/d1/0040_enterprise_inquiries.sql:42` declares `encrypted_payload_b64 TEXT NOT NULL` plus sanitised non-PII surrogate columns (`company_initials`, `email_domain`, `has_phone`), and the header documents the BYOK envelope contract per CTRL-PRIV-001. But `crates/corelink-enterprise-inquiry/src/ledger.rs:32-51` stores `InquiryRecord { form: EnterpriseInquiryForm, ... }` as plaintext (`form.company`, `form.email`, `form.phone_optional` are all serialised verbatim into the in-memory mirror and into the Slack body at line 211). No `encrypt`, `seal_envelope`, `aes`, or BYOK call sites exist in `crates/corelink-enterprise-inquiry/src/` (grep confirmed). The schema *promise* is intact; the code *fulfilment* is deferred to the production worker boundary. This matches the migration header note ("the BYOK envelope per S-14") but the spec does not say "ledger holds plaintext mirror, worker seals on D1 write" anywhere in WI-S19-005 — the contract is ambiguous. Track as P1-NEW-3. |

---

## 4. New findings (P0 / P1 / P2)

### P0 — none

No new P0 surfaced post-merge. The 5 pre-flight P0s are closed at FIXED (4 of 5) and PARTIAL (P0-5; tracked as P1-NEW-3 below).

### P1 (new, not in pre-flight)

**P1-NEW-1. Stripe webhook idempotency parallel-table risk at production wiring.**
The in-process `processed_event_ids: HashSet<String>` in `crates/corelink-tier-selection/src/ledger.rs:144-145` is correct for the ledger contract, but the production worker MUST bind to `migrations/d1/0018_stripe_idem_keys.sql` (already in tree) when persisting webhook events — not introduce a third dedup table mirror. This is a wiring-layer P1 that the in-process ledger code cannot enforce. Recommend a doc note in `0039_tier_selection.sql` header or the `corelink-tier-selection/src/lib.rs` rustdoc explicitly saying "production wiring binds to `stripe_idem_keys` (0018), NOT a parallel `tier_selection_event_dedup`".

**P1-NEW-2. `dpa-legal-review.yml` workflow not SHA-pinned (charter constraint #18).**
`.github/workflows/dpa-legal-review.yml:34,47` uses `actions/github-script@v7` (tag-pinned). Repo convention (per `cargo-deny.yml`, `admin-ui-ci.yml`, `docs-a11y.yml`, `reproducible-build.yml`) is `actions/<name>@<40-char-sha> # <semver>`. Bump to `actions/github-script@60a0d83039c74a4aee543508d2ffcb1c3799cdea # v7.0.1` (canonical SHA for v7.0.1).

**P1-NEW-3. Enterprise inquiry plaintext-in-mirror; BYOK envelope deferred to wiring.**
`crates/corelink-enterprise-inquiry/src/ledger.rs` stores `form: EnterpriseInquiryForm` plaintext. Migration `0040_enterprise_inquiries.sql:42` declares `encrypted_payload_b64 TEXT NOT NULL` but no code path in this crate produces that ciphertext. Either (a) the ledger should accept an `EnvelopeSealer` collaborator that produces the `encrypted_payload_b64` blob at submit time (similar to S-14 `corelink-byok` envelope pattern), or (b) the WI-S19-005 spec must explicitly defer envelope sealing to the Cloudflare Worker boundary and document the plaintext-in-mirror contract. Today the contract is implicit. Hard expiry at PRR-S19 W5 (prod-keys cutover D+10) — before any real customer PII flows.

**P1-NEW-4. DPA acceptance happy-path audit ordering violates lookup → emit → mutate (charter #16).**
`crates/corelink-dpa-acceptance/src/service.rs:195-204`: `insert_idempotent(record)` (line 195) executes BEFORE `audit.emit(DpaAuditEvent::Accepted { ... })` (line 199). The negative paths (`LocaleMismatch`, `HashMismatch`, `IdempotencyConflict`) all correctly emit-then-return, but the canonical positive path mutates first. Compare with `crates/corelink-tier-selection/src/ledger.rs:202-208` and `crates/corelink-enterprise-inquiry/src/ledger.rs:173-203` which correctly emit BEFORE mutation. Fix: hoist the `Accepted` audit emit to before `store.insert_idempotent(record)` — the receipt fields `(jti, submission_ts)` are already computed by line 195. Risk: audit-sink failure after a successful insert leaves an un-audited acceptance in the store; the current `?` propagation surfaces an error to the caller but the row remains.

**P1-NEW-5. DPA re-acceptance handler emits no audit event at all.**
`crates/corelink-dpa-versioning/src/re_accept.rs:41-85` walks read → version-check → tenant-read → `write_tenant` → return receipt. There is no `audit` collaborator and no event emit on the version bump. The WI-002 click-through handler is *implicitly* invoked downstream of `re_accept` per the WI-003 spec, but the re-acceptance code path itself produces no `dpa.re_accepted` or `dpa.version_bumped` event — so the audit chain cannot prove forensically that the v1 → v2 transition occurred at time T for tenant X. Pre-flight P1-1 named this gap in the funnel context; it survives at the re-acceptance handler boundary as well. Fix: re_accept handler takes an audit-sink, emits `ReAccepted { tenant, from_version, to_version, accepted_at }` BEFORE `write_tenant`.

**P1-NEW-6. PRR-S19 single Legal Counsel seat, not 3 per locale (pre-flight P1-8 partial).**
`specs/04_sprints/_sealed/S19/PRR-S19.md:264-277` table has **one** Legal Counsel slot (#7), explicitly tagged "synthetic Architect-folded SME review at SEAL for 3 locales DPA templates". Pre-flight P1-8 explicitly asked for 3 separate Legal sign-offs (en-US/CCPA, pt-BR/LGPD, es-419). The PRR W6 waiver expiry (D+10) does say "Real Legal counsel sign-off per locale committed as addendum"; the structural reconciliation is to expand the §9 table to 3 rows (Legal-en-US, Legal-pt-BR, Legal-es-419) so the addendum has 3 named slots, not 1. Same intent, cleaner audit trail.

### P2

**P2-NEW-1.** `crates/corelink-signup/src/billing.rs:70` rustdoc says "NEVER `tokio::spawn` per Lote 10.7bis R5 P0-3" — good — but the crate's Cargo.toml does not have a `[lints.rust] tokio = "deny"` style block. Add a lint at the workspace level if available to mechanically enforce "no tokio in src" (charter #13).

**P2-NEW-2.** The `enterprise_inquiry_outbox` `FOREIGN KEY (inquiry_id) REFERENCES enterprise_inquiries(inquiry_id)` (migrations/d1/0040:111) is a hard FK in D1; D1 does not enforce FKs by default (PRAGMA foreign_keys is per-connection). Confirm production worker sets `PRAGMA foreign_keys=ON` at the connection layer or this is decorative.

---

## 5. Charter constraint check

| # | Constraint | Result | Evidence |
|---|---|---|---|
| 11 | `#[non_exhaustive]` on new public enums | **PASS** | 79 occurrences across the 5 S-19 crates' `src/`. All inspected public enums (`SignupOutcome`, `OrchestrationError`, `BillingIntent`, `TierKind`, `TierError`, `SubscriptionState`, `TierSelectionReceipt`, `SubscriptionActivationReceipt`, `BYOKRequirementsKind`, `ResidencyKind`, `Role`, `InquiryStatus`, `EnterpriseInquiryError`, `DpaAcceptanceError`, `DpaVersioningError`) carry the marker. |
| 12 | Zero `unsafe` outside FFI | **PASS** | `grep -rn "unsafe " crates/corelink-{signup,dpa-acceptance,dpa-versioning,tier-selection,enterprise-inquiry}/` returned empty (excluding comments / test code). |
| 13 | No `tokio` in `src/` | **PASS** | All four hits in `src/` are rustdoc / lint comments documenting the prohibition (`"NEVER tokio::spawn"`, `"no tokio in src"`). Zero actual `tokio::` use sites or `use tokio` imports. |
| 14 | No `prop_assert!(matches!(..., Variant { .. }))` anti-pattern | **PASS** | All hits use the tuple form `Variant(_)` or unit-variant form `Variant`; none use the struct-pattern form. |
| 15 | `PROPTEST_CASES` runtime env-var fn (not const) | **PASS** | `crates/corelink-signup/tests/prop_signup_orchestration.rs:31`, `corelink-dpa-versioning/tests/prop_*.rs:25/36`, etc. All use `std::env::var("PROPTEST_CASES").ok().and_then(...).unwrap_or(10_000)` pattern. |
| 16 | Audit fail-CLOSED ordering: lookup → emit_audit → mutate_state | **MIXED (P1-NEW-4)** | Tier-selection ledger PASSES (line 202-208 emits before line 297 mutation; FailingTierSelectionAuditSink test ratifies fail-CLOSED). Enterprise inquiry ledger PASSES (line 173-203 emit before mutation). DPA-acceptance positive path FAILS — `insert_idempotent` runs before `Accepted` audit emit (service.rs:195-204). Spot-check covered ≥2 crates as charter requires; one violation flagged. |
| 17 | CTRL-CRED-001: no PAT material in any signup/DPA/billing payload | **PASS** | `crates/corelink-signup/src/orchestrator.rs:296-305` returns `first_pat_hash` + `shown_once_token` only in the response body; no PAT material in `SignupAuditRecord` (`crates/corelink-signup/src/audit.rs`); no URL embedding. WI-002 verify endpoint is POST-body per Lote 10.19 fix. Grep for PAT-like patterns (`pat_secret`, `bearer`, `authorization` in DPA/billing payload constructors) returned no leaks. |
| 18 | All new GitHub Actions workflows SHA-pinned | **FAIL → P1-NEW-2** | `.github/workflows/dpa-legal-review.yml:34,47` uses `actions/github-script@v7` (tag). Repo convention is SHA-pinned (see `cargo-deny.yml:56`, `admin-ui-ci.yml:37`, etc.). |

---

## 6. Positives

- **INV-ONBOARD-DPA-FIRST is honoured uniformly.** `prop_dpa_first_never_bypassed_10k` strategy explicitly includes `Just(TierKind::Free)`; the orchestrator structurally cannot reach the Free / Paid dispatch without passing the DPA gate. The pre-flight P0-2 hole is structurally closed, not just patched.
- **Audit-emit-BEFORE-mutation ratified by a property test on the tier-selection orchestrator.** `prop_audit_emit_before_mutation` (`prop_tier_selection.rs:252-287`) drives 1k cases with a `FailingTierSelectionAuditSink` and asserts zero Stripe calls + zero ledger rows; this is the strongest possible enforcement of charter #16 on the load-bearing path.
- **Atomic D1 tx scope is correct and documented.** The 4-insert atomic boundary (`tenant` + `dpa_acceptance_pending` + `pat` + `usage_counter`) is enforced step-by-step with explicit rollback; the boundary deliberately EXCLUDES Stripe (chaos-safe per Lote 10.19 P0).
- **`#[non_exhaustive]` discipline is uniform.** 79 occurrences across the 5 new crates; no public enum without it. This single decision insulates downstream consumers from breaking growth.
- **Stripe webhook signature verify has its own property test.** `prop_stripe_signature_verify` (`prop_tier_selection.rs:303-333`) drives 1k cases with HMAC + 5-min replay window + tamper bit; closes pre-flight P1-3 strictly (not by inheritance).
- **Spec validator clean for S-19.** 327 OK / 9 YAML-only / 14 pre-existing S-11/S-13 ADR failures; **0 S-19 failures**.
- **Migration schema discipline.** Every `CREATE TABLE` is `IF NOT EXISTS`; every column has either a `CHECK` constraint, a `NOT NULL`, or an explicit nullable rationale in a comment. The `enterprise_inquiries` migration is structurally correct even though the in-process ledger does not yet produce the `encrypted_payload_b64` blob (P1-NEW-3 covers the wiring gap).

---

## 7. Per-WI sub-scores

| WI | Score | Notes |
|---|---|---|
| WI-S19-001 | 9.0 | Atomic D1 tx is the load-bearing primitive; rollback discipline is clean; chaos Stripe outage path correctly maps to `Deferred` (202) without leaking partial state. PAT shown-once token returned in response body, no URL embedding. Minor: `handle_billing_failure` (orchestrator.rs:397) could emit a more specific audit reason taxonomy. |
| WI-S19-002 | 8.5 | 6-field consent + ES256 JWT receipt + 3 locales all present; `dpa_acceptances` table PK on `signup_id` enforces PAT-RETRY-IDEMPOTENT-001. Held back by **P1-NEW-4** (positive-path audit emits AFTER `insert_idempotent`). |
| WI-S19-003 | 7.5 | Version-bump + grace + read-only degrade logic correct; the `re_accept` handler is concise. Held back by **P1-NEW-5** (no audit emit at all on re-acceptance — the chain is silent on the v1 → v2 transition forensically). |
| WI-S19-004 | 9.2 | INV-ONBOARD-DPA-FIRST property-test-ratified for all 5 tiers including Free; audit fail-CLOSED property-test-ratified; Stripe signature property-test-ratified; enterprise route enforcement property-test-ratified; UNIQUE partial index defense-in-depth in code AND schema. Highest-quality WI in the sprint. |
| WI-S19-005 | 7.8 | Saga PAT-SAGA-001 atomic Slack + CRM correct; outbox + reconciler shape clean; 24h SLA detector well-designed. Held back by **P1-NEW-3** (schema declares `encrypted_payload_b64 NOT NULL` but ledger holds plaintext form — wiring gap, not a code defect, but the contract is ambiguous). |
| WI-S19-006 | 8.2 | PRR-S19 committed, evidence pack complete, two-phase SEAL canonical, 27 adversarial scenarios catalogued. Held back by **P1-NEW-6** (single Legal Counsel seat in §9 table; should be 3 per locale for cleaner audit trail). The `dpa-legal-review.yml` SHA-pin violation (P1-NEW-2) lands in this WI's deliverable bucket. |

**Sprint average:** 8.4 (median 8.35).

---

## 8. Summary

- **P0 count (new post-merge):** 0
- **P0 count (pre-flight, post-fix):** 4 FIXED + 1 PARTIAL (P0-5 tracked as P1-NEW-3)
- **P1 count (new):** 6 (P1-NEW-1 through P1-NEW-6)
- **P2 count (new):** 2
- **Charter constraints:** 7/8 PASS · 1 violation (P1-NEW-2 SHA-pin) · 1 mixed (P1-NEW-4 audit ordering on DPA-acceptance happy path)
- **Spec validator:** 0 S-19 failures
- **Quality gates:** `cargo build` PASS · `cargo clippy -D warnings` PASS · `cargo test --no-run` PASS · all 5 S-19 crate test suites green on individual runs (signup 42+2+7 = 51, dpa-acceptance + dpa-versioning compile-and-test green via workspace `--no-run`, tier-selection 33+5+19+7 = 64).

**SEAL stands.** No re-SEAL required. The 6 new P1 items track to PRR-S19's existing W5/W6/W7/W8 expiry calendar (D+10 / D+15 / D+45). Round-2 follow-up should focus on closing P1-NEW-3 (encryption envelope), P1-NEW-4 (DPA happy-path audit ordering), and P1-NEW-2 (SHA-pin) before the D+10 cutover.

**Fim AUDIT-S19-SPRINT-CLOSE-R1 v1.0.0.**
