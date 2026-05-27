---
id: "AUDIT-S19-PREFLIGHT"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Adversarial Pre-flight (Sonnet)"
tags: ["audit", "preflight", "s19", "onboarding", "high-risk"]
---

# AUDIT-S19-PREFLIGHT — Adversarial Pre-flight Review of S-19 WI Specs (6 WIs, HIGH_RISK)

> **Scope:** spec-only review of `specs/04_sprints/S19/` (`_spec_contract.md` + `work_items/WI-S19-001..006`). Unmerged branches NOT inspected.
> **Working dir:** `/Users/gustavoschneiter/Documents/HuGR/corelink-server` @ `a1d00f1` (main).
> **Cycle:** pre-flight (6 WIs in parallel build).

---

## 1. Pre-flight verdict

**NEEDS-SCOPE-FIX** (P0 blockers that must close before the parallel build can land cleanly; nothing FF-HR-009-escalates beyond what is already lane-classified).

The spec corpus is impressively coherent (canonical Lote 10.19 codex fixes already in-flight for ES256 receipt + BEGIN IMMEDIATE + body-not-URL verify endpoint), but **5 P0 issues** would break the build at integration time. None of them require re-scoping the sprint or escalating the lane; they are surgical fixes a parallel builder can fold in without rework.

Lane HIGH_RISK is appropriate. FF-HR-009 (DPA + Terms customer-facing contract) is correctly cited on all 6 WIs. Two **additional latent FF-HR forcing factors** are present and under-acknowledged in the contract (see §5 Compliance/Legal gaps):

- **FF-HR-005** (security control change): Stripe webhook signature verify + DPA receipt JWT signing key rotation + Clerk webhook secret are new authn/audit controls → should be added to `lane_forcing_factors` in `_spec_contract.md` §2 and WI-001 / WI-004 frontmatter.
- **FF-HR-003** (PII processing): enterprise inquiry form persists raw company + role + email + phone + residency + BYOK requirements into CRM (HubSpot) — this is a *new* PII flow not covered by S-11 consent ledger. Should be added to WI-005 frontmatter.

---

## 2. Spec gaps — P0 / P1

### P0 (block build / merge)

**P0-1. D1 migration filename collision (6 WIs collide on next-free number).**
- Current highest committed: `0036_oncall_pages.sql` (`migrations/d1/`).
- WI-001 cites `migrations/0XX_signup_atomic.sql` (placeholder).
- WI-002 needs a `consent_ledger` extension migration (purpose=DataProcessingAgreement enum).
- WI-003 needs `tenant.dpa_acceptance_pending`, `dpa_version_accepted`, `grace_expires_at`.
- WI-004 needs `tenant.subscription_state`, `tenant.tier`, `tenant.subscription_started_at`, `tenant.stripe_customer_id`, plus the **UNIQUE partial index `idx_tenant_active_subscription`**.
- WI-005 needs `enterprise_inquiries` table.
- WI-006 needs nothing at the schema layer (instrumentation only) but may touch `usage_event_idem` for funnel idempotency.
- **All 6 will race on `0037..0042` with no reservation in the contract.** Recommend a fixed reservation block in §3 (below).

**P0-2. INV-ONBOARD-DPA-FIRST: `free` tier path lacks the same D1 lock primitive.**
- WI-004 §6 item 5 says "Free tier ALSO requires DPA (no exception)" — good. But the SQL primitive listed in §6 item 4 (`BEGIN IMMEDIATE TRANSACTION; SELECT dpa_signed_ts; UPDATE subscription_state='active'`) is only on the paid path. Free-tier activation does **not** flow through Stripe Checkout, so the lock must be applied at `select_tier(Free)` too. Spec text is ambiguous; property test 10k must explicitly include `tier=Free` generator.
- **Risk:** free-tier user races DPA acceptance → activated `tenant.subscription_state='active'` with `dpa_signed_ts IS NULL` → INV violation that property test (paid-tier-only generators) would miss.

**P0-3. WI-001 §6.2 "Out-of-scope" carries a contradiction with §6.1 (Lote 10.19 fix incomplete).**
- §6.2 says: "DPA click-through 6-field consent UI surface (WI-S19-002 owns frontend rendering + JWT receipt issuance); **WI-S19-001 atomic D1 tx DOES include INSERT dpa_acceptance row**".
- But §6.1 item 2 lists the D1 transaction inserts as `tenant + tenant_metadata + usage_counter + pat` — **no `dpa_acceptance` row**.
- The two paragraphs disagree. Either (a) §6.1 must add `dpa_acceptance` row insert (with `accepted=false, pending=true`), or (b) §6.2 retracts the "DOES include" clause. Choice matters because the cross-WI invariant chain (signup → DPA → tier → billing) hinges on this row's existence at signup time.

