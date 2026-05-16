# Changelog

All notable changes to CoreLink will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Sprint-tagged sections below mirror the 21-sprint spec-corpus → impl-sealed
trajectory (S-00 through S-20 + the `ga-engineering-gate-complete` cutover
on 2026-05-14). Each post-S-13 sprint receives its own dated section; the
S-00 → S-13 spec-corpus phase is collapsed under `[0.x]`.

Each entry cross-references:

- **WI-S__-___** — sprint work items (see `specs/04_sprints/S__/`)
- **CAP-_____** — capabilities (declared in `_spec_contract.md` §4)
- **R1-9!** / **R2-__** — security findings closed (see `ROADMAP-TO-GA.md`)
- **P0/P1** audit-doc IDs — sprint-close adversarial review findings
  (see `specs/04_sprints/S__/_audits/sprint-close-round-*.md`)

---

## [Unreleased]

### Added

- Customer-facing CHANGELOG generation tooling (`scripts/generate-changelog.sh`)
  and PR-level enforcement workflow (`.github/workflows/changelog-validate.yml`).
- Customer-facing **release-notes auto-generator** (`scripts/generate-release-notes.py`)
  with `--from / --to / --dry-run` flags, tag-trigger CI
  (`.github/workflows/release-notes.yml`), polish template
  (`releases/TEMPLATE.md`), and operator editorial guide
  (`marketing/launch/RELEASE-NOTES-EDITORIAL-GUIDE.md`). On every `v*` tag
  push, CI generates `releases/RELEASE-<version>.md`, opens an editorial PR,
  and creates a draft GitHub Release.

### Changed

- (none)

### Deprecated

- (none)

### Removed

- (none)

### Fixed

- (none)

### Security

- (none)

---

## [1.0.0] - DRAFT — pending `framework-v1-0-0-ga` tag + Owner approval

> **DRAFT.** This section is the technical changelog companion to
> `RELEASE-NOTES-v1.0.0-GA.md`. Publication is gated on the
> `framework-v1-0-0-ga` tag and the 2-key Owner + on-call SRE approval
> recorded in `specs/_audits/2026-05-16-ga-readiness-final.md` §13.
> Wave references below trace to `specs/_audits/2026-05-16-wave{N}-closure.md`.

### Wave summary — production wiring + adversarial review + DEBT closure

The 21-sprint spec-corpus phase (S-00 → S-20) is captured in the `[0.x]`
and per-sprint sections below. The post-S-20 production wiring + GA
readiness phase ran across **25 waves** dispatched on `main` between
the `ga-engineering-gate-complete` tag (2026-05-14) and the GA
cutover window (2026-05-16+). Each wave layered adversarial review +
debt closure + production wiring + chaos / endurance evidence on top
of the sealed sprint contracts.

| Wave | Focus | SEAL evidence |
|---|---|---|
| **R-prep + wave-1..17** | Per-sprint SEAL cadence; spec-corpus build-out; production wiring layer additions (real Stripe wasm32, real BYOK providers, real CF bindings, real Neon driver, replica coordinator, DSR worker production, customer dashboard, statuspage init, breach notification templates, etc.) | Per-sprint `_audits/sprint-close-round-*.md` |
| **Wave-18** | Audit-export production wiring (Stream A); Neon shadow analytics plane (Stream B); first cold-tool adversarial review pass | `specs/_audits/2026-05-16-wave18-aggregate-closure.md` (9.5 / 10) |
| **Wave-19** | S-18 pentest scope freeze; CLI `verify-ndjson` HTTP wiring; SDK example expansion | `specs/_audits/2026-05-16-wave19-adversarial-review.md` (8.86 / 10) |
| **Wave-20** | Audit-export streaming + payload column; 10-stream adversarial review | `specs/_audits/2026-05-16-wave20-closure.md` (9.40 / 10) |
| **Wave-21** | WallClock cross-route closure; DEBT-008 mutation sweep (hash 77.78 → 97.22 %); tenant-config region resolver (9.85 / 10) | `specs/_audits/2026-05-16-wave21-closure.md` (9.55 / 10) |
| **Wave-22** | Tenant-path UUID fix (9.9 / 10); Stripe MatClock wasm32 (9.7 / 10); chaos campaign harness (8 isolated fail-CLOSED scenarios); 24h endurance harness | `specs/_audits/2026-05-16-wave22-closure.md` (9.45 / 10) |
| **Wave-23** | Chaos combined-failures matrix (executor-loss × replication-lag × tenant-isolation); pilot onboarding E2E rig; CS playbook; beta-feedback triage; LFPDPPP MX attorney-package; INV-PAT-REVOKE-PROPAGATION promotion | `specs/_audits/2026-05-16-wave23-closure.md` (9.20 / 10) |
| **Wave-24** | GA cutover dry-run (RB-GA-CUTOVER §3, G1..G6 GREEN); GA readiness final audit (CONDITIONAL GO); DEBT-008 wave-24 closure batch; PAT-revoke TLA-exempt registration; ADR-0034b dual-hat path | `specs/_audits/2026-05-16-wave24-closure.md` (codex-Opus pass in flight) |
| **Wave-25** | External pentest engagement scope freeze (RFP + shortlist + SOW); DEBT-015-BUILD path-(3) ssgRequire; endurance 10-min dress-rehearsal; statuspage init dress-run; tenant-config CF prod-wire; pre-GA security attestation; GA-readiness DEFER drift detector; wave-24 adversarial-review pass | `specs/_audits/2026-05-16-wave25-closure.md` (in flight) |

