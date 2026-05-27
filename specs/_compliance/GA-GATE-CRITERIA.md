---
id: "GA-GATE-CRITERIA"
type: "compliance_criteria_checklist"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-15"
updated: "2026-05-27"
sprint: "R-7"
parent_wave: "R-7"
parent: "ROADMAP-TO-GA"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["ga", "gate", "criteria", "checklist", "go-nogo", "r7", "engineering-gate", "soc2", "lighthouse", "pentest", "launch"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-lighthouse-tracker` was absorbed into `corelink-telemetry` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-telemetry-absorption.md. Canonical consumer path is now `corelink_telemetry::lighthouse_tracker::*`.

# GA-GATE-CRITERIA — Formal GA Gate Criteria Checklist

> **Purpose.** The single authoritative checklist that the **CEO / CTO / VPSec / VPProduct** sign 24 h before the public GA launch announcement (see `GA-GATE-GO-NOGO-TEMPLATE.md`). Each criterion has a concrete pass/fail success metric + an evidence link + an owner + a status. Drives the binary **GA-GO / GA-WITH-WAIVER / DEFER** decision per ROADMAP-TO-GA §7 (Wave R-7).
>
> **Scope.** All gates the GA decision depends on, across **6 tracks**: Engineering, Security, Operations, Customer, Legal/Compliance, Marketing/Launch. **59 criteria total** (15 engineering · 12 security · 10 operations · 8 customer · 8 legal · 6 launch). Each ID is `GA-GATE-XX` with the track prefix.
>
> **Companion docs.** `GA-GATE-GO-NOGO-TEMPLATE.md` (decision meeting template) · `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` (failure-mode rollback) · `specs/04_sprints/_sealed/S20/PRR-S20-GA.md` (the per-WI evidence backing each criterion).
>
> **Cross-ref with PRR-S20-GA.** PRR-S20-GA §5 DoD checklist (19 items) covers the engineering binary gate. This doc is **wider** — it adds non-engineering tracks (customer, legal, marketing, ops) that PRR-S20-GA delegates to the Owner / Product / Legal sign-offs. The PRR remains the canonical sign-off doc; THIS doc is the **operational pre-flight checklist** used by the four launch executives.

---

## Status legend

| Code | Meaning |
|---|---|
| `READY` | Criterion met; evidence linked + verified; signer approval recorded. |
| `IN_PROGRESS` | Work in flight; ETA documented; on-track to be `READY` ≥ 48 h before T-0. |
| `NOT_STARTED` | No execution yet; **defers GA** unless waived. |
| `BLOCKED` | Active blocker (vendor / regulatory / customer); **defers GA** unless waiver granted by VP+. |
| `WAIVED` | VP+ waiver granted with documented compensating control + time-bound expiry; see waiver register §7. |

A green Go/No-Go meeting requires: ALL `READY` **OR** `WAIVED-with-VP+`. Any `NOT_STARTED` / `IN_PROGRESS` / `BLOCKED` at T-24 h forces **DEFER** unless converted to `WAIVED` by the decision matrix in `GA-GATE-GO-NOGO-TEMPLATE.md`.

---

## 1. Engineering track (15 criteria) — owner: CTO

Coverage thresholds, mutation testing, fuzz, perf budgets, SLO instrumentation, license/SBOM hygiene, P0 closure.

| ID | Criterion | Success metric (concrete pass/fail) | Evidence | Owner | Status |
|---|---|---|---|---|---|
| GA-GATE-E01 | **Workspace test coverage** | `cargo llvm-cov --workspace` ≥ **85% line + 80% branch** on `main`; per-crate floor 70% (no crate < 70%); admin-ui ≥ 80% (vitest); docs ≥ 80%. CI artefact ID linked. | `specs/_audits/2026-MM-DD-coverage-baseline.md` (latest); CI run `coverage-workspace.yml` | Engineer Lead | NOT_STARTED |
| GA-GATE-E02 | **Mutation testing kill-rate floor** | `cargo mutants` kill-rate ≥ **70%** across CRITICAL crates (corelink-cas, corelink-audit, corelink-byok-*, corelink-billing, corelink-tier-selection, corelink-tenant-isolation); zero `MISSED` on invariant-bearing fns. | `specs/_audits/2026-MM-DD-mutation-baseline.md`; commit SHA | QA Lead | NOT_STARTED |
| GA-GATE-E03 | **Fuzz corpus runtime minimum** | ≥ **24 h cumulative `cargo fuzz`** per fuzz target × 8 targets (cas-parse, audit-chain, byok-envelope, dpa-parse, billing-replay, signup-saga, residency-check, consent-receipt) since last release tag; zero crashes in last 24 h. | `specs/_audits/2026-MM-DD-fuzz-baseline.md`; CI artefacts | Security Lead | NOT_STARTED |
| GA-GATE-E04 | **Performance p99 budgets** | All SLOs in `slo_catalog.md` p99 < target sustained 30 d in staging: SLO-LAT-CAS-GET p99 < **300 ms** · SLO-LAT-CAS-PUT p99 < **800 ms** · SLO-LAT-SIGNUP p99 < **3 s** · SLO-LAT-DSR-RECEIPT p99 < **5 s**. Grafana export pinned. | DASH-GA-READINESS export; `specs/_audits/2026-MM-DD-perf-30d.md` | SRE Lead | NOT_STARTED |
| GA-GATE-E05 | **SLO instrumentation 100%** | Every SLO in `specs/05_quality/slo_catalog.md` has an emitting metric in the Prometheus catalog **AND** a Grafana panel **AND** an alert rule **AND** a runbook. Auto-checked by `scripts/validate_slo_instrumentation.py` exit 0. | CI run; `specs/_audits/2026-MM-DD-slo-instrumentation.md` | SRE Lead | NOT_STARTED |
| GA-GATE-E06 | **License policy clean** | `cargo deny check licenses` exit 0 on `main`; allowlist = MIT/Apache-2.0/BSD-2-Clause/BSD-3-Clause/ISC/Unicode-DFS-2016/CC0-1.0/MPL-2.0; zero GPL/AGPL/SSPL/proprietary. `pnpm licenses list` cross-check clean. | CI run `license-check.yml`; `specs/_audits/2026-MM-DD-license-policy.md` | Engineer Lead | NOT_STARTED |
| GA-GATE-E07 | **SBOM published + retained** | CycloneDX 1.5+ SBOM signed by cosign keyless + published to R2 `sbom/` + 90 d retention proof daily cron (AUDIT-S20-SBOM-90D-RETENTION); per-release tag SBOM stored as GitHub Release asset. | AUDIT-S20-SBOM-90D-RETENTION; release artefact `corelink-ga-v1.0.0-sbom.cdx.json.sig` | Engineer Lead | NOT_STARTED |
| GA-GATE-E08 | **Every P0 closed** | Zero open P0 issues across `gh issue list --label P0 --state open`; zero open P0 PR review threads; zero `// TODO(P0)` markers in compiled crates (rg sweep). | `gh issue list` snapshot; `rg "TODO\(P0\)"` sweep | Engineer Lead | NOT_STARTED |
| GA-GATE-E09 | **Reproducible build** | Two independent CI runs on identical commit SHA produce **byte-identical** release binaries (linux-x86_64-musl, linux-aarch64-musl, darwin-arm64, darwin-x86_64); SHA256 manifest committed. | `specs/_audits/2026-MM-DD-reproducible-build.md`; release manifest `corelink-ga-v1.0.0.sha256` | Engineer Lead | NOT_STARTED |
| GA-GATE-E10 | **TLA+ 8 specs verified** | All 8 TLA+ specs GREEN in CI on `main`: tenant_isolation, cas_integrity, audit_immutability, gc_correctness, signup_atomic, dpa_versioning_grace, byok_kill_switch, residency_failover. TLC v1.8.0 SHA-256 pinned. | PRR-S20-CLOSING §4; CI run `tla-verify.yml` | Architect | NOT_STARTED |
| GA-GATE-E11 | **Workspace clean build** | `cargo build --workspace --release --all-features` exit 0; `cargo clippy --workspace --all-targets -- -D warnings` exit 0; `cargo test --workspace --release` exit 0 in last 48 h. | CI run `release-build.yml`; commit SHA | Engineer Lead | NOT_STARTED |
| GA-GATE-E12 | **27+ INVs active** | All 27+ cumulative INVs in `specs/03_architecture/invariant_registry.md` `status: ACTIVE`; INV-CRITICAL (10) test-coverage matrix 100% green per PRR-S20-CLOSING §3 row 19. | invariant_registry.md; PRR-S20-GA §9 | Architect | NOT_STARTED |
| GA-GATE-E13 | **Spec corpus clean** | `python3 scripts/validate_specs.py` exit 0 (zero schema failures); zero stale `DRAFT` sprint.md frontmatter with corresponding `impl-sealed` tag (G-DOC-SWEEP closed). | CI run; PRR-S20-GA §7 G-DOC-SWEEP | Owner | NOT_STARTED |
| GA-GATE-E14 | **CF Worker bundle size + cold-start** | `apps/server` worker bundle ≤ **1 MB** gzipped; cold-start p99 < **50 ms** measured weekly against staging custom-domain; per-WI-S01-001 budget. | DASH-GA-READINESS; `specs/_audits/2026-MM-DD-worker-bundle.md` | Engineer Lead | NOT_STARTED |
| GA-GATE-E15 | **D1 migrations applied + reversible** | All 43+ D1 migrations applied to staging via `wrangler d1 migrations apply`; each migration has a dry-run rollback test (RB-D1-MIGRATION-APPLY); zero pending in `migrations/applied.json`. | RB-D1-MIGRATION-APPLY dry-run log; staging D1 console | SRE Lead | NOT_STARTED |

**Engineering track sign-off:** CTO must confirm all 15 `READY` (or `WAIVED-with-VP+`) before signing the Go/No-Go form.

---

## 2. Security track (12 criteria) — owner: VPSec / Security Lead

Pentest remediation, SOC 2 readiness, BYOK matrix, audit-fail-CLOSED enforcement, CVE hygiene, secrets matrix, RFC 9116 disclosure, hall-of-fame.

| ID | Criterion | Success metric | Evidence | Owner | Status |
|---|---|---|---|---|---|
| GA-GATE-S01 | **Pentest findings remediated** | External pentest report (Schellman/Bishop Fox/A-LIGN) **zero HIGH/CRITICAL** open after retest; MEDIUM findings ≤ 3 with documented compensating controls + 90 d remediation plan. | `specs/_pentest/PENTEST-RETEST-LETTER.pdf`; PRR-S20-GA §6.2 W-PT | Security Lead | NOT_STARTED |
| GA-GATE-S02 | **SOC 2 readiness ≥ 90%** | Drata Trust Center readiness score ≥ **90%** across CC1..CC9 + A1 + C1 + PI1; ≤ **5 major gaps** open with 90 d remediation plans (per SOC2-GAP-ANALYSIS.md); Type I fieldwork kicked off ≥ T-30 d. | Drata Trust Center dashboard screenshot; `specs/_compliance/SOC2-GAP-ANALYSIS.md` | Compliance Officer | NOT_STARTED |
| GA-GATE-S03 | **BYOK 4-provider matrix tested** | E2E test green against **AWS KMS + GCP KMS + Azure Key Vault + HashiCorp Vault** — encrypt/decrypt/grant/revoke/access-check/kill-switch — within last 7 d on staging keys; `tests/e2e/byok-revoke-flow.rs` green for each provider. | `specs/_audits/2026-MM-DD-byok-4-provider-matrix.md`; CI run `byok-e2e.yml` | Security Lead + Architect | NOT_STARTED |
| GA-GATE-S04 | **Audit-fail-CLOSED enforced everywhere** | Every audit-emitting code path has a `fail_closed` ordering test that proves the operation **does not proceed** if the audit emit returns Err; `corelink_audit_fail_open_total` gauge = 0 sustained 30 d. INV-AUDIT-APPEND-ONLY TLA+ green. | `corelink_audit_fail_open_total` Grafana panel; `cargo test fail_closed_` | Security Lead | NOT_STARTED |
| GA-GATE-S05 | **Dependency CVE scan clean** | `cargo audit` exit 0 on `main` (zero RUSTSEC HIGH/CRITICAL); `pnpm audit --audit-level=high` zero CRITICAL+HIGH; OSV scan via `osv-scanner` clean; dependabot backlog ≤ 5 open. | CI run `dep-audit.yml`; `gh pr list --label dependencies` | Security Lead | NOT_STARTED |
| GA-GATE-S06 | **Secrets matrix complete** | All production secrets enumerated in `docs/internal/secrets-checklist.md` (Stripe Live + Clerk Live + PagerDuty + 4 BYOK + DNS + SES + Twilio + GPG + Apple + Windows + Cookiebot + Statuspage + Drata + Hubspot + Slack); each verified loaded via `wrangler secret list`; rotation cadence documented (≤ 365 d). | `docs/internal/secrets-checklist.md`; `wrangler secret list` snapshot | Security Lead + SRE Lead | NOT_STARTED |
| GA-GATE-S07 | **RFC 9116 security.txt live** | `https://corelink.humangr.com/.well-known/security.txt` returns 200 with valid PGP-signed RFC 9116 body; `Expires:` ≥ T+365 d; `Contact: mailto:security@humangr.com` reachable; `Encryption:` key on keys.openpgp.org. | `curl https://corelink.humangr.com/.well-known/security.txt`; PGP fingerprint verified | Security Lead | NOT_STARTED |
| GA-GATE-S08 | **Hall-of-fame configured** | Public security researcher hall-of-fame page live at `https://corelink.humangr.com/security/hall-of-fame`; bug bounty program ToS published; HackerOne / Intigriti private program scoped (public optional Day-30). | Public URL HTTP 200; program scope doc | Security Lead | NOT_STARTED |
| GA-GATE-S09 | **Supply-chain provenance** | SLSA Level 3 provenance signed by GitHub Actions OIDC; Rekor entry per release artefact; INV-SUPPLY-SIGNED-DEPLOY + INV-SUPPLY-PROVENANCE-IN-REKOR green; cosign verify passes against release tag. | Rekor log URL; `cosign verify --certificate-identity` exit 0 | Engineer Lead + Security Lead | NOT_STARTED |
| GA-GATE-S10 | **Threat model current** | `specs/03_architecture/security_model.md` + STRIDE workbooks reviewed within last 90 d; any architectural change post-review noted with delta; LINDDUN privacy threat model current. | security_model.md `updated:`; threat-model review log | Architect + Security Lead | NOT_STARTED |
| GA-GATE-S11 | **MFA + session policy enforced** | INV-ADMIN-MFA-FRESHNESS (`UV=1` < 24 h) enforced for all admin paths; `corelink_admin_mfa_freshness_violation_total` = 0 sustained 30 d; INV-ADMIN-DUAL-APPROVAL test green. | Grafana panel; `cargo test admin_mfa_` | Security Lead | NOT_STARTED |
| GA-GATE-S12 | **Crypto sovereignty proof** | INV-BYOK-CRYPTO-SOVEREIGNTY E2E test proves CoreLink **cannot decrypt customer data** after CMK revoke; staging dry-run within last 7 d; AUDIT-S20-BYOK-SOVEREIGNTY signed by Architect + Security Lead. | `specs/_audits/2026-MM-DD-byok-sovereignty.md` | Security Lead + Architect | NOT_STARTED |

**Security track sign-off:** VPSec must confirm all 12 `READY` (or `WAIVED-with-VPSec`). Any HIGH/CRITICAL pentest finding open at T-24 h is **DEFER** — no waiver.

---

## 3. Operations track (10 criteria) — owner: SRE Lead

30 d staging, on-call rotation, runbook coverage, DR drills, status page, incident playbooks.

| ID | Criterion | Success metric | Evidence | Owner | Status |
|---|---|---|---|---|---|
| GA-GATE-O01 | **30 d staging completed without P1+** | 30 consecutive calendar days of `daily-staging-evidence.md` all GREEN: zero SEV-1, ≤ 3 SEV-2 not resolved, all SLOs sustained, chaos drills passing per rotation; CCN counter `corelink_staging_consecutive_green_days_gauge` ≥ 30. | R-6 evidence digest series; `specs/_audits/2026-MM-DD-30d-staging.md` | SRE Lead | NOT_STARTED |
| GA-GATE-O02 | **On-call rotation staffed** | 24/7 PagerDuty rotation across **3 regions** (US + EU + APAC or US + EU + follow-the-sun); ≥ 4 humans per region tier-1; ≥ 2 tier-2 escalation per region; ONCALL-ESCALATION-MATRIX.md current. | PagerDuty schedule export; ONCALL-ESCALATION-MATRIX.md | SRE Lead | NOT_STARTED |
| GA-GATE-O03 | **All SLOs have runbooks (100%)** | Every SLO in `slo_catalog.md` maps 1:1 to a runbook in `specs/_runbooks/`; `scripts/validate_slo_runbook_coverage.py` exit 0; runbook dry-run cadence current (≤ 90 d since last dry-run per P0/P1 runbook). | CI run; runbook index | SRE Lead | NOT_STARTED |
| GA-GATE-O04 | **DR drill rate ≥ 90%** | At least **9 of 10 scheduled DR drills** in BCP-DR-DRILL-CADENCE.md executed in last 90 d with `status: completed` + auditor-ready evidence doc per DR-DRILL-EVIDENCE template; remaining 1 either rescheduled within 14 d or waived by SRE Lead. | BCP-DR-DRILL-CADENCE.md; `specs/_audits/2026-MM-DD-bcp-drill-*.md` series | SRE Lead | NOT_STARTED |
| GA-GATE-O05 | **Status page live** | `https://status.corelink.humangr.com` returns 200; auto-subscribed to Prometheus alerts for SEV-1/SEV-2; component map mirrors public capabilities; T-7 dry-run incident posted + reverted. | Status page URL; subscription audit | SRE Lead | NOT_STARTED |
| GA-GATE-O06 | **Incident playbooks current** | All 47 runbooks `doc_status: FROZEN` or `ACTIVE` (none `DRAFT`); per-runbook `updated:` within last 180 d; postmortem template + process runbook (RB-POSTMORTEM-PROCESS) signed by SRE Lead. | `specs/_runbooks/` directory sweep; RB-POSTMORTEM-PROCESS | SRE Lead | NOT_STARTED |
| GA-GATE-O07 | **Synthetic page weekly < 5 min** | Synthetic PagerDuty page test fires every Monday 09:00 UTC; SRE on-call ack p99 < **5 min** sustained 4 consecutive weeks; RB-SYNTHETIC-PAGE-DRILL current. | DASH-ONCALL-MTTA; synthetic-page log | SRE Lead | NOT_STARTED |
| GA-GATE-O08 | **Backup verification daily** | RB-BACKUP-VERIFICATION daily cron exit 0 for last 30 d; restore-from-backup dry-run completed within last 30 d; D1 export checksum-verified. | `corelink_backup_verification_success_total` panel; RB-BACKUP-VERIFICATION log | SRE Lead | NOT_STARTED |
| GA-GATE-O09 | **Capacity headroom** | Production-projected load + 3 × peak per S-13 capacity model fits within Cloudflare Workers / R2 / D1 quota with ≥ **50% headroom**; load test `tests/load/launch-day-projection.rs` green at T-1 d. | `specs/_audits/2026-MM-DD-capacity-headroom.md`; load test log | SRE Lead | NOT_STARTED |
| GA-GATE-O10 | **Chaos catalog rotated** | RB-CHAOS-CATALOG: at least 8 of 8 experiments executed in last 30 d with documented blast radius + recovery time; zero permanent regressions; chaos cron firing weekly. | RB-CHAOS-CATALOG; weekly chaos report | SRE Lead | NOT_STARTED |

**Operations track sign-off:** SRE Lead must confirm all 10 `READY`. GA-GATE-O01 (30 d staging) is non-waivable per PRR-S20-GA §6 — any reset of the 30 d clock automatically **DEFERs** GA by ≥ 30 d.

---

## 4. Customer track (8 criteria) — owner: VPProduct / Product Lead

Lighthouse attestations, onboarding, docs translations, quickstart, billing surface.

| ID | Criterion | Success metric | Evidence | Owner | Status |
|---|---|---|---|---|---|
| GA-GATE-C01 | **3 lighthouse customers attested** | 3 signed lighthouse SLA attestations on file (1 Team + 1 OSS + 1 Enterprise BYOK, OR 2 Team + 1 Enterprise BYOK per PRR-S20-GA §5 row 5); each customer's 30 d SLA sustained per `corelink-lighthouse-tracker`; attestation doc `signed_at: < T-7 d`. | `specs/_lighthouse/attestations/`; `lighthouse_sla_samples` D1 query | Product Lead | NOT_STARTED |
| GA-GATE-C02 | **Onboarding playbook reviewed** | `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` doc_status FROZEN; reviewed by 2 customer-facing engineers within last 30 d; internal phase-management runbook RB-LIGHTHOUSE-PHASE-MANAGEMENT current. | CUSTOMER-PLAYBOOK.md frontmatter; RB-LIGHTHOUSE-PHASE-MANAGEMENT | Product Lead | NOT_STARTED |
| GA-GATE-C03 | **Public docs 100% pt-BR + es** | `apps/docs` build green with **pt-BR + es-419** locales 100% coverage of canonical pages (zero `<!-- i18n:TODO -->`); MT-stub OK for GA Day-1 with banner; native human translation contract signed for Day-30 backfill. | `pnpm build` on docs; translator contract; i18n sweep `rg "i18n:TODO"` | Product Lead | NOT_STARTED |
| GA-GATE-C04 | **Quickstart < 10 min validated** | 3 external testers (not employees, not lighthouse customers) complete `docs/quickstart` from `corelink.humangr.com` landing → first CAS put/get in **< 10 minutes**; recorded screen captures on file; median time ≤ 7 min. | `specs/_audits/2026-MM-DD-quickstart-validation.md` (3 testers); video links | Product Lead | NOT_STARTED |
| GA-GATE-C05 | **Pricing/billing surface clean** | `https://corelink.humangr.com/pricing` returns 200 with correct tiers (Free / Team / Enterprise BYOK); Stripe Checkout end-to-end test green within last 24 h (test card + live mode dry-run); webhook idempotency test green; INV-BILLING-NO-DUP + INV-BILLING-NO-LOSS active. | Stripe Checkout dry-run log; pricing URL | Product Lead + Engineer Lead | NOT_STARTED |
| GA-GATE-C06 | **Cookie consent UI live** | `https://corelink.humangr.com` shows Cookiebot (or equivalent) consent banner pre-set to deny-by-default; integration with consent ledger (WI-S11-003) tested; CTRL-PRIV-CONSENT-001..006 enforced E2E; LGPD + GDPR + ePrivacy compliant. | Consent banner screenshot; consent-ledger CI test | Product Lead + Privacy Officer | NOT_STARTED |
| GA-GATE-C07 | **Custom domains live + WCAG 2.2 AA** | `corelink.humangr.com` + `api.corelink.humangr.com` + `admin.corelink.humangr.com` + `docs.corelink.humangr.com` + `status.corelink.humangr.com` all respond 200 with valid TLS + CSP + axe-core WCAG 2.2 AA sweep clean (zero serious/critical). | axe-core CI run; DNS + TLS audit | Product Lead | NOT_STARTED |
| GA-GATE-C08 | **Customer support channel ready** | `support@humangr.com` inbox staffed; SLA published (first response ≤ 1 business day Team tier, ≤ 4 h Enterprise tier); ticketing system (Linear/Pylon) integrated with on-call rotation for P0/P1. | Inbox audit; SLA published URL | Product Lead | NOT_STARTED |

**Customer track sign-off:** VPProduct must confirm all 8 `READY`. GA-GATE-C01 (3 lighthouse attestations) is non-waivable per PRR-S20-GA §6.

---

## 5. Legal / Compliance track (8 criteria) — owner: Legal Counsel + DPO

DPA, sub-processor list, breach notification, retention, residency, ToS/Privacy, LGPD + GDPR DSR pipelines.

| ID | Criterion | Success metric | Evidence | Owner | Status |
|---|---|---|---|---|---|
| GA-GATE-L01 | **DPA signed by 3 lighthouse customers** | DPA v1.0.0 (en + pt-BR + es-419) executed by all 3 lighthouse customers; signed PDF on file; INV-ONBOARD-DPA-FIRST green for each tenant_id. | `legal/dpa/signed/`; `dpa_acceptances` D1 query | Legal Counsel | NOT_STARTED |
| GA-GATE-L02 | **Sub-processor list public** | `https://corelink.humangr.com/legal/sub-processors` returns 200 with current list (Cloudflare, Clerk, Stripe, PagerDuty, Drata, Statuspage, etc.); RSS / email-subscribe live for changes per RB-DPA-CHANGE; sub-processor-published.v1 event emitted. | Public URL; RSS feed; CloudEvent log | Legal Counsel + DPO | NOT_STARTED |
| GA-GATE-L03 | **Breach notification runbook legal-signed** | `specs/_runbooks/RB-BREACH-NOTIFICATION` (or `legal/breach-notification/`) signed off by external Legal Counsel (Cooley/DLA Piper/Bird & Bird); dry-run tabletop completed with each lighthouse customer per SOC2-GAP-ANALYSIS GAP-06. | Legal sign-off PDF; tabletop audit doc | Legal Counsel + Privacy Officer | NOT_STARTED |
| GA-GATE-L04 | **Retention 7y enforced** | Audit chain + invoice + DPA records retention configured to **7 years** (LGPD Art. 16 + SOC 2 CC1.4); deletion job at T+7y dry-run tested; `corelink_retention_policy_violation_total` = 0. | Retention policy config; dry-run log | DPO + Compliance Officer | NOT_STARTED |
| GA-GATE-L05 | **Residency E2E test green** | `tests/e2e/residency-failover.rs` green within last 7 d; INV-REGION-NO-CROSS-LEAK + INV-DATA-RESIDENCY active; `corelink_residency_violation_total` = 0 sustained 30 d; TLA+ residency_failover green. | CI run; Grafana panel | Engineer Lead + Privacy Officer | NOT_STARTED |
| GA-GATE-L06 | **Terms of Service live** | `https://corelink.humangr.com/legal/terms` returns 200 with Legal-Counsel-signed ToS v1.0.0; effective date ≥ T-7 d; clickwrap acceptance UI ties to `terms_acceptances` D1 ledger. | Public URL; ledger query | Legal Counsel | NOT_STARTED |
| GA-GATE-L07 | **Privacy Policy live** | `https://corelink.humangr.com/legal/privacy` returns 200 with Legal-Counsel-signed Privacy Notice v1.0.0 (en + pt-BR + es-419); LINDDUN findings disclosed; data subject rights enumerated. | Public URL; LINDDUN cross-ref | Privacy Officer + Legal Counsel | NOT_STARTED |
| GA-GATE-L08 | **LGPD + GDPR DSR pipelines green** | DSR access + portability + erasure pipelines E2E test green within last 7 d for both LGPD and GDPR flows; SLO-FRESH-DSR-ERASURE ≤ 30 d sustained; INV-DATA-ERASURE-COMPLETE + INV-ERASURE-ATTESTATION-SIGNED active. | `tests/e2e/dsr-erasure.rs`; Grafana DSR panel | Privacy Officer + Engineer Lead | NOT_STARTED |

**Legal track sign-off:** Legal Counsel must confirm all 8 `READY`. GA-GATE-L01 (3 signed DPAs) is non-waivable.

---

## 6. Marketing / Launch track (6 criteria) — owner: Owner / CEO

Blog posts, launch playbook, press list, customer quotes, status comms.

| ID | Criterion | Success metric | Evidence | Owner | Status |
|---|---|---|---|---|---|
| GA-GATE-M01 | **5 blog posts ready** | 5 blog posts drafted + edited + scheduled in `apps/docs/blog/` (or marketing CMS): (1) launch announcement, (2) architecture deep-dive, (3) lighthouse customer story, (4) security & BYOK, (5) performance & SLOs. Each reviewed by ≥ 2 reviewers; SEO meta + OG tags green. | `apps/docs/blog/` PRs; SEO lint | Owner | NOT_STARTED |
| GA-GATE-M02 | **Launch playbook drafted** | `marketing/launch/COORDINATION/LAUNCH-RUNBOOK.md` doc_status FROZEN; T-7..T+7 timeline detailed (Product Hunt slot booked, BusinessWire wire scheduled, Show HN slot, LinkedIn/Twitter scheduled posts); on-call paging plan integrated with SRE rotation. | LAUNCH-RUNBOOK.md frontmatter | Owner | NOT_STARTED |
| GA-GATE-M03 | **Press list confirmed** | Top-tier press contacts confirmed (TechCrunch, The Register, InfoQ, HN-active commentators); embargo letters drafted + signed by ≥ 5 journalists for T-3 d delivery; PR firm engagement (optional) or DIY plan documented. | Press list spreadsheet; embargo letter template | Owner | NOT_STARTED |
| GA-GATE-M04 | **Lighthouse customer quotes/case-studies ready** | 3 lighthouse customer quotes approved + 3 case-study one-pagers drafted (from `marketing/lighthouse-kit/case-study-template/`); each customer signs off on attributed quote (no surprises). | Customer-approved PDFs; quote approval emails | Owner + Product Lead | NOT_STARTED |
| GA-GATE-M05 | **Day-1 status communication template signed** | T-0 launch announcement template + T+1 retrospective template + SEV-1-during-launch comms template signed by Owner + Product Lead + SRE Lead; pre-staged in CMS / status page; cross-channel (blog, status, email, X/LinkedIn) consistency verified. | `marketing/launch/COMMS/`; sign-off log | Owner | NOT_STARTED |
| GA-GATE-M06 | **Brand assets ready** | Logo SVG + favicon + OG image + app screenshots high-res in `marketing/brand-assets/`; trademark filing initiated (USPTO + WIPO Madrid); brand guidelines doc current. | `marketing/brand-assets/` audit; USPTO filing receipt | Owner | NOT_STARTED |

**Marketing track sign-off:** Owner (acting as CEO + CMO) must confirm all 6 `READY`. Marketing track is the most waiver-friendly per spec contract §6.2 launch-orchestration **soft gate** — any of M01..M06 can be `WAIVED-with-Owner` to a Day-7 backfill without blocking engineering GA-go.

---

## 7. Waiver register

Active waivers (filled at Go/No-Go meeting; mirrored to `GA-GATE-GO-NOGO-TEMPLATE.md`).

| Waiver ID | Criterion | Granted by | Granted at | Compensating control | Expiry |
|---|---|---|---|---|---|
| _(none yet — populate at T-24 h meeting)_ | | | | | |

**Waiver policy.** Per `GA-GATE-GO-NOGO-TEMPLATE.md` §3: any waiver requires (1) **documented risk**, (2) **compensating control** (with link to runbook/CTRL), (3) **time-bound expiry** ≤ 30 d post-launch, (4) **sign-off by ≥ 1 VP+** (CEO / CTO / VPSec / VPProduct). Non-waivable criteria (see per-track sections): GA-GATE-O01, GA-GATE-C01, GA-GATE-L01, GA-GATE-S01 (HIGH/CRITICAL pentest), and all PRR-S20-GA §6 binary items.

---

## 8. Readiness summary (auto-rolled-up; fill 24 h pre-launch)

| Track | Total | READY | IN_PROGRESS | NOT_STARTED | BLOCKED | WAIVED | Verdict |
|---|---|---|---|---|---|---|---|
| Engineering | 15 | 0 | 0 | 15 | 0 | 0 | NOT_READY |
| Security | 12 | 0 | 0 | 12 | 0 | 0 | NOT_READY |
| Operations | 10 | 0 | 0 | 10 | 0 | 0 | NOT_READY |
| Customer | 8 | 0 | 0 | 8 | 0 | 0 | NOT_READY |
| Legal | 8 | 0 | 0 | 8 | 0 | 0 | NOT_READY |
| Marketing | 6 | 0 | 0 | 6 | 0 | 0 | NOT_READY |
| **TOTAL** | **59** | **0** | **0** | **59** | **0** | **0** | **NOT_READY** |

> Baseline at doc creation (2026-05-15) = all `NOT_STARTED`; the matrix is populated and re-rolled as each WI / wave converges. R-7 Sonnet support agents auto-update statuses against PRR-S20-GA + R-6 daily evidence digests.

---

## 9. Promotion gate semantics

- **GO (full GA)** — all 59 `READY` (or `WAIVED-with-VP+` with non-blocking justification); zero `NOT_STARTED` / `IN_PROGRESS` / `BLOCKED`.
- **GO-WITH-WAIVER** — ≤ 5 criteria `WAIVED` (each with VP+ sign-off + documented compensating control); the 4 non-waivable items (O01, C01, L01, S01-HIGH/CRITICAL) all `READY`.
- **DEFER** — any non-waivable item not `READY` **OR** > 5 total `WAIVED` **OR** any single track shows < 80% `READY`.

Decision is **unanimous-4-signer veto** per `GA-GATE-GO-NOGO-TEMPLATE.md` — any of CEO / CTO / VPSec / VPProduct can veto.

---

## 10. References

- `ROADMAP-TO-GA.md` §7 — Wave R-7 Evidence Gate.
- `specs/04_sprints/_sealed/S20/PRR-S20-GA.md` — global PRR with binary engineering DoD (cross-ref §5 of PRR-S20-GA).
- `specs/04_sprints/_sealed/S20/PRR-S20-CLOSING.md` — closing PRR with 8 TLA+ specs + final sign-off matrix.
- `specs/_compliance/SOC2-GAP-ANALYSIS.md` — GAP-XX backlog ≤ T+3 m post-GA.
- `specs/_compliance/BCP-DR-DRILL-CADENCE.md` — DR drill cadence + GA-GATE-O04 evidence source.
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — GA-GATE-O02 evidence source.
- `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` — failure-mode rollback if GA-GATE fails post-launch.
- `GA-GATE-GO-NOGO-TEMPLATE.md` — 60 min Go/No-Go meeting template; locks 4-signer veto decision.

---

## 11. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Sonnet R-7 builder, worktree `wt/r7-2-ga-gate`) | Initial GA Gate criteria checklist — 59 criteria across 6 tracks (Engineering 15 · Security 12 · Operations 10 · Customer 8 · Legal 8 · Marketing 6); concrete success metric + evidence link + owner + status per criterion; waiver register placeholder; promotion gate semantics (GO / GO-WITH-WAIVER / DEFER); cross-refs with PRR-S20-GA §5 DoD + §6 waivers + ROADMAP-TO-GA Wave R-7. |

---

**Status:** ACTIVE. Sign-off slots populated at T-24 h pre-launch Go/No-Go meeting per `GA-GATE-GO-NOGO-TEMPLATE.md`.