**P0-4. Stripe webhook idempotency not specified by `event_id` in WI-004.**
- WI-004 §6 item 3 mentions "Idempotency via Stripe `idempotency_key` + dedup in D1 audit chain (event_id already-processed check)" but does not specify the **storage primitive** (existing `0018_stripe_idem_keys.sql` migration is in tree; WI-004 should bind to it explicitly).
- **Risk:** double-billing on Stripe retry storm. The migration exists; the spec must reference it (and `0021_billing_replay_audit.sql`) explicitly so the builder doesn't introduce a parallel idempotency table.

**P0-5. WI-005 enterprise inquiry has no encryption-at-rest spec for PII.**
- WI-005 §26 LINDDUN says "company + email em CRM (intentional sales engagement; LGPD Art. 7º legitimate interest)" but never specifies:
  - encryption-at-rest for the local `enterprise_inquiries` D1 row,
  - PII field-level encryption (e.g., `pgcrypto BYTEA` analog applied like S-03 user_email_hash treatment),
  - residency: where the inquiry row lives if `residency_requirements=eu` (cross-region implication; FM-451 territory).
- This is a HIGH_RISK PII flow on a HIGH_RISK lane — **needs an explicit `email_cipher BYTEA`-style column spec** plus residency pin (or explicit deferral note that pre-conversion enterprise inquiries are stored `us` region only, with banner in form).

### P1 (do not block merge but fix before SEAL)

**P1-1. Audit chain coverage incomplete for state transitions.**
- WI-006 enumerates 7 funnel steps but the **audit emit** events for state transitions are not symmetrically enumerated across WIs:
  - `corelink.signup.started` — missing (WI-001 only emits `webhook_invalid_signature`, `replay_rejected`, `duplicate`, `atomicity_rollback`).
  - `corelink.signup.completed` — missing (no positive-path emit specified).
  - `corelink.dpa.accepted` — implied by EVT-049 but not named as a CloudEvents emit.
  - `corelink.tier.selected` — missing.
  - `corelink.billing.activated` — missing (only metric `stripe_activation_total`).
  - `corelink.signup.stripe_link_failed` — present in WI-001 §9.2 (good).
  - `corelink.enterprise_inquiry_atomic_ok` — present in WI-005 (good).
- **Fix:** WI-006 §6 should add a canonical CloudEvents emit list for all 7 transitions, mirror the funnel steps; otherwise the funnel dashboard tells you *that* a step happened but the audit chain can't *prove* it forensically.

**P1-2. WI-003 grace-period degrade has no spec for in-flight DSR / open billing cycles.**
- WI-003 says degrade is read-only (PAT-DEGRADE-001) but doesn't address:
  - **Pending DSR (S-11):** if tenant has a DSR in-flight at degrade moment, does DSR continue (legal obligation) or pause? GDPR Art. 12§3 mandates 30-day response — degrade cannot block this.
  - **Open Stripe billing cycle:** if tenant has an active subscription mid-month and gets degraded, does Stripe continue charging? Refund? Pro-rate? Hard pause via Stripe API?
- Fix: WI-003 §6 needs a "degrade interaction matrix" — for each downstream state (active subscription, open DSR, pending erasure attestation, BYOK key rotation in-flight), the policy is X.

**P1-3. Stripe webhook signature verify not explicitly mandatory in WI-004.**
- WI-004 §8 Gherkin scenarios cover concurrent flow + replay but the **Stripe-Signature HMAC verify + nonce + timestamp window** language is only inherited via "S-10 reuse". Given lane HIGH_RISK + FF-HR-009, this should be an explicit P0 in §6 with its own scenario (not relied on by inheritance).
- Also: idempotency by `event_id` should be a named acceptance criterion not buried in §6 item 3.

**P1-4. CTRL-PRIV-CONSENT-001..006 inheritance verification.**
- WI-002 §6 item 3 enumerates all 6 fields explicitly with correct mapping (good).
- WI-003 re-acceptance flow §6 item 3 says "Same UI as initial DPA click-through (S-19 WI-S19-002 reuse): render DPA v<new> + scroll-gate + 6-field consent capture + JWT receipt" — implicit reuse. Should explicitly state the 6 fields are *re-captured* (new `submission_ts`, new `wording_id`, same `notice_text_hash` pattern but for v2) so a reviewer can verify CTRL-PRIV-CONSENT-005 (versioning) flows correctly.