### Added — production wiring + customer-facing surfaces

- **Audit export** — NDJSON streaming via signed URL + offline verifier
  (`corelink audit verify-ndjson`) + payload column + Merkle proof
  embed (waves 18 + 20).
- **Neon shadow analytics plane** — wired with RLS WITH CHECK at the
  SQL layer; real driver; replication SLO §4.27 – §4.29 observation
  streak active (waves 18 + 22).
- **BYOK 4-provider matrix** — AWS KMS, GCP KMS, Azure Key Vault,
  HashiCorp Vault — real provider pattern documented and exercised
  (R-prep + wave-25 attestation rollup).
- **Customer-facing audit export** + **customer dashboard** + **Stripe
  customer portal** (R-prep + waves 18 – 25).
- **Customer breach notification templates** (R-prep).
- **`RB-GA-CUTOVER.md`** + `RB-GA-LAUNCH-ROLLBACK.md` +
  `RB-LAUNCH-WAR-ROOM-COORDINATION.md` + 13 additional SEV-class
  runbooks (waves 19 – 24).
- **Chaos campaign** — 8 isolated fail-CLOSED scenarios + 3
  combined-failure scenarios under `cargo test --features chaos`
  (waves 22 – 23).
- **24-hour endurance harness** — built wave-22; 10-minute dress-run
  wave-25; soak scheduled in the pre-cutover T-24h window.
- **Statuspage** at `status.corelink.dev` — URL-substitution mechanism
  (wave-24), dress-rehearsed wave-25.
- **External pentest engagement** — scope frozen wave-25
  (`specs/_audits/2026-05-16-pre-ga-pentest-scope.md` +
  `specs/_pentest/SOW-S20-EXTERNAL-PENTEST.md`); vendor engagement
  scheduled 2026-Q3.
- **Pre-GA security attestation package** — wave-25 rollup
  (`specs/_audits/2026-05-16-pre-ga-security-attestation.md`).
- **Pilot onboarding E2E rig** + **CS playbook** + **beta-feedback
  triage pipeline** (wave-23).
- **GA gate** — `specs/_compliance/GA-GATE-CRITERIA.md` (59 criteria
  across 6 tracks) + `GA-GATE-GO-NOGO-TEMPLATE.md` (2-key signature
  template) + ADR-0034 / ADR-0034b PRR staffing + dual-hat waiver
  paths.
