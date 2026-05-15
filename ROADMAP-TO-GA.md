---
id: "ROADMAP-TO-GA"
type: "roadmap"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["roadmap", "ga", "production-ready", "post-spec-corpus"]
---

# CoreLink — Roadmap to 100% SOTA Production GA

**Baseline:** 2026-05-14, HEAD `f09d640`, tag `ga-engineering-gate-complete`. All 21 sprint specs SEALED; ~70 Rust crates compile + test; 244 vitest in admin-ui; 264 in docs; 8 TLA+ specs; 62 runbooks. **Engineering Gate state: `CONDITIONALLY_APPROVED`** pending 8 D+60 evidence items. Spec corpus is done; what remains is **production wiring + external engagement + observation period**.

**Target:** GA Limited 2026-07-14 (D+60); GA Full 2026-08-31 (D+108).

**Execution model:** orchestrator (Opus) drives merges/PRs/verification; up to **15 Sonnet agents in parallel** per wave. Human (Gustavo) drives external engagements + signs contracts + pays for certs.

---

## 0. Why this roadmap exists

Sprint waves S-00..S-20 produced a **SOTA spec corpus + reference implementation** but most crates are wired against `InMemoryFake` impls of their dependencies (`trait-abstraction-defer` charter pattern). The code compiles + tests pass + invariants hold mathematically, but production-side HTTP/CF/KMS calls have never executed against real services.

This roadmap closes the gap with **6 phases × 8 waves**:

| Phase | Wave | Goal | Duration | Peak agents | Type |
|---|---|---|---|---|---|
| **1 — Cleanup** | R-1 | Spec hygiene + abandoned review backlog + Opus eyeball pass | 5 days | 8 | Agent |
| **2 — Wiring** | R-2 | Replace InMemory fakes with real HTTP/CF/KMS clients | 10 days | **14** | Agent |
| **3 — Integration** | R-3 | E2E flows + cross-crate integration under real-ish staging | 7 days | 10 | Agent |
| **4 — Apps** | R-4 | admin-ui + docs deployed, Clerk real, custom domains, consent UI live | 7 days | 8 | Agent + Human |
| **5 — Evidence** | R-5 | External engagements (pentest, SOC 2, lighthouse, UX, legal, certs) | 30-60 days | 2-4 | **Human-driven** |
| **5 — Stability** | R-6 | 30d sustained staging + chaos + DR drill + oncall 24/7 real | 30 days | 1-2 | Agent + cron |
| **5 — Gate** | R-7 | Collect D+60 evidence, promote PRR-S20-GA CONDITIONALLY_APPROVED → APPROVED | 7 days | 2 | Orchestrator + Human |
| **6 — Launch** | R-8 | T-7 to T+7 launch orchestration execution | 14 days | 4 | Agent + Human |

**Total elapsed: ~90-120 days** to GA Full. Most agent-budget concentrated in first 30 days (R-1..R-4).

---

## 1. Wave R-1 — Cleanup & Verification (immediate; 5 days; ≤8 agents)

Pre-conditions: none (start now).