**P1-5. CTRL-CRED-001 (zero secrets in URL) verification.**
- WI-002 already corrected the verify endpoint to POST body (Lote 10.19 P1 fix — good).
- WI-001 first PAT `shown_only_once_token` — spec says UUID v7 invalidated after first GET, but does not say the *PAT itself* is delivered in response body (not URL); should affirm explicitly. Lower risk because S-03 PAT format already prohibits URL embedding, but cross-WI sanity check needed.
- Signup payload itself: WI-001 §6.1 does not show the response shape. Should explicitly assert: no PAT material returned in any URL/redirect; only response body of the `shown_only_once_token` GET.

**P1-6. RB-FM-SIGNUP-FAILED cross-reference to `failure_modes.md`.**
- WI-006 §6 item 4 creates the runbook stub but does not assign a canonical FM-NNN id. Current registry (`failure_modes.md`) tops out at FM-453. Recommend:
  - `FM-454`: signup atomicity violation (orphan tenant; INV-ONBOARD-ATOMIC-PROVISIONING).
  - `FM-455`: DPA-first race window (INV-ONBOARD-DPA-FIRST).
  - `FM-456`: enterprise handoff Slack+CRM saga partial state.
  - `FM-457`: DPA re-acceptance grace expiration handling drift.
- Without FM-IDs the runbooks float without a registry anchor and the failure-mode review at PRR has no checklist target.

**P1-7. Audit chain DSR pseudonymization (WI-006 §2 acknowledges as a "bug to catch" but no spec).**
- WI-006 §2 lists "DSR pseudonymization gap em onboarding logs" as a compound bug to catch — but no WI actually implements the pseudonymization. Either WI-001 (audit emit time) or WI-006 (cleanup pass) must own the user_email_hash sha256 emission so DSR DELETE in S-11 has a stable handle.

**P1-8. PRR sign-off list missing Legal at the contract level.**
- `_spec_contract.md §6` PRR checklist enumerates "Privacy Officer + Legal + Engineer + QA + Product + SRE + Compliance officer + Sales lead + 2 peers + Privacy/UX advisor".
- WI-S19-001 §28 lists 11 roles including **Legal Counsel #5** — good.
- But the contract §6 says **Legal** singular while WIs list **Legal Counsel** — naming inconsistency; PRR-S19.md must use the canonical "Legal Counsel" with named individual.
- More importantly: FF-HR-009 mandates Legal Counsel **per locale** for the 3 DPA locales (WI-002). PRR-S19 should require **3 separate Legal sign-offs** (en-US, pt-BR, es-419), not a single signature line. Currently `_spec_contract.md §14` says "DPA Legal sign-off em 3 locales" but the PRR roster (§6) has only one Legal seat — these must reconcile.

---

## 3. D1 migration numbering plan

**Current state:** `migrations/d1/` highest = `0036_oncall_pages.sql`.

**Reservation (recommend adding to `_spec_contract.md §11 Dependencies` or a new §11.1 "Schema reservations"):**

| Number | WI | Filename (proposed) | Purpose |
|---|---|---|---|
| `0037` | WI-S19-001 | `0037_signup_atomic.sql` | tenant + tenant_metadata + usage_counter + pat columns/indexes for atomic signup; tenant.primary_region not-null backfill; tenant.dpa_acceptance_pending bool default true; tenant.stripe_customer_id nullable; UNIQUE(user_email_hash). |
| `0038` | WI-S19-002 | `0038_consent_ledger_dpa.sql` | extends `consent_ledger.purpose` enum with `DataProcessingAgreement`; adds `dpa_receipt_jwt_kid` + `dpa_region` columns; per-region JWT key index. |
| `0039` | WI-S19-003 | `0039_dpa_versioning.sql` | tenant.dpa_version_accepted (semver text); tenant.grace_expires_at (utc ts nullable); tenant.degrade_state enum (active|pending_billing_link|grace_degraded). |
| `0040` | WI-S19-004 | `0040_subscription_state.sql` | tenant.subscription_state enum; tenant.tier enum; tenant.subscription_started_at; **UNIQUE partial index `idx_tenant_active_subscription` ON tenant(tenant_id) WHERE subscription_state='active'**; FK + bind to `0018_stripe_idem_keys.sql`. |
| `0041` | WI-S19-005 | `0041_enterprise_inquiries.sql` | `enterprise_inquiries` table; **email_cipher BYTEA (pgcrypto pattern)**; company_cipher; phone_cipher; residency_requirements enum; byok_requirements enum; inquiry_id ULID; submission_ts; slack_idempotency_key; crm_idempotency_key; saga_state enum. |
| `0042` | WI-S19-006 | `0042_onboarding_funnel_idem.sql` | funnel step idempotency table (event_id dedup for step transitions); reuses `0017_usage_event_idem.sql` pattern. |

