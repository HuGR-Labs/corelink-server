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

**Wave R-1 gate:** all 8 items complete; commit `r1-cleanup-complete`; tag `roadmap-r1-sealed`.

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

Canonical 90-day pre-GA BCP/DR rehearsal calendar (14 drills covering P1 single-region, P2 cross-region + BYOK CMK rotation, P3 full SEV1 simulations, X cross-cutting) lives in `specs/_compliance/BCP-DR-DRILL-CADENCE.md`. The companion 3-tier (L1/L2/L3) on-call escalation matrix with severity × tier × SLA + comms templates + conflict tie-breakers lives in `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md`. Each drill emits an auditor-ready evidence doc per the template in `specs/_compliance/templates/DR-DRILL-EVIDENCE.md` (SOC 2 CC7.5 + CC9.1 + ISO 27031 §8.4 aligned).

### Agent work during R-6

| # | Work item | Owner | Cadence |
|---|---|---|---|
| R6-1 | **Daily evidence digest** — Sonnet agent reads dashboards + audit chain + chaos results + pages, generates `specs/_audits/2026-MM-DD-daily-staging-evidence.md` | 1 Sonnet | Daily |
| R6-2 | **Weekly chaos report** — aggregate weekly chaos drill outcomes + flag regressions | 1 Sonnet | Weekly |
| R6-3 | **Incident response if SEV-1** — orchestrator emergency dispatch per `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md`; root cause + postmortem within 24h. **Drill cadence companion:** `specs/_compliance/BCP-DR-DRILL-CADENCE.md` rehearses the same path bi-weekly (DR-009/DR-010). | Orchestrator + 2 Sonnets if needed | Ad-hoc |

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

### T-7 to T-1 (preparation)
- T-7d: Pentest letter confirmed received; SOC 2 readiness confirmed; lighthouse SLAs attested
- T-7d: **10-min quickstart sealed** — [`apps/docs/docs/tutorials/quickstart-10min.mdx`](apps/docs/docs/tutorials/quickstart-10min.mdx) + [FAQ](apps/docs/docs/tutorials/quickstart-faq.mdx) live, CI validator `quickstart-validate.yml` green, sidebar CTA promoted to top of Get Started, home-page card refreshed (R-prep launch checklist)
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
| H-10 | **SOC 2 audit firm contract** + Drata subscription | R-5 day 0 | $50-95k | R5-3 |
| H-11 | **Law firm engagement** for DPA review | R-5 day 0 | $20-40k | R5-4, R4-5 |
| H-12 | **Lighthouse customer outreach** (3 candidates) — hand `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` at kick-off; internal team works from `specs/_runbooks/RB-LIGHTHOUSE-PHASE-MANAGEMENT.md` (R5-A5) | R-5 day 0 | engagement cost (6mo free) | R5-5, R3-1..5 |
| H-13 | **Apple Developer Program** | R-5 day 0 | $99/yr | R5-6, S-15 Windows fallback |
| H-14 | **Windows EV cert** vendor (DigiCert/Sectigo) | R-5 day 0 | $300-500/yr | R5-7 |
| H-15 | **External advisor pool** — 5+ people | R-5 day 0 | $5-15k/quarter each | R5-8, R7-2 |
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