| # | Work item | Owner | Effort | Success criterion |
|---|---|---|---|---|
| R1-1 | **S-20 sprint-close round-2 review** post-P0 remediation (verify 7.2→8.5+) | 1 Sonnet | 2h | Audit doc committed; verdict `SEAL APPROVED` or `CONDITIONALLY APPROVED (P1 only)` |
| R1-2 | **Fix 14 pre-existing validate_specs failures** on S-11/S-12/S-13 ADRs (frontmatter `parent` field schema drift) | 1 Sonnet | 3h | `python3 scripts/validate_specs.py` zero failures (down from 14) |
| R1-3 | **S-11 backlog: round-2 truth-table sweep V2 + WI fixes + legal citation re-validation** (tasks #116-119) | 1 Sonnet | 6h | S-11 spec corpus passes validate_specs + audit doc with PASS verdict |
| R1-4 | **S-06 backlog: 2-agent review + P0 fix cascade** (tasks #85-86 abandoned mid-cycle) | 1 Sonnet | 4h | S-06 spec passes validate + WI frontmatters SEALED if not already |
| R1-5 | **S-09 backlog: review cycle + 3 layers of P0 fixes** (tasks #93-97) | 1 Sonnet | 6h | S-09 spec passes validate + audit shows ≥ 8.5 score |
| R1-6 | **Opus eyeball pass on ~70 Cargo.toml + lib.rs** — verify `#[non_exhaustive]`, audit-fail-CLOSED ordering, zero unsafe, PROPTEST_CASES runtime fn, no `prop_assert!(matches!(..., Variant { .. }))` anti-pattern | 1 Opus (this orchestrator can sample, OR delegate to 1 Opus subagent) | 4h | Charter constraint compliance matrix doc with per-crate pass/fail |
| R1-7 | **Worktree cleanup** (S-18/S-19/S-20 = ~15GB after agents done) | 1 Sonnet | 30min | Disk space recovered; only active worktrees remain |
| R1-8 | **Dependency audit** — `cargo audit` + `cargo deny check` + `pnpm audit` on `apps/{admin-ui,docs}` + dependabot PR backlog | 1 Sonnet | 2h | Zero HIGH/CRITICAL advisories; report doc |
| R1-9 | **Engineering onboarding doc live** — Day-0..Day-30 path + 5 domain tracks + first-PR backlog + glossary + buddy protocol under `docs/internal/ENGINEERING-ONBOARDING.md` + `docs/internal/onboarding/` | 1 Sonnet | 3h | Doc set committed; README cross-linked; quarterly-refresh owner named |
| R1-10 | **CI workflow optimization audit** — 71 GHA workflows audited; 11 P1/P2/P3 followup tickets filed; best-practice template `.github/workflows/_TEMPLATE.yml.md` authored; projected ~52 billable-min + ~24 wall-clock min savings per PR p50 once tickets land; zero quality gate weakened. Docs: `specs/_audits/2026-05-15-ci-workflow-optimization.md` + `specs/_audits/ci-optimization-followup-tickets.md`. | 1 Opus orchestrator (delivered 2026-05-15) | 2h | Audit + tickets + template committed; followup tickets queued for R-prep next wave |
| R1-11 | **TLA+ coverage audit + 1 new spec** — 11 TLA+ specs audited (7 canonical + 4 runbook); 9 followup tickets filed in `specs/_audits/tla-followup-tickets.md` (1 CRITICAL net-new + 1 CRITICAL upgrade + 5 HIGH + 2 infra). Highest-impact gap closed by `specs/tla/auth_revocation.tla` covering 3 CRITICAL invariants (INV-AUTH-REVOCATION-IDEMPOTENT / -SLO-60S / -MASS-REVOKE-ATOMIC) + 1 HIGH (INV-AUTH-PROPAGATION-AT-LEAST-ONCE). TLC verified locally: 6 785 distinct states, ~2 s. CI matrix updated (`tla_check.yml` + `nightly.yml`). Docs: `specs/_audits/2026-05-15-tla-coverage-audit.md` + `specs/_audits/tla-followup-tickets.md`. | 1 Opus orchestrator (delivered 2026-05-15) | 2h | Audit + new spec + tickets + CI matrix committed; CRITICAL gap density in §4.4 obligation matrix reduced by 3. |

**Wave R-1 gate:** all 10 items complete; commit `r1-cleanup-complete`; tag `roadmap-r1-sealed`.

---

## 2. Wave R-2 — Production Wiring (depends on R-1; 10 days; ≤14 agents)

Pre-conditions: R-1 complete + user provides production credentials via secrets (Stripe / Clerk / PagerDuty / BYOK / DNS — see §9 Human Track).

Each work item = one Sonnet builder in own worktree. Pattern matches sprint waves.

| # | Work item | Crate | Reference |
|---|---|---|---|
| R2-1 | **Stripe HTTPS client** (replace `InMemoryStripeClient`) — Checkout Session create, webhook signature verify, subscription state sync | `corelink-stripe-real` (new) or `corelink-tier-selection` patch | WI-S19-004 + S-10 billing pipeline |
| R2-2 | **Clerk session validator** real HTTP JWKS fetch + JWT verify | `corelink-clerk-real` (patch existing `corelink-clerk` + `corelink-clerk-cf`) | S-03 |
| R2-3 | **PagerDuty Events API v2** real client | `corelink-pagerduty-real` (patch `corelink-oncall::pagerduty`) | WI-S17-005 |
| R2-4 | **Slack incoming webhook** real client | `corelink-slack-real` (patch `corelink-enterprise-inquiry::slack`) | WI-S19-005 |
| R2-5 | **HubSpot CRM** real client + alt Salesforce stub | `corelink-crm-real` (patch `corelink-enterprise-inquiry::crm`) | WI-S19-005 |
| R2-6 | **AWS KMS real client** (replace `AwsKmsStub`) — encrypt/decrypt/grant/access-check | `corelink-byok-aws-real` (patch) | WI-S14-004 |
| R2-7 | **GCP KMS real client** | `corelink-byok-gcp-real` (patch) | WI-S14-005 |
| R2-8 | **Azure Key Vault real client** | `corelink-byok-azure-real` (patch) | WI-S14-005 |
| R2-9 | **HashiCorp Vault real client** | `corelink-byok-vault-real` (patch) | WI-S14-005 |
| R2-10 | **CF Worker bindings wiring** — R2/D1/KV/DO in `apps/server`, `corelink-clerk-cf`, admin-ui middleware | `apps/server` + multiple | WI-S01-001 + all sprints |
| R2-11 | **HubSpot encryption-at-rest** (close S-19 P1-NEW-3: `encrypted_payload_b64` envelope actually called) | `corelink-enterprise-inquiry::ledger` | S-19 sprint-close P1 |
| R2-12 | **Stripe webhook endpoint** routed in `apps/server` with signature verify + idempotency | `apps/server` + `corelink-tier-selection` | WI-S19-004 |
| R2-13 | **D1 migrations applied to staging DB** — run `wrangler d1 migrations apply` for all 43 migrations on staging | infra | all sprints |
| R2-14 | **Production env secrets matrix** — document required secrets (Stripe / Clerk / PD / 4 BYOK / DNS / GPG / Apple / Windows / SES / SendGrid) in `docs/internal/secrets-checklist.md`; populate via wrangler secrets | docs | cross-cutting |

**Wave R-2 gate:** `cargo build --workspace` clean against `production` feature flag; `cargo test --workspace` includes ≥ 3 integration tests per real client (against staging endpoints with test-mode creds); commit `r2-wiring-complete`; tag `roadmap-r2-sealed`.

**Risk:** rate-limit on simultaneous agents. Mitigation: dispatch in 2 batches of 7 if 14 hits limit (R2-1..7 first, then R2-8..14).

---

## 3. Wave R-3 — Integration & E2E (depends on R-2; 7 days; ≤10 agents)

Pre-conditions: R-2 staging wiring functional.

| # | Work item | Scope |
|---|---|---|
| R3-1 | **E2E: signup → DPA → tier → Stripe Checkout → first PAT → R2 put/get** | `tests/e2e/signup-to-cas.rs` |
| R3-2 | **E2E: BYOK CMK revoke → DEK cache evict → audit → kill switch metric** (real AWS KMS staging key) | `tests/e2e/byok-revoke-flow.rs` |
| R3-3 | **E2E: DSR access → MFA → JWT receipt → erasure worker → verification artifact** | `tests/e2e/dsr-erasure.rs` |
| R3-4 | **E2E: chaos drill → SLO impact → audit** (against real staging chaos cron) | `tests/e2e/chaos-staging.rs` |
| R3-5 | **E2E: oncall page → MTTA → resolution → postmortem template generated** | `tests/e2e/oncall-page.rs` |
| R3-6 | **E2E: admin-ui Playwright suite un-FIXME** — un-skip 14 tests waiting Clerk tokens; provision Clerk test-mode + run | `apps/admin-ui/playwright/` |
| R3-7 | **Integration: signup atomic under Stripe outage** (chaos-injected) — verifies WI-S19-001 atomicity claim against real Stripe transient errors | `tests/integration/signup-stripe-chaos.rs` |
| R3-8 | **TLA+ 4 invariant specs verified** — `cd specs/tla && tlc tenant_isolation.tla` etc. for all 4. Document tlc commit-SHA + counter-examples found (or none) | `specs/tla/` |
| R3-9 | **Workspace clippy --release** + `cargo test --release` — catches release-only bugs (some bugs only surface in opt-level=3) | CI |
| R3-10 | **`pnpm build` on admin-ui + docs on Node 20** — verify CI pipeline green on real GHA runner | `.github/workflows/` |

**Wave R-3 gate:** ≥ 5 of 10 E2E tests pass against staging; tag `roadmap-r3-sealed`.

---

## 4. Wave R-4 — App Hardening + Deploy (depends on R-2 partial; 7 days; ≤8 agents)

| # | Work item | Owner | Notes |
|---|---|---|---|
| R4-1 | **admin-ui CF Pages deploy** with real Clerk publishable key | 1 Sonnet + Gustavo (Clerk dashboard) | Custom domain `admin.corelink.dev` |
| R4-2 | **docs CF Pages deploy** to `docs.corelink.dev` | 1 Sonnet + Gustavo (DNS) | Algolia DocSearch registration |
| R4-3 | **DNS setup**: corelink.dev apex + 3 subdomains + email DKIM/SPF/DMARC | Gustavo (registrar) | 24h propagation |
| R4-4 | **Cookie consent UI live** — Cookiebot or equivalent + integration with consent ledger (WI-S11-003) | 1 Sonnet | LGPD + GDPR + ePrivacy |
| R4-5 | **Privacy/legal pages real content** — replace `cross_functional_review: TBD` flags after Legal review (R-5 dependency) | 1 Sonnet | Block on R5-4 Legal sign-off |
| R4-6 | **3-locale stub translations replaced with real translations** — pt-BR + es-419 native speakers (1-2 weeks effort by translators) | Translators (hired contractors) | 100 + 100 stubs |
| R4-7 | **Status page** at `status.corelink.dev` — Statuspage.io or self-hosted; subscribed to SLO breaches | 1 Sonnet + Gustavo (Statuspage account) | Customer-facing |
| R4-8 | **Email + SMS providers** — SES (transactional) + Twilio (SMS) wired into notification path | 1 Sonnet + Gustavo (AWS/Twilio accounts) | DSR notifications, breach alerts |

**Wave R-4 gate:** all 3 customer-facing URLs respond + return valid CSP + WCAG 2.2 AA axe sweep clean; tag `roadmap-r4-sealed`.

---

## 5. Phase 5 — External Engagements (R-5: 30-60 days; mostly HUMAN-driven)

This phase runs **in parallel** with R-2..R-4 but progresses on calendar time, not agent time. Agent role here: prepare/format artifacts as they arrive.

### R-5 work items — Human (Gustavo) drives, agents support

| # | Work item | Owner | Lead time | Cost (USD) |
|---|---|---|---|---|
| R5-1 | **Pentest vendor selection + SOW signed** (Schellman primary, Bishop Fox secondary, A-LIGN fallback) | Gustavo + Security advisor | T+0..T+10 | $70-150k |
| R5-2 | **Pentest engagement 2-week window + 1-week retest** | Pentest firm | T+15..T+45 | (included) |
| R5-3 | **SOC 2 Type 1 audit kickoff** (Drata + Schellman) | Gustavo + DPO + Compliance | T+0..T+30 (kickoff); 6m to Type 1 report | $40-85k Schellman + $10k/yr Drata |
| R5-4 | **Real Legal review of DPA v1.0.0** (English + Portuguese + Spanish) | External law firm (e.g. Pinheiro Neto for LGPD; DLA Piper for GDPR + CCPA) | T+0..T+30 | $20-40k |
| R5-5 | **3 Lighthouse customer recruitment** (2 Team + 1 Enterprise BYOK) | Gustavo + Sales | T+0..T+20 | engagement: 6mo free + dedicated engineer |
| R5-6 | **Apple Developer Program enrollment** + Developer ID cert | Gustavo | T+0..T+7 | $99/yr |
| R5-7 | **Windows EV code-signing cert** (DigiCert/Sectigo) | Gustavo | T+0..T+14 | $300-500/yr |
| R5-8 | **External advisor pool** — recruit + contract: 3 Security advisors, 2 Privacy/DPO interim, 1 SRE consultant | Gustavo + advisor referrals | T+0..T+30 | $5-15k per advisor/quarter |
| R5-9 | **UX workshop with 3 external devs** | Gustavo + dev community outreach | T+10..T+13 | $500-2k participant honoraria |

### R-5 — Agents do these support tasks (parallel)

| # | Work item | Owner | Effort |
|---|---|---|---|
| R5-A1 | **Lighthouse customer onboarding kit** — slack templates, intro deck, integration timeline, weekly check-in agenda | 1 Sonnet | 4h |
| R5-A2 | **Pentest evidence package** — collect all `specs/_pentest/`, `specs/03_architecture/security_model.md`, audit chain proofs, BYOK matrix, threat model into a single PDF for vendor onboarding | 1 Sonnet | 6h |
| R5-A3 | **SOC 2 evidence collection automation** — Drata API integration to auto-pull evidence from D1 (audit chain), R2 (SBOM), GitHub (PR reviews), PagerDuty (incident timeline) | 1 Sonnet | 8h |
| R5-A4 | **Legal review delivery package** — DPA v1.0.0 3-locale + SCC + sub-processor commitments + privacy notice + 14 canonical compliance docs zipped + signed-PDF transmittal | 1 Sonnet | 4h |
| R5-A5 | **Lighthouse customer playbook + phase-management runbook + 3 case-study templates** — Day1→Day30+ customer-facing playbook (`marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`), internal phase-management runbook (`specs/_runbooks/RB-LIGHTHOUSE-PHASE-MANAGEMENT.md`), 3 marketing-ready case-study templates (`marketing/lighthouse-kit/case-study-template/`) with `corelink-lighthouse-tracker` D1 prefills. Commit: `wt/r5-2-lighthouse-playbook` (see §9 Human Track H-12). | 1 Sonnet (wt-r5-2-lighthouse-playbook) | 4h |

**Wave R-5 gate:** evidence collected as each item completes; this wave does NOT block R-6 onwards — they run in parallel.

---

## 6. Wave R-6 — Sustained Staging (30d observation; depends on R-2/R-4 deployed; 1-2 agents)

Pre-conditions: R-2 wiring deployed to staging; R-4 apps deployed; chaos cron firing.

### Observation criteria (per WI-S20-007 framework)

- 0 SEV-1 incidents over 30d
- ≤ 3 SEV-2 not-resolved over 30d
- Chaos drills weekly (8 experiments per S-17 catalog rotated)
- DR drill semestral (1 cycle in this window)
- Runbook drills monthly (3 P0/P1)
- Synthetic page weekly
- Audit chain INV-AUDIT-APPEND-ONLY daily verification (TLA+ checker)
- Zero data residency violations
- 90d SBOM retention proof daily

### BCP / DR drill cadence + on-call escalation (operational rehearsal)

Canonical 90-day pre-GA BCP/DR rehearsal calendar (16 drills covering P1 single-region, P2 cross-region + BYOK CMK rotation, P3 full SEV1 simulations, X cross-cutting including DR-15 cold-restore-from-zero and **DR-16 active-region warm failover**) lives in `specs/_compliance/BCP-DR-DRILL-CADENCE.md`. The companion 3-tier (L1/L2/L3) on-call escalation matrix with severity × tier × SLA + comms templates + conflict tie-breakers lives in `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md`. Each drill emits an auditor-ready evidence doc per the template in `specs/_compliance/templates/DR-DRILL-EVIDENCE.md` (SOC 2 CC7.5 + CC9.1 + ISO 27031 §8.4 aligned).

**DR-16 active-region warm failover (added 2026-05-15):** complement to DR-15 cold-restore — DR-15 rebuilds from N-1 when a region is *destroyed*; DR-16 flips the active write lease to the sibling when a region is *degraded but not zero* (p95 > 5s OR error rate > 5% sustained). Spec: `specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md`. Runbook (7 steps Detect → Decide → Drain → Promote → Reroute → Verify → Reverse): `specs/_runbooks/RB-ACTIVE-FAILOVER.md`. Orchestrator (3 modes — `--simulate` / `--staging` / `--prod`): `scripts/active-failover-drill.sh`. E2E harness (5 scenarios — primary-up, primary-degraded, split-brain prevention, failback, partial-region): `tests/e2e-failover-router/`. RTO: ≤ 15 min write-flip + ≤ 30 min failback. RPO: ≤ 5 min. SOC 2 evidence stream: `A1.2` (availability) + `CC7.5` + `CC9.1`.

### IR tabletop playbook (Incident Response rehearsal — GAP-03)

Companion to the BCP/DR drill cadence (which focuses on SRE-level operational recovery), the **IR tabletop playbook** lives in `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` (NIST SP 800-61 Rev.2 aligned; quarterly cadence; 90–120 min/session). It exercises the end-to-end human IR program — IC, Scribe, Comms, Tech Lead, Legal Liaison, Privacy Officer, Customer Comms Lead, Security Lead. Six scenarios catalogued under `specs/_compliance/ir-scenarios/`: TT-01 SEV0 data breach (BYOK envelope leak), TT-02 SEV1 cascading failure (D1 replica lag + audit-chain head divergence), TT-03 Stripe webhook compromise, TT-04 insider threat (bulk PAT issuance), TT-05 supply-chain compromise (cargo-audit CRITICAL), TT-06 DDoS / abuse storm. 2026/Q1-2027 execution schedule in `specs/_compliance/IR-TABLETOP-SCHEDULE-2026.md`; auditor-ready evidence form in `specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md`. SOC 2 CC7.3 + CC7.4 + CC7.5 operating-effectiveness evidence. **GAP-03 status:** artefacts landed 2026-05-15 via `wt/gap03-ir-tabletop`; first execution Q3-2026 (TT-01 + TT-02); full 6-scenario rotation completes Q1-2027.

### Agent work during R-6

| # | Work item | Owner | Cadence |
|---|---|---|---|
| R6-1 | **Daily evidence digest** — Sonnet agent reads dashboards + audit chain + chaos results + pages, generates `specs/_audits/2026-MM-DD-daily-staging-evidence.md` | 1 Sonnet | Daily |
| R6-2 | **Weekly chaos report** — aggregate weekly chaos drill outcomes + flag regressions | 1 Sonnet | Weekly |
| R6-3 | **Incident response if SEV-1** — orchestrator emergency dispatch per `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md`; root cause + postmortem within 24h. **Drill cadence companion:** `specs/_compliance/BCP-DR-DRILL-CADENCE.md` rehearses the same path bi-weekly (DR-009/DR-010). | Orchestrator + 2 Sonnets if needed | Ad-hoc |
| R6-4 | **24h endurance drill (manual) + 2h endurance CI (nightly)** — sustained realistic-mix load against staging surfaces slow-drift (memory, p99 creep, audit lag, log volume). Manual 24h cadence: monthly while pre-GA, bi-annually post-GA. CI 2h variant: daily at 03:00 UTC. Drill runbook: `specs/_runbooks/RB-ENDURANCE-24H-DRILL.md`. Script: `tests/load/k6/scenarios/endurance-24h.js`. CI: `.github/workflows/endurance-2h-nightly.yml`. Report template: `tests/load/k6/scenarios/endurance-24h-ANALYSIS-TEMPLATE.md` (SOC 2 CC7.5 + ISO 27031 §8.4 aligned). | 1 Sonnet (CI) + Gustavo + on-call rotation (24h manual) | Monthly (manual) / daily (CI 2h) |

**Wave R-6 gate:** 30 consecutive days of `daily-staging-evidence.md` showing all green; tag `roadmap-r6-sealed`.

---

## 7. Wave R-7 — Evidence Gate (depends on R-5 + R-6 sustained; 7 days; 2 agents)

Pre-conditions: pentest retest letter received; 3 lighthouse SLA attestations signed; 30d staging green; ≥ 5/8 external advisor sign-offs filled.

| # | Work item | Owner | Notes |
|---|---|---|---|
| R7-1 | **Aggregate D+60 evidence pack** — single archive with: pentest retest letter, SOC 2 readiness scorecard, 3 lighthouse SLA attestations, 30d staging report, all sprint impl-sealed tags, 13 canonical sign-offs signed | 1 Sonnet | Output: `specs/_audits/D60-GA-EVIDENCE-PACK.tar.gz` |
| R7-2 | **PRR-S20-GA promotion vote** — collect 13 canonical signatures (5 dual-hat at SEAL + 5-8 external) via DocuSign or equivalent | Gustavo + 13 signers | Async over 7 days |
| R7-3 | **CONDITIONALLY_APPROVED → APPROVED transition commit** — `specs/04_sprints/S20/PRR-S20-GA.md` work_status update + spec contract bump + tag `ga-approved` | Orchestrator | Final commit |
| R7-4 | **Go/No-Go review meeting** — all 13 sign-offs reviewed in real-time; veto from any single sign-off = blocked | Gustavo + 13 signers | Live meeting — driven by `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md` (60 min agenda; 4-signer veto unanimous) against `specs/_compliance/GA-GATE-CRITERIA.md` (59-criteria checklist across 6 tracks) |

### 7.1 GA Gate canonical artefacts (R-7-2 cluster)

The Go/No-Go meeting consumes three formal canonical documents (R-7-2 cluster, worktree `wt/r7-2-ga-gate`):

- **`specs/_compliance/GA-GATE-CRITERIA.md`** — 59-criteria checklist across 6 tracks (Engineering 15 · Security 12 · Operations 10 · Customer 8 · Legal 8 · Marketing 6); each criterion has a concrete pass/fail success metric + evidence link + owner + status; populated at T-24 h pre-launch. Non-waivable floor (GA-GATE-O01, C01, L01, S01-HIGH/CRITICAL) flagged.
- **`specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md`** — 60 min meeting template + 12 PRE-X pre-flight checklist + 3-decision matrix (**GO** / **GO-WITH-WAIVER** / **DEFER**) + 4-signer veto (CEO + CTO + VPSec + VPProduct unanimous required) + waiver discipline (4 requirements; ≤ 5 cumulative; ≤ 30 d expiry; VP+ sign-off) + D+1/D+7/D+30 rollback trigger forward refs.
- **`specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md`** — failure-mode runbook with 18 trigger conditions across 3 post-launch windows (D+1 / D+7 / D+30); 3-decision tree (**HOLD** / **REVERT** / **ESCALATE**); full REVERT sequence (5 steps; ≤ 4 h target); SEV-0 ESCALATE path with 72 h GDPR breach notification clock; re-attestation criteria (8 items; min 14 d / 90 d gap between attempts); quarterly tabletop cadence.

**Wave R-7 gate:** PRR-S20-GA `work_status: APPROVED`; tag `ga-approved`; **GA-go decision locked** per `GA-GATE-GO-NOGO-TEMPLATE.md` §3 decision matrix.

---

## 8. Wave R-8 — GA Launch (depends on R-7 APPROVED; 14 days T-7..T+7; ≤4 agents)

Per `marketing/launch/COORDINATION/LAUNCH-RUNBOOK.md` (shipped by WI-S20-008).

### R-8 artifact links (T-24h..T+72h moment-of-truth window)

| Artifact | Purpose |
|---|---|
| `marketing/launch/LAUNCH-CHECKLIST-V2.md` | 46-row T-24h..T+72h master checklist (owner / action / success metric / fallback). |
| `marketing/launch/STATUS-PAGE-SPEC.md` | Public status page spec (Statuspage.io for GA; 8 components; severity mapping; PagerDuty auto-publish; approval workflow; 7d maintenance template). |
| `marketing/launch/CRISIS-COMMS-TEMPLATES.md` | 5 crisis scenarios with ready-to-send templates (SEV1 outage, privacy incident w/ LGPD Art. 48 + GDPR Art. 33 72h notice, billing bug, BYOK CMK rumor, pentester early disclosure) + launch-defer comms. |
| `marketing/launch/DAY-1-DASHBOARD-SPEC.md` | Single-screen war-room dashboard: signups, activation %, support tickets, top-5 SLO burn, press/social mentions. |
| `specs/_runbooks/RB-LAUNCH-WAR-ROOM-COORDINATION.md` | Lightweight operator playbook for the war room coordinator. |
| `specs/_runbooks/RB-CUSTOMER-SUPPORT-T-90.md` | T+0..T+90 customer-support runbook: 4-tier triage (P0/P1/P2/P3), per-severity SLA + escalation + comms cadence, ticket→incident conversion criteria. |
| `marketing/launch/SUPPORT-RESPONSE-TEMPLATES.md` | 15 canonical ticket-level response templates (first-touch P0/P1/P2/P3, investigating-hold, root-cause, fix-deployed, resolved, DSR, billing, sandbox-expiry). |
| `specs/_runbooks/RB-DSR-TICKET-TRIAGE.md` | DSR inbound triage mapping to `corelink-dsr` pipeline + DPO/Legal escalation for unusual requests. |
| `marketing/launch/SUPPORT-DASHBOARD-SPEC.md` | Support team's operational dashboard: open by severity, SLA burn-down, top-10 categories, NPS sample, DSR + billing panels. |

### T-7 to T-1 (preparation)
- T-7d: Pentest letter confirmed received; SOC 2 readiness confirmed; lighthouse SLAs attested
- T-7d: **10-min quickstart sealed** — [`apps/docs/docs/tutorials/quickstart-10min.mdx`](apps/docs/docs/tutorials/quickstart-10min.mdx) + [FAQ](apps/docs/docs/tutorials/quickstart-faq.mdx) live, CI validator `quickstart-validate.yml` green, sidebar CTA promoted to top of Get Started, home-page card refreshed (R-prep launch checklist)
- T-7d: **Public sub-processors page auto-sync sealed** — [`apps/docs/docs/trust/subprocessors.mdx`](apps/docs/docs/trust/subprocessors.mdx) auto-generated from [`specs/_compliance/VENDOR-RISK-REGISTER.md`](specs/_compliance/VENDOR-RISK-REGISTER.md) via [`scripts/gen-public-subprocessors.py`](scripts/gen-public-subprocessors.py); 30-day customer-broadcast hook ([`scripts/subprocessor-change-notify.py`](scripts/subprocessor-change-notify.py) emitting `corelink.privacy.subprocessor.notify_required` CloudEvents); CI drift gate + nightly sweep ([`.github/workflows/subprocessors-sync.yml`](.github/workflows/subprocessors-sync.yml)) auto-opens legal-review-required PR on register change; runbook [`specs/_runbooks/RB-SUBPROCESSOR-CHANGE.md`](specs/_runbooks/RB-SUBPROCESSOR-CHANGE.md). Closes LGPD Art. 27 §4º + Art. 39 + GDPR Art. 28 §2 obligations (compliance_matrix §4.1 + §5.1 + §7).
- T-3d: Press kit to journalists under embargo (Gustavo + PR firm if hired)
- T-1d: Dry-run system load test (1 Sonnet runs `tests/load/launch-day-projection.rs`)

### T-0 (launch day)
- 00:01 PT: Product Hunt go-live (Gustavo posts; 1 Sonnet monitors comments)
- 06:00 PT: Press release wire (BusinessWire), blog post 01 publish (CF Pages deploy)
- 09:00 PT: CEO LinkedIn (Gustavo posts), Twitter thread (scheduled), Show HN (Gustavo posts)
- 12:00 PT: Monitor incident response; oncall escalation if SEV-1 (existing 24/7 rotation)

### T+1d..T+7d (post-launch)
- T+1d: Media response coordination (1 Sonnet drafts replies; Gustavo approves)
- T+7d: Retrospective + metrics report (1 Sonnet aggregates from METRICS-DASHBOARD.md)

**Wave R-8 gate:** GA tag `corelink-ga-v1.0.0` + retrospective committed; **CoreLink production-launched**.

---

## 9. Human Track (Gustavo must do these; parallel with R-2..R-7)

These are non-delegatable to agents. Track in `specs/04_sprints/S20/human-action-tracker.md`.

| # | Action | When | Cost (USD) | Blocks |
|---|---|---|---|---|
| H-1 | **Stripe account** — create + complete Atlas application + obtain API keys (Test + Live) | R-2 start | $0 | R2-1, R2-12 |
| H-2 | **Clerk account** + production app + publishable+secret keys | R-2 start | $25/mo Pro | R2-2, R4-1 |
| H-3 | **PagerDuty account** + initial rotation + 3-region setup | R-2 start | $21/user/mo | R2-3, R6 |
| H-4 | **AWS / GCP / Azure / Vault accounts** for BYOK staging keys (4 separate vendor accounts) | R-2 start | varies | R2-6..9 |
| H-5 | **HubSpot CRM** account + API key | R-2 start | $45/mo Sales Hub Starter | R2-5 |
| H-6 | **Slack workspace** + incoming webhook URL | R-2 start | free tier ok | R2-4 |
| H-7 | **DNS provider** (Cloudflare Registrar recommended for unified CF) + DKIM/SPF/DMARC | R-4 start | ~$10/yr per domain | R4-3 |
| H-8 | **SES + Twilio** accounts + verified sender domains | R-4 start | pay-per-use | R4-8 |
| H-9 | **Pentest vendor RFP + contract** — engagement scoping pkg `specs/_pentest/PENTEST-EVIDENCE-PACKAGE.md` v2.0.0 finalized 2026-05-14 (R5-1). Engagement scheduled **2026-06-15** (D+0); retest letter due **2026-07-29** (D+44). | R-5 day 0 | $70-150k | R5-1, R5-2 |
| H-10 | **SOC 2 audit firm contract** + Drata subscription · readiness **83.7% internal / 96.4% Drata** (snapshot 2026-05-15, commit `f18acdc`); 33 GAPs · 1 blocking-GA closing D+30 · 9 major · 23 minor — rollup `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` + walkthrough `specs/_compliance/AUDITOR-WALKTHROUGH-SCRIPT.md`. **GAP-14 (vendor risk register, CC9.2) — IMPLEMENTED 2026-05-15** via `specs/_compliance/VENDOR-RISK-REGISTER.md` (19 vendors: 6 Critical / 8 Important / 5 Standard) + `VENDOR-RISK-METHODOLOGY.md` + 5 Critical DD files in `specs/_compliance/vendor-dd/` + `specs/_runbooks/RB-VENDOR-RISK-QUARTERLY-REVIEW.md`. Quarterly review owner: VP-Sec. **Important-tier DD batch landed 2026-05-15** (+ 6 DD files: `DD-GCP-KMS.md`, `DD-AZURE-KEYVAULT.md`, `DD-HASHICORP-VAULT.md`, `DD-HUBSPOT.md`, `DD-PAGERDUTY.md`, `DD-SLACK.md`; register cross-linked v1.1.0); DD coverage now 11/19 vendors. Remaining Important backlog (GitHub, Anthropic, OpenAI, Grafana, Neon) deferred to Q3 bi-annual review window. **GAP-22 (LGPD Art. 33 §1º residency, P-2/Privacy) — IMPLEMENTED 2026-05-15** via `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` + `LGPD-DPO-MONTHLY-CHECKLIST.md` + customer explainer `apps/docs/docs/explanation/residency/lgpd-brazil.mdx` + nightly verifier `scripts/verify-lgpd-residency.py`. **GAP-15 progress (2026-05-15):** cold-restore drill procedure sealed (spec `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` + runbook `specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md` + orchestrator `scripts/cold-restore-drill.sh` + verification gate `scripts/verify-cold-restore.py`); DR-15 row added to `BCP-DR-DRILL-CADENCE.md`; RTO 4h read / 8h write, RPO 15min; first `--dry-run` cycle scheduled T-30d pre-GA. **GAP-02 progress (2026-05-15):** BYOK FIPS attestation kit landed via `wt/gap02-fips-attestation-kit` — matrix `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` (1/4 Attested = AWS; 3/4 Pending = GCP, Azure, Vault — RFIs dispatched 2026-05-15) + 4 vendor letter templates `specs/_compliance/fips-attestation-letters/LETTER-{AWS-KMS,GCP-KMS,AZURE-KV,VAULT}.md` + 30-question RFI `specs/_compliance/FIPS-RFI-QUESTIONNAIRE.md` + endpoint verifier `scripts/verify-fips-endpoints.py` (4/4 providers OK, wired into CI) + quarterly renewal runbook `specs/_runbooks/RB-FIPS-ATTESTATION-RENEWAL.md`. Vendor SLA D+30 (response window closes 2026-06-14); fallback ADR per WI-S20-003 §5.2 ready if any letter slips. **PCI-DSS SAQ-A self-attestation IMPLEMENTED 2026-05-15** via `wt/r-prep-pci-saq-a` — formal SAQ-A questionnaire `specs/_compliance/PCI-DSS-SAQ-A-2026-05-15.md` (24 Q&A across PCI DSS v4.0; Stripe-as-TPSP responsibility matrix; SOC 2 cross-map) + boundary diagram `specs/_compliance/PCI-DSS-BOUNDARY-DIAGRAM.md` (Mermaid + ASCII data-flow proving zero CHD on CoreLink surfaces; 4 grep hits all in `crates/corelink-logpush/src/redaction.rs` defense-in-depth redactor) + annual recertification checklist `specs/_compliance/PCI-DSS-ANNUAL-RECERTIFY.md` (8-step recert; trigger events for early re-attest; next due 2027-05-15) + customer-facing trust page `apps/docs/docs/trust/pci-dss.mdx`; `compliance_matrix.md` PCI row flipped from "fora de escopo" to "Compliant (SAQ-A self-attested)". **GAP-08 (CC4.2 weekly compliance review cadence) — IMPLEMENTED 2026-05-15** via `wt/r-prep-compliance-weekly`: automated digest (`scripts/compliance-weekly-digest.py`) + Monday 09:00 UTC cron (`.github/workflows/compliance-weekly.yml`) + triage runbook (`specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md`) + index + methodology (`specs/_compliance/weekly-digests/README.md`) + 8 escalation triggers (severity-upgrade, GAP count up, drill missed by class, vendor 2× SLA breach, compliance-gate CI failure, multi-class co-occurrence) wired to PagerDuty service `corelink-compliance` + PR-comment pipeline. **GDPR full audit (Art. 5-50) IMPLEMENTED 2026-05-15** via `wt/r-prep-gdpr-full-audit` — 25 articles audited (Art. 5-50 inc. 12-22 DSR rights + Art. 35 DPIA + Art. 44-50 transfers); `specs/_compliance/GDPR-FULL-AUDIT-2026-05-15.md` + SCC execution `GDPR-SCC-EXECUTION-2026-05-15.md` (4 modules P2P/C2C/P2C/C2P + Schrems II TIA) + DPIA library `GDPR-DPIA-LIBRARY.md` (5 completed + lighthouse trigger list) + customer explainer `apps/docs/docs/explanation/privacy/gdpr.mdx` + runbook `specs/_runbooks/RB-DSR-GDPR.md`. | R-5 day 0 | $50-95k | R5-3 |
| H-10b | **ISO 27001:2022 preliminary crosswalk + Q1-2027 cert roadmap** — preliminary Annex A (93 controls) crosswalk landed 2026-05-15 via `wt/r-prep-iso27001-crosswalk`: master `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md` (98.9% in-scope coverage; 91% SOC 2 overlap) + ISO-unique gap analysis `specs/_compliance/ISO27001-GAP-ANALYSIS.md` (7 active minor + 1 informational PIMS) + phased roadmap `specs/_compliance/ISO27001-ROADMAP.md` (M0–M7: ISMS scope → SoA refresh → clauses 4–10 → internal audit → management review → Stage 1 Q4-2026 stacked with SOC 2 Type I → Stage 2 Q1-2027 → certificate). Audit partner: Schellman (same as SOC 2). Incremental cert cost: $55-90k first-year; ~6.25 person-weeks ISO-unique gap closure on top of SOC 2 23.5. Customer-facing one-pager: `apps/docs/docs/trust/iso27001.mdx`. | R-5 day 0 | $55-90k (stacked) | R5-3 |
| H-11 | **Law firm engagement** for DPA review | R-5 day 0 | $20-40k | R5-4, R4-5 |
| H-12 | **Lighthouse customer outreach** (3 candidates) — hand `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` at kick-off; internal team works from `specs/_runbooks/RB-LIGHTHOUSE-PHASE-MANAGEMENT.md` (R5-A5) | R-5 day 0 | engagement cost (6mo free) | R5-5, R3-1..5 |
| H-13 | **Apple Developer Program** | R-5 day 0 | $99/yr | R5-6, S-15 Windows fallback |
| H-14 | **Windows EV cert** vendor (DigiCert/Sectigo) | R-5 day 0 | $300-500/yr | R5-7 |
| H-15 | **External advisor pool** — 5+ people (3 Security advisors, 2 Privacy/DPO interim, 1 SRE consultant per R5-8). **GAP-01 DPO formalization — IMPLEMENTED 2026-05-15** via `wt/gap01-dpo-formalization` → interim DPO formally designated (`specs/_compliance/DPO-APPOINTMENT-2026-05-15.md`, LGPD Art. 41 §1º + ANPD Resolução 18/2024 compliant) + responsibilities RACI (`DPO-RESPONSIBILITIES-MATRIX.md`, 30 rows) + escalation runbook (`specs/_runbooks/RB-DPO-ESCALATION.md`, 12 trigger families, 5-tier SLA matrix) + 90-day handoff plan to permanent DPO (`DPO-HANDOFF-PLAN.md`). Interim acceptable for SAM-only GA per conflict-of-interest analysis; permanent hire blocking for EU enterprise (GDPR Art. 37). Permanent DPO ceremony per `DPO-HANDOFF-PLAN.md §7`. | R-5 day 0 | $5-15k/quarter each (advisor pool) · $120-250k/yr (permanent DPO) | R5-8, R7-2 |
| H-16 | **Translators** for pt-BR + es-419 (replace 200 TODO stubs) | R-4 day 0 | $4-8k flat | R4-6 |
| H-17 | **Cookiebot or equivalent consent platform** | R-4 day 0 | ~$100/mo | R4-4 |
| H-18 | **Statuspage.io** or self-host | R-4 day 0 | $29-99/mo | R4-7 |

**Estimated total Human Track spend to GA: $200-400k** (pentest + SOC 2 + Legal dominate). Plus $1-3k/mo recurring ops (Clerk + PagerDuty + HubSpot + Statuspage + Drata).

---

## 10. Risk register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R-X1 | **Pentest finds CRITICAL bug post R-2 wiring** | M | H | R-6 30d staging window absorbs fix; budget 2-week buffer pre-GA |
| R-X2 | **Lighthouse customer churns mid-engagement** | M | M | Recruit 5 candidates, expect 3 to retain |
| R-X3 | **SOC 2 Type 1 letter slips past D+60** | H | L | Document Type 1 as parallel track, not GA blocker; Type 2 is the customer-facing milestone (12m post-Type-1) |
| R-X4 | **Windows EV cert acquisition slips** | M | L | Document Windows release deferral per ADR-S15-009 (already shipped); macOS + Linux ship anyway |
| R-X5 | **Agent rate limit blocks parallel waves** | H | L | Batch waves (7 agents at a time); buffer days |
| R-X6 | **Real-world traffic surfaces invariant bugs not caught in InMemory tests** | M | H | Canary deploy (5% → 25% → 100%) per S-13 admin plane rollout; auto-rollback at error budget breach |
| R-X7 | **Clerk / Stripe / PagerDuty / BYOK vendor outage during R-3 E2E** | L | M | Vendor SLAs cover; document fallback per WI-S19-001 chaos handling |
| R-X8 | **Code-signing cert acquisition for Apple Notarization fails (Apple rejects)** | L | M | Apple notarization is well-trodden; LLC + business verification standard |

---

## 11. Wave dependency graph

```
R-1 (cleanup) ──┐
                ├──> R-2 (wiring) ──┬──> R-3 (E2E) ──┐
                                    │                ├──> R-7 (gate) ──> R-8 (launch)
                                    └──> R-4 (apps) ─┤
                                                     │
                R-5 (engagements, human-driven) ─────┤
                                                     │
                R-6 (30d staging, parallel R-3..R-5) ┘
```

Critical path: **R-1 → R-2 → R-6 → R-7 → R-8** = 5 + 10 + 30 + 7 + 14 = **66 days minimum** assuming everything is parallel-perfect. Add buffer: **90-120 days realistic**.

---

## 12. Orchestrator (Opus) protocol per wave

For each wave:

1. **Pre-flight**: dispatch 1 Sonnet reviewer to assess wave scope vs current state; identify gaps.
2. **Dispatch**: launch N Sonnet builders in worktrees (1 per work item).
3. **Verify**: when each agent reports SEAL, orchestrator runs `git diff` + `cargo build` + spot-check key patterns; does NOT trust report blindly (charter constraint).
4. **Merge**: orchestrator (or merge-specialist Sonnet) merges sequentially; resolves conflicts; preserves all feature files.
5. **Sprint-close**: dispatch 1 Sonnet adversarial reviewer post-merge; target score ≥ 8.5.
6. **P0 fix** if any: dispatch 1 Sonnet fixer; re-verify.
7. **Tag**: `roadmap-rN-sealed` + push.

**Maximum parallel agents per moment: 14** (wave R-2 builders) + 1 (parallel reviewer/fixer for prior wave) = **15 agents simultaneous peak**, matching user constraint.

---

## 13. Decision points where orchestrator must ask user

1. **R-2 start**: confirm all H-1..H-6 credentials populated as wrangler secrets. Without this, agents will fake-wire and waste cycles.
2. **R-4 deploy**: DNS configuration is hard to reverse; confirm domain + custom-domain plan.
3. **R-5 contracts**: pentest + SOC 2 + Legal are $$$ commitments. Confirm budget + vendor before agents draft engagement letters.
4. **R-6 enter window**: 30d clock starts ticking; any production change resets the window. Confirm staging is feature-frozen.
5. **R-7 promote**: APPROVED vote is irreversible product commitment. Confirm Go before tagging `ga-approved`.
6. **R-8 launch day**: T-0 launch sequence is non-rollback-able marketing fire. Confirm 24h pre-launch.

---

## 14. Success criteria — what "100% SOTA production-ready" means

- ✅ All 8 D+60 Evidence Gate items resolved (per PRR-S20-CLOSING §6)
- ✅ Pentest retest letter: zero HIGH/CRITICAL
- ✅ 3 lighthouse customers attested 30d SLA met
- ✅ 30d sustained staging green
- ✅ 5/8 external advisor sign-offs filled
- ✅ SOC 2 Type 1 audit kicked off (Type 1 letter not required for GA Limited, but kickoff is)
- ✅ Legal review of DPA + privacy notice + SLA complete
- ✅ Real translations (no `<!-- i18n:TODO -->` markers in shipping content)
- ✅ Production wiring against real Stripe + Clerk + PagerDuty + 4 BYOK + CF bindings
- ✅ Custom domains live (api / admin / docs / status)
- ✅ Cookie consent UI live + CTRL-PRIV-CONSENT-001..006 enforced end-to-end
- ✅ All 8 TLA+ specs verified by TLC (commit SHA pinned)
- ✅ Zero validate_specs failures
- ✅ Apple notarization + Linux GPG + (best-effort) Windows Authenticode on release binaries
- ✅ `ga-approved` tag + `corelink-ga-v1.0.0` release tag

---

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Opus 4.7) | Initial roadmap post `ga-engineering-gate-complete` tag; 6 phases × 8 waves; 15 agents simultaneous peak (Wave R-2); 90-120 day path to GA Full; $200-400k Human Track spend. |

---

**Status:** ACTIVE. Wave R-1 dispatch authorized 2026-05-14.