**Coordination rule:** the first WI to PR adds *its own* migration only and reserves the next-available; subsequent WIs MUST `git pull --rebase main` before adding theirs and confirm no collision. Recommend the contract §11 state explicitly that all 6 reservations are **claimed** and parallel branches must use the assigned number even if merge order differs.

---

## 4. Cross-WI invariant enforcement matrix

The canonical chain is **signup → DPA → tier → billing**. Each link must be enforced by an invariant or transactional primitive with no bypass.

| Link | Invariant | Enforcement primitive | Owner WI | Bypass risk |
|---|---|---|---|---|
| Email verified → tenant exists | INV-ONBOARD-ATOMIC-PROVISIONING (HIGH §3.12) | D1 `BEGIN ... COMMIT` atomic tx (tenant + tenant_metadata + usage_counter + pat + **dpa_acceptance pending row** per P0-3) | WI-001 | P0-3 (see §2): missing dpa_acceptance row insert creates a window where tenant exists but no DPA pending marker; WI-002 click-through would have to INSERT (not UPDATE) → race. |
| Tenant exists → DPA captured | INV-CONSENT-PROOF-VERIFIABLE (CRITICAL §3.12 inherited S-11) | Verify endpoint round-trip ≥ 99.9% + signed JWT receipt ES256 per-region key | WI-002 | None on happy path. Locale forge (mitigated). Hash tamper (mitigated). |
| DPA captured → tier selectable | INV-ONBOARD-DPA-FIRST (HIGH §3.12 NEW) | `BEGIN IMMEDIATE` + `SELECT dpa_signed_ts ... ` + UNIQUE partial index `idx_tenant_active_subscription` | WI-004 | **P0-2:** free-tier path not visibly behind the same lock. Free tier handler may bypass. |
| Tier selected → subscription activated | INV-ONBOARD-DPA-FIRST (same) + Stripe webhook idempotency | Stripe `idempotency_key` + `event_id` dedup via `0018_stripe_idem_keys.sql` | WI-004 | **P0-4:** spec doesn't explicitly bind to existing migration; could create parallel dedup. **P1-3:** signature verify not explicitly mandated. |
| Subscription active → DPA re-acceptance lifecycle | CTRL-PRIV-CONSENT-005 (notice versioning) + PAT-DEGRADE-001 | Daily cron + `grace_expires_at` UTC + read-only degrade | WI-003 | **P1-2:** degrade interaction with in-flight DSR / open Stripe cycle undefined. |
| Enterprise tier → inquiry form (out of chain) | Atomic saga (PAT-SAGA-001) | Slack + CRM both-or-neither via reverse compensation | WI-005 | **P0-5:** PII encryption-at-rest unspecified. |

**Verdict:** the chain is well-thought-out at the spec level but has **3 enforcement gaps** (P0-2, P0-3, P0-4) where a builder following the spec literally could ship a hole.

---

## 5. Legal / compliance sign-off gaps

### Legal seat representation

- `_spec_contract.md §6` PRR roster: **1 Legal seat** listed.
- WI specs §28: each individually lists Legal Counsel as #5 (✅), but for FF-HR-009 onboarding with **3 DPA locales** + **3 jurisdictional regimes** (GDPR / LGPD / CCPA), a single seat is structurally insufficient. Recommend the PRR-S19.md template force **3 Legal sign-off rows** (en-US/CCPA, pt-BR/LGPD, es-419/regional EU+LATAM bridge).
- `_spec_contract.md §14` already implicitly requires "DPA Legal sign-off em 3 locales" — reconcile §6 PRR roster with this.

### Compliance/regulatory bodies not yet sign-off-mapped