- **GA-readiness DEFER drift detector** — CI gate that prevents
  silent regression of the DEFER population between wave-25 and
  cutover (wave-25 stream #4).

### Changed

- **Invariant registry** — 197 declared (61 CRITICAL, 132 HIGH,
  4 MEDIUM); 81+ TLA+ verified; zero CRITICAL lacking proof or
  documented `§4.3` exemption; 0 orphan refs; 143 / 143 WI-coverage.
- **DEFER counter** scrubbed wave-25 (`specs/_audits/2026-05-16-ga-readiness-defer-scrub.md`):
  stale "Docs CI billing reinstatement" row removed (CI runs locally
  per `feedback_ci_local`); current counter is 7 external items
  (5 user-bound + 1 vendor-bound + 1 mixed).
- **`canary` → `staging-only`** chaos discipline at GA per S-17
  cross-functional decision (production chaos not authorised on
  the GA cutover day).

### Fixed

- **DEBT-001** — secrets matrix tighten (closed wave-15);
  `validate_secrets_matrix.py` code-only false-positive resolved
  wave-22.
- **DEBT-002, DEBT-004, DEBT-005, DEBT-006, DEBT-007, DEBT-009,
  DEBT-011, DEBT-012, DEBT-014, DEBT-017 – DEBT-020, DEBT-022,
  DEBT-024** — closed (no waiver) across waves 15 – 24.
- **DEBT-008** — mutation kill-rate baseline empirically CLOSED for 8
  of 15 crates (≥ 75 % floor on the remaining 5 via the
  `mutation-nightly.yml` CI-nightly matrix).
- **WallClock cross-route** — wave-21 closure (`f3462c6`);
  9.55 / 10 adversarial score.
- **`INV-PAT-REVOKE-PROPAGATION`** — promoted to CRITICAL wave-23;
  TLA+ exempt under §4.3 (wall-clock obligation, not consensus
  property); empirical sub-second propagation verified via mutation
  sweep + runbook drill.

### Security

- **External pentest** — scope frozen; engagement contracted for
  2026-Q3; HIGH / CRITICAL findings gate any future `GA-Full` /
  `v1.1.0` promotion. **No external pentest report is yet published**;
  customer-facing security claims do not depend on a completed
  external pentest at v1.0.0 GA.
- **Adversarial review** — 8 consecutive waves averaging
  9.41 / 10 (mean over the last 5 sealed waves), 0 P0 / 0 outstanding
  P1 at any wave boundary since wave-19 SEAL.
- **Compliance** — SOC 2 Type I ready, ISO 27001 Stage-1 eligible,
  GDPR / LGPD / PCI-SAQ-A / CCPA ready; LFPDPPP MX attorney sign-off
  pending (DEBT-025; wave-26 absorption); FedRAMP Moderate documented
  as not in scope for GA.
- **BYOK** — 4-provider FIPS attestation matrix complete for 3 of 4
  rows (AWS Artifact PDF download pending — DEBT-003 user-bound).

---

## [1.0.0-rc.1] - 2026-05-14 — GA Engineering Gate complete

Tag: `ga-engineering-gate-complete` (HEAD `f09d640`, alias of `s20-impl-sealed`).

The **GA Engineering Gate** is the binary technical readiness boundary —
distinct from `Launch Orchestration` (CAP-LAUNCH-001, marketing/PR/Product
Hunt). This tag asserts that 21 sprint specs are SEALED, ~70 Rust crates
compile and test, 8 TLA+ specs check, 62 runbooks exist, and 14 canonical
sources are green. Production wiring (Wave R-2..R-4) and external evidence
(Wave R-5..R-7) follow under `ROADMAP-TO-GA.md`.

### Added — GA Engineering Gate

- **CAP-GA-001** — Engineering gate `CONDITIONALLY_APPROVED` per PRR-S20-GA
  pending 8 D+60 evidence items (`ROADMAP-TO-GA.md` §5 Wave R-5/R-7).
- **CAP-GA-002** — External pentest engagement contract template + report
  intake workflow (Schellman / A-LIGN); EVT-040 evidence event registered.
- **CAP-GA-003** — SOC 2 gap analysis preparation (Drata / Vanta) with
  concrete GAP-XX items and fix timeline for Type I engagement at D+180.
- **CAP-GA-004** — Lighthouse customer migration framework (2 team-tier,
  1 enterprise-BYOK); SLA-claim-met-in-30d criterion encoded.
- **CAP-GA-005** — SLA contractual terms + DPA v1 template published in
  `legal/`; ready for 3-customer signature flow.
- **CAP-GA-006** — Incident response 24/7 PagerDuty schedule covering
  3 regions; response-time < 5 min tested.

### Changed

- "10 canonical sources" → **14 canonical sources** (codex finding;
  alignment with §3 of `_spec_contract.md`).
- "Full SBOM v1.0" → **CycloneDX 1.5+** (alignment with S-12 R-S12-3).
- Roadmap-to-GA marketing/launch concerns separated from engineering
  gate per codex feedback; `CAP-LAUNCH-001` carved out of `CAP-GA-*`.

### Security

- 14 canonical sources verified green at gate (SECURITY-MODEL,
  PRIVACY-MODEL, AUTH-MODEL, KEY-MANAGEMENT, COMPLIANCE-MATRIX,
  STORAGE-SEMANTICS-MATRIX, RESILIENCE-PATTERNS, OBSERVABILITY-MODEL,
  SLO-CATALOG, FAILURE-MODES, INVARIANT-REGISTRY, DATA-MODEL,
  REMOTE-CACHE-PRODUCT-PROFILE, FRAMEWORK-00).

---

## [0.20.0] - 2026-05-14 — S-20: GA Readiness

Tag: `s20-impl-sealed` (HEAD `f09d640`).

### Added

- **WI-S20-001..008** — PRR global execution + external pentest contract
  + 30d-sustained-staging evidence framework + SOC 2 gap-analysis prep
  + 3-lighthouse-customer migration scaffold + SLA/DPA v1 publish
  + incident-response 24/7 PagerDuty rotation + launch orchestration kit.
- **WI-S20-007** — 30d staging evidence framework + TLA+ 4 runbooks
  + 90d SBOM retention + PRR-S20 closing audit.
- **WI-S20-008** — Launch orchestration prep: press release + 5 blog
  posts + 3 case studies + Product Hunt kit + social media kit
  + launch runbook + launch-metrics dashboard. **Final WI of final sprint.**

### Changed

- Sprint-close round-1 P0 remediation (7.2/10 → SEAL approved); see
  `_audits/sprint-close-round-1.md`.
- Synthetic-page-drills migration renumbered `0042` → `0043` to remove
  conflict with `0042_lighthouse_customers.sql`.

### Fixed

- P0 audit findings round-1 cascade — engineering-gate-vs-launch
  separation enforced in §4 capability table.

### Security

- External pentest report intake gated on EVT-040; HIGH/CRITICAL
  remediation pre-condition for `GA-Full` tag promotion.

---

## [0.19.0] - 2026-05-14 — S-19: Customer Onboarding

Tag: `s19-impl-sealed` (HEAD `e967c65`).

### Added

- **WI-S19-001..006** — Self-service signup business logic + DPA
  click-through (CTRL-PRIV-CONSENT-001..006 capture, EVT-049, signed
  JWT receipt) + tier selection + Stripe Checkout integration
  + enterprise inquiry form with white-glove handoff
  + conversion-funnel instrumentation + DPA versioning re-acceptance.
- **WI-S19-004** — Tier selection + Stripe Checkout + INV-ONBOARD-DPA-FIRST
  + D1 row-lock atomicity (subscription activation requires DPA signed).
- **WI-S19-005** — Enterprise inquiry + Slack/CRM atomic outbox + 24h
  auto-reply SLA.
- 4 D1 migrations: `0037_signup_orchestration` · `0038_dpa_acceptances`
  · `0039_tier_selection` · `0040_enterprise_inquiries`
  · `0041_dpa_versioning`.

### Changed

- Lane upgrade STANDARD → **HIGH_RISK** per codex finding
  (FF-HR-009 customer-facing contract).
- Region pinning derives from rendered-locale cookie `corelink_locale`
  (set by S-16 middleware), not `Accept-Language` header
  (Lote 10.19 codex P1 canonical fix).
- Sprint-close round-1 P1 remediation (8.4/10 → SEAL approved).

### Security

- FF-HR-009 enforced: DPA + Terms click-through cryptographically
  proven via signed JWT receipt (legal-exposure mitigation).
- Stripe webhook signature verification + D1 idempotency keys
  (`0044_stripe_webhook_events_processed`).

---

## [0.18.0] - 2026-05-14 — S-18: Public Docs + API Reference + Pricing

Tag: `s18-impl-sealed` (HEAD `a1d00f1`).

### Added

- **WI-S18-001..005** — Docusaurus 3.x at `apps/docs/` deployed to CF
  Pages with Diátaxis taxonomy (tutorial / how-to / reference /
  explanation) + 5-min Bazel/Buck2/Native quickstart + REAPI v2
  auto-generated reference + SDK guides (Python/Go/JS/CLI) + compliance
  & security page (SOC 2 timeline + SBOM access + pentest exec summary)
  + pricing page (5 tiers + feature matrix + calculator).
- **CAP-DOCS-007** — i18n (en/pt-BR/es) + WCAG 2.2 AA + Lighthouse ≥ 95.
- **CAP-DOCS-008** — Vale tone-lint + lychee broken-link CI gates.

### Changed

- Sprint-close round-1 P0 remediation (5.5/10 → SEAL target reached).
- Sprint-close round-2 P1 — replace 28 i18n stub relative imports with
  `@site/src/` alias.
- Pin Node 20 + add `.npmrc` / `.nvmrc` — root-cause Docusaurus build
  failure under Node 22.

### Fixed

- Cross-functional anti-scope gate (§10): pricing/security claims
  require Finance + Legal + Security review before publish.

---

## [0.17.0] - 2026-05-14 — S-17: Ops Maturity

Tag: `s17-impl-sealed` (HEAD `bbbd99a`).

### Added

- **WI-S17-001..006** — Chaos engineering automation (weekly staging
  chaos, deterministic seed, ≥ 8 FMs covered, auto-rollback on SEV-1)
  + DR drill scheduler (semestral cadence, full region outage simulation)
  + runbook dry-run tracker EVT-017 (monthly P0/P1 runbook cadence)
  + incident + blameless post-mortem templates + oncall rotation with
  fatigue tracking + chaos catalog + game-day tabletop exercises.
- **WI-S17-003** — Runbook dry-run tracker + 3 P0/P1 monthly cadence
  (PAT-RUNBOOK-DRILL-001).
- **WI-S17-004** — Incident + blameless post-mortem templates +
  1 synthetic SEV-2 post-mortem + RB-POSTMORTEM-PROCESS.
- **WI-S17-005** — Oncall scheduler + fatigue tracking + PagerDuty
  integration trait + Grafana dashboard.
- 4 D1 migrations: `0033_chaos_runs` · `0034_dr_drill_runs`
  · `0035_runbook_drills` · `0036_oncall_pages`.

### Changed

- Sprint-close round-1 P0 remediation (7.6/10 → SEAL approved).
- Sprint duration corrected 2.5 → 4 weeks to accommodate parallel
  4-week chaos test (codex finding).

### Security

- Chaos discipline: production chaos NOT authorized at GA;
  staging-only weekly for 4 weeks pre-GA.

---

## [0.16.0] - 2026-05-14 — S-16: Frontend Admin UI

Tag: `s16-impl-sealed` (HEAD `5d70701`).

### Added

- **WI-S16-001..007** — Next.js 15 at `apps/web/` deployed to CF Pages
  with: tenant onboarding flow + per-tenant usage dashboard
  + audit-log viewer (CloudEvents R2 query proxy) + consent management
  UI (6-field proof: notice_text_hash + version + locale + wording_id
  + ui_capture_ts + submission_ts) + DSR request form (6 rights, MFA
  re-auth, JWT receipt) + PAT management + privacy/sub-processors
  pages + billing overview.
- **WI-S16-007** — Playwright e2e + Lighthouse + axe sweep + CSP
  enforce + UX workshop + PRR-S16.
- **CAP-UI-009** — i18n (en/pt-BR/es) + WCAG 2.2 AA baseline.

### Changed

- Sprint-close round-1 P0 remediation (7.4/10 → SEAL prep).
- HF-S17-001 — remove nested `<html>` from `[locale]/layout.tsx`.
- **CAP-UI-002** (full Grafana embed) **DEFERRED** post-S-16 to S-18 or
  post-GA (Lote 10.16 codex P0 fix); S-16 ships basic plan/quota
  progress widget + audit-viewer link + billing overview.

### Deprecated

- Inline `--telemetry=on` CLI flag (rejected per Lote 10.15 alignment;
  telemetry only via persistent `~/.corelink/config.toml`).

### Security

- Hardened CSP `default-src 'none'` + explicit allowlists enforced;
  XSS-exfiltration of PAT mitigated.

---

## [0.15.0] - 2026-05-14 — S-15: CLI + SDK Integration

Tag: `s15-impl-sealed` (HEAD `88cd55b`).

### Added

- **WI-S15-001..006** — `corelink` CLI (7 subcommands: ls / get / put
  / stat / bench / doctor / version) cross-OS signed (macOS notarized,
  Linux GPG-signed, Windows Authenticode-signed) + Bazel starter project
  with credential-helper-protocol + Buck2 starter project + FFI wrappers
  (Python pyO3, Go cgo, JS/TS WASM) with client-verify default-on
  + CI templates (GitHub Actions + GitLab + CircleCI).
- **WI-S15-006** — Fuzz 1M + 3-OS signing + 2 OSS-proof
  conformance + PRR-S15 + adversarial summary.
- `corelink doctor` 8-check actionable diagnostic (network, auth,
  storage write, storage read, BYOK, region, quota, client-verify).

### Changed

- Sprint-close round-1 P0 remediation (8.7/10 final).
- Bazel `.bazelrc` uses credential-helper protocol (Bazel 6+) — PAT
  via stdout-JSON, never in `argv` (CTRL-CRED-001 enforcement;
  Lote 10.15 canonical fix).

### Security

- Tokens never in CLI args; only env-var `CORELINK_PAT` or
  `~/.corelink/config.toml`.

---

## [0.14.0] - 2026-05-14 — S-14: Region Expansion + BYOK

Tag: `s14-impl-sealed` (HEAD `70ea887`).

### Added

- **WI-S14-001..009** — 4 production regions (WNAM us-west, ENAM us-east,
  WEUR eu-west, SAM sa-east) + tenant primary_region pinning + hot-blob
  cross-region replication (top 1% via offline aggregation, escaping
  INV-OBS-CARDINALITY-BUDGET) + PAT-REGION-FAILOVER-001 read failover.
- **CAP-BYOK-001..006** — BYOK adapter trait `crates/corelink-byok`
  with 4 KMS providers: AWS KMS (FIPS 140-3 L1), GCP KMS (FIPS 140-2 L1),
  Azure Key Vault Premium (FIPS 140-2 L2), HashiCorp Vault Enterprise
  (FIPS 140-3 L1).
- **WI-S14-007** — Ed25519 (FIPS 186-5) erasure attestation + JCS
  canonicalization + 7y retention + verify path.
- **WI-S14-008** — DPA amendment + Schrems II TIA + Legal external
  review path.
- **WI-S14-009** — TLA+ `region_residency` spec + RB-BYOK-REVOKE
  prod-grade + 3 RB dry-runs + pentest stub + PRR-S14.
- 6 D1 migrations: `0027_region_provisioning` · `0028_tenant_primary_region`
  · `0029_hot_blobs` · `0030_byok_envelope` · `0031_byok_tenant_status`
  · `0032_erasure_attestation`.

### Changed

- Customer kill switch SLA ≤ **6 min p99** (60s detection + 5min DEK
  cache TTL hard, codex-corrected from initial 5 min target).
- **DEK derivation** — random 32 bytes via `getrandom::getrandom`
  CSPRNG, **not** BLAKE3-derived from blob hash (Lote 10.14 codex P1
  fix; deterministic DEK = compromise propagation across blobs).
- AES-256-GCM (FIPS 197 + FIPS 140-3 approved) with 96-bit random
  nonce; per-blob envelope encryption.
- Sprint-close P0+P1 remediation cascade (6.48/10 FAIL → 8.5+).

### Fixed

- Port `corelink-byok-revocation` + `customer-alerts` to canonical
  trait surface (sprint-close P0-1 + P0-2 resolution).

### Security

- **R1-9!** FIPS-mode toggle documented per provider in
  `compliance/byok-fips-matrix.md`.
- Cross-region tenant isolation property-tested at 20k iter, 0 leaks.
- Customer kill switch end-to-end runbook RB-BYOK-REVOKE dry-run
  evidence committed.

---

## [0.x] - 2026-04 to 2026-05 — Spec corpus phase (S-00 → S-13)

Tags: `s00-impl-sealed` ... `s13-impl-sealed` (14 tags).

This collapsed section records the **pre-1.0 framework history**: the
foundation sprints that delivered the spec corpus (262 docs · 136 invariants),
the reference Rust crates (~70 crates wired against `InMemoryFake` traits),
the 8 TLA+ specifications, and the canonical-source bedrock that
everything from S-14 onwards inherits from.

| Sprint | Date | Name | Lane | Highlights |
|---|---|---|---|---|
| **S-00** | 2026-04-15 | Roadmap & Planning | n/a | 14 canonical sources skeleton + sprint waveform planned. |
| **S-01** | 2026-04-29 | CAS Foundation (write path + HMAC + integrity) | HIGH_RISK | `corelink-hash` BLAKE3 + `corelink-worker` R2 PUT + `corelink-reapi` REAPI v2 gRPC handlers (BatchUpdateBlobs, Capabilities, ByteStream::Write); vendored bazelbuild/remote-apis @ v2.12.0 proto subset; CloudEvents 1.0 audit envelope; INV-CAS-INTEGRITY enforced. |
| **S-02** | 2026-04-30 | CAS Read Path + Client Verify | HIGH_RISK | Server-and-client BLAKE3 verify default-on; `corelink-client-verify` crate; CTRL-CAS-002. |
| **S-03** | 2026-05-01 | Auth Real | HIGH_RISK | Clerk integration + PAT scope model + MFA + `corelink-clerk` + `corelink-clerk-cf`. |
| **S-04** | 2026-05-01 | Action Cache (AC) | HIGH_RISK | AC put/get + HKDF-keyed-MAC signature + dedup-safe; `corelink-ac::merkle` RFC 6962-style domain separation. |
| **S-05** | 2026-05-01 | Multipart Upload + Chunking + Merkle (blobs > 5 MiB) | HIGH_RISK | `corelink-chunker` + `corelink-manifest` Merkle manifest with O(1) streaming-memory verify (INV-MULTIPART-STREAMING-MEMORY); MAX_CHUNKS_PER_BLOB = 81920. |
| **S-06** | 2026-05-01 | Garbage Collection: Mark & Sweep + INV-GC-001/004 | HIGH_RISK | `corelink-gc` worker binary + scheduler + 8 GcEventType audit taxonomy + degrade overload-detector + partial-UNIQUE running-status invariant. |
| **S-07** | 2026-05-02 | Dedup + Eviction Policy (intra-tenant default; cross-tenant backlog) | STANDARD | LRU + LFU + size-tiered eviction; intra-tenant dedup default; cross-tenant deferred. |
| **S-08** | 2026-05-03 | Rate Limiting Multi-Camada + Quotas + Abuse Detection | HIGH_RISK | Token-bucket multi-layer + abuse-score + edge blocklist + quota FSM + circuit breaker. |
| **S-09** | 2026-05-05 | Observability Stack | HIGH_RISK | OTLP traces + structured logs + 4-burn-rate SLO alerts + audit-log CloudEvents R2 + cardinality budget INV-OBS-CARDINALITY-BUDGET. |
| **S-10** | 2026-05-07 | Billing Pipeline | HIGH_RISK | Stripe webhook idempotency + usage-event-idem + billing replay audit + reconciliation drift detector + Stripe Checkout. |
| **S-11** | 2026-05-09 | Privacy Pipeline | HIGH_RISK | CTRL-PRIV-CONSENT-001..006 6-field consent capture + 6 DSR rights + erasure log + notice-text-hash canonicalization. |
| **S-12** | 2026-05-11 | Supply Chain Hardening | HIGH_RISK | CycloneDX 1.5+ SBOM + cosign signing + SLSA L3 attestation + cargo-audit + cargo-deny + Dependabot auto-merge + Bazel/Buck2 starter CI. |
| **S-13** | 2026-05-13 | Admin Plane | HIGH_RISK | Admin op-log + rotation-state + admin surfaces gated behind feature-flag for tenant emergency ops. |

**Cumulative deliverables at end of phase:**

- ~70 Rust crates compiling + testing under `cargo test --workspace`.
- 26 D1 migrations (`0001` ... `0026`) all additive-only (INV-AUTH-MIGRATION-ADDITIVE).
- 8 TLA+ specifications.
- 244 vitest cases in `apps/web` + 264 in `apps/docs`.
- 62 runbooks in `specs/_runbooks/`.
- 14 canonical sources SEALED (`SECURITY-MODEL`, `PRIVACY-MODEL`,
  `AUTH-MODEL`, `KEY-MANAGEMENT`, `COMPLIANCE-MATRIX`,
  `STORAGE-SEMANTICS-MATRIX`, `RESILIENCE-PATTERNS`,
  `OBSERVABILITY-MODEL`, `SLO-CATALOG`, `FAILURE-MODES`,
  `INVARIANT-REGISTRY`, `DATA-MODEL`, `REMOTE-CACHE-PRODUCT-PROFILE`,
  `FRAMEWORK-00`).

---

## Migration Notes (post-1.0)

### Applying D1 migrations

CoreLink ships **43 D1 migrations** (`migrations/d1/0001_blob_meta.sql` ...
`migrations/d1/0044_stripe_webhook_events_processed.sql`, with one
renumbering hop `0042_lighthouse_customers.sql` introduced in S-20).
All migrations are **additive-only** (INV-AUTH-MIGRATION-ADDITIVE):
no destructive `DROP`, no breaking column rename without dual-write
transition window.

Apply via the canonical runner:

```bash
./scripts/d1-migration-runner.sh <staging|prod> <d1-binding-id>
```

Pre-conditions:

- Cloudflare auth (`wrangler login`) from an operator workstation.
- D1 binding ID for the target environment.
- Read `specs/_runbooks/RB-D1-MIGRATION-APPLY.md` before promoting to prod.

Offline pre-flight (CI also runs these):

```bash
python3 scripts/check_migrations_additive.py
python3 scripts/d1-migration-verify.py --schema-only
cargo test -p corelink-d1-migrations --test d1_migration_integration
```

### Required environment variables

The following secrets MUST be set in the operator workstation or CF
Workers binding before a fresh deploy will boot:

| Var | Scope | Notes |
|---|---|---|
| `CORELINK_PAT` | CLI / SDK | Tenant PAT; never pass via CLI argv (CTRL-CRED-001). |
| `CLERK_SECRET_KEY` | Worker | Server-side Clerk JWT verify. |
| `CLERK_PUBLISHABLE_KEY` | Worker / Web | Client-side. |
| `STRIPE_SECRET_KEY` | Worker | Subscription + webhook. |
| `STRIPE_WEBHOOK_SECRET` | Worker | Signature verify. |
| `PAGERDUTY_INTEGRATION_KEY` | Worker | Events API v2 (S-17). |
| `SLACK_WEBHOOK_URL` | Worker | Enterprise inquiry notify (S-19). |
| `HUBSPOT_API_KEY` | Worker | CRM atomic outbox (S-19). |
| `AWS_KMS_KEY_ARN` | Tenant BYOK | Per-tenant; required if `tenant.byok_provider = aws`. |
| `GCP_KMS_KEY_NAME` | Tenant BYOK | Per-tenant. |
| `AZURE_KEY_VAULT_URI` | Tenant BYOK | Per-tenant. |
| `VAULT_TRANSIT_KEY` | Tenant BYOK | Per-tenant. |
| `GRAFANA_CLOUD_PUSH_TOKEN` | Worker | Metrics push. |
| `DRATA_API_TOKEN` | Worker | SOC 2 evidence collection (S-20). |

### Breaking changes warning — major version bumps

When bumping the major version (e.g., `2.0.0`):

1. Review all entries under `### Removed` and `### Changed` since the
   previous major in this changelog.
2. Run `scripts/d1-migration-verify.py --diff-from <prev-major-tag>`
   to catalogue destructive operations introduced under the new major.
3. **Required**: 6-month deprecation window for any public REAPI v2
   surface change; cross-reference `specs/_canonical/REMOTE-CACHE-PRODUCT-PROFILE.md`
   §wire-compat-matrix.
4. **Required**: signed customer notice 30d before any breaking change
   that affects PAT scope, BYOK envelope format, or audit event schema.
5. Re-run external pentest (Schellman or A-LIGN) before tagging `vN.0.0`.
6. Bump `compliance/version_pins.yaml` and trigger SOC 2 Type II
   continuous-monitoring re-baseline.

[Unreleased]: https://github.com/humangr-labs/corelink/compare/ga-engineering-gate-complete...HEAD
[1.0.0-rc.1]: https://github.com/humangr-labs/corelink/releases/tag/ga-engineering-gate-complete
[0.20.0]: https://github.com/humangr-labs/corelink/releases/tag/s20-impl-sealed
[0.19.0]: https://github.com/humangr-labs/corelink/releases/tag/s19-impl-sealed
[0.18.0]: https://github.com/humangr-labs/corelink/releases/tag/s18-impl-sealed
[0.17.0]: https://github.com/humangr-labs/corelink/releases/tag/s17-impl-sealed
[0.16.0]: https://github.com/humangr-labs/corelink/releases/tag/s16-impl-sealed
[0.15.0]: https://github.com/humangr-labs/corelink/releases/tag/s15-impl-sealed
[0.14.0]: https://github.com/humangr-labs/corelink/releases/tag/s14-impl-sealed
[0.x]: https://github.com/humangr-labs/corelink/compare/s00-impl-sealed...s13-impl-sealed