- **GDPR Art. 28** sub-processor disclosure: enterprise inquiries persist data in HubSpot (US-hosted SaaS); EU residency requirement field acknowledges this but no sub-processor entry. Should reference S-11 sub_processor_broadcast_log handling (FM-453).
- **CCPA §1798.140(v)** service provider clause: DPA template must include explicit CCPA service provider language; WI-002 §6 doesn't call this out as a Legal review checklist item per-locale.
- **LGPD Art. 33** transferência internacional: enterprise inquiry with `residency_requirements=eu` from a Brazilian customer with HubSpot in US triggers Art. 33. Not in WI-005 spec.

### PII handling on enterprise inquiry (WI-005)

- Encryption-at-rest: see P0-5.
- DSR coverage: if an enterprise prospect submits an inquiry then later requests deletion (CCPA §1798.105 / LGPD Art. 18 / GDPR Art. 17), is the `enterprise_inquiries` row in the DSR scope (S-11)? Not specified. Should be enumerated either in WI-005 §26 or by S-11 retroactive coverage matrix.

### Sales lead seat

- Listed in WI-001 §28 #11 and `_spec_contract.md §6` (✅).
- But Sales lead sign-off should cover **WI-005 sales-handoff.md** content quality, not just delivery — recommend explicit review criterion: lead triage decision tree validated against actual current Sales workflow.

### Audit retention claims

- WI-002 says EVT-049 retention 7y via R2 Object Lock; consistent with S-09. ✅
- WI-003 doesn't restate retention for re-acceptance EVT-049 events — should affirm same 7y window applies to v2/v3 receipts.

---

## 6. Additional findings

### Chaos / cross-sprint integration

- **WI-001 §15 item 1** ("Stripe outage during signup") does **not** reference `corelink-chaos-scheduler` (S-17) — the chaos drill cadence "weekly em staging" should be wired into the S-17 scheduler crate, not a standalone script. Recommend WI-001 §13 Artifacts list `chaos_stripe_outage_signup` as a scheduler entry (yaml/toml manifest under S-17), not a freestanding `tests/chaos_stripe_outage_signup.rs`.

### Runbook anchors

- `specs/05_runbooks/` currently contains only 3 files (RB-BYOK-REVOKE, RB-RUNBOOK-DRILL-INDEX, RB-region). No `RB-FM-` prefix exists yet. WI-006 will introduce the `RB-FM-` convention — confirm with framework that this naming is canonical (Lote 10.20 sealed sprints may have established a different prefix). If the framework already uses `RB-FM-NNN`, anchor IDs accordingly.

### TLA+ coverage

- `invariant_registry.md` notes `onboarding_atomicity.tla` is PLANNED for S-19 covering `InvAtomicTx` + `InvCorrelationIdPropagation`. WI-001 §12 cross-references this. No WI in S-19 is creating the TLA+ spec — confirm with planning whether this slips to S-20 or one of the 6 WIs is implicitly on the hook. Recommend explicit deferral note in WI-006.

### Naming inconsistencies

- Spec contract §5.3 R-S19-7 says tier selection is WI-S19-003 ("…Stripe Checkout S-10 reuse"), but PERT table §12 assigns this to **WI-S19-003** (and frontmatter assigns WI-S19-004). Cross-check §12 PERT row labels — they appear off-by-one in the contract (the row "WI-S19-003" describes tier+Stripe whereas the actual `WI-S19-003-*.md` filename is DPA versioning). This is purely cosmetic but will confuse reviewers; the contract §12 table should be regenerated.

---

## 7. Summary

- **P0 count:** 5
- **P1 count:** 8
- **Lane:** HIGH_RISK appropriate; **add FF-HR-005 + FF-HR-003** to lane_forcing_factors on relevant WIs.
- **FF-HR-ESCALATE:** No.
- **NEEDS-SCOPE-FIX:** Yes — fix P0-1..P0-5 before integration merge.

The 6 specs are mature and recent canonical fixes (Lote 10.19) closed most of the obvious adversarial holes (ES256, BEGIN IMMEDIATE, body-not-URL verify, two-phase atomicity Stripe). The remaining P0s are surgical and predominantly about (a) D1 migration coordination, (b) free-tier consistency with the DPA-first invariant, (c) WI-001/WI-002 atomic boundary alignment for dpa_acceptance row, (d) Stripe webhook idempotency binding to existing migration, (e) WI-005 PII encryption-at-rest. None require rescoping or lane escalation.

**Fim AUDIT-S19-PREFLIGHT v1.0.0.**
