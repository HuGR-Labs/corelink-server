---
id: "DEBT-REGISTER-2026-05-15"
type: "compliance_evidence_rollup"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["debt-register", "techlead", "ga-readiness", "must-close-before-tag"]
---

# Debt Register — 2026-05-15

> **Mandate (user, 2026-05-15):** *"Nao deixamos debitos aqui, nao se esqueca disso."*
>
> This file enumerates every known debt surfaced by the 8 R-wave dispatches. Each row has owner + plan + target close date. Nothing in this register is allowed to drift past its target without explicit waiver in this file. New debts surfaced in future waves append here, never to scattered audit docs.
>
> **Snapshot commit:** main HEAD `6d80530` + wave 7 + wave 8 in flight.
> **Total debt rows:** 19 (P0:3 · P1:8 · P2:8).

---

## 1. Hard P0 — must close before any sprint tag or roadmap milestone

| ID | Title | Source | Owner | Target | Plan |
|---|---|---|---|---|---|
| **DEBT-001** | **19 code-only secrets drift entries** (k6 load-test env vars, Vault auth-modes `VAULT_KUBERNETES_*`, Drata onboarding `DRATA_INVITE_*`, Azure federation `AZURE_FEDERATION_*`) — all reference real env vars in code but not in `docs/internal/secrets-checklist.md`. | `specs/_audits/2026-05-15-secrets-coverage-baseline.md` (commit `c000dd8`) | Orchestrator | T+3d (2026-05-18) | Dispatch 1 Sonnet to extend secrets-checklist.md with the 19 missing entries (category, owner, rotation cadence, where consumed). Re-run validator → 0 code-only. |
| **DEBT-002** | **OSS release prep blocked** — content-filter blocked twice on standard Apache 2.0 / MIT / Contributor Covenant text (agents `abcaa7d43f0040ca4`, agent v2). LICENSE files + per-crate `license =` tags + CONTRIBUTING + CoC + DCO CI all still missing. Blocks public-launch (LAUNCH-CHECKLIST-V2 L23 "OSS repos public"). | Wave 6 + Wave 7 OSS prep agents | Orchestrator (inline; no agent) | T+5d (2026-05-20) | Orchestrator writes LICENSE-APACHE-2.0 + LICENSE-MIT + CONTRIBUTING.md + CODE_OF_CONDUCT.md + DCO CI workflow + per-crate Cargo.toml `license = "MIT OR Apache-2.0"` directly — no agent dispatch (no agent can produce verbatim legal text without filter). |
| **DEBT-003** | **AWS attestation doc hash placeholder** — `BYOK-FIPS-ATTESTATION-MATRIX.md` row for AWS KMS shows `TBD-on-receipt` for the attestation PDF SHA-256. Must be filled when AWS Artifact PDF is downloaded. Blocks GAP-02 closure. | `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` (commit `eed34ab`) | Gustavo (Human) | T+30d (2026-06-14, per GAP-02 hard cap) | Human downloads AWS Artifact SOC 2 + FIPS attestation PDF, runs `sha256sum`, updates matrix row. |

---

## 2. P1 — must close before R-7 GA Gate

| ID | Title | Source | Owner | Target | Plan |
|---|---|---|---|---|---|
| DEBT-004 | **15 orphan INV refs** in canonical-lint baseline (BACKUP/ROLLOUT/AC/AUTH/AVAIL forward-looking IDs cited in code/tests/specs without matching `invariant_registry.md` entries). | `specs/_audits/2026-05-15-canonical-consistency-baseline.md` (commit `ddf4c10`) | Orchestrator | T+10d (2026-05-25) | Dispatch 1 Sonnet to promote (add to registry) or remove (orphan citation in code). Re-run canonical lint → 0 orphan. |
| DEBT-005 | **40 critical INVs without TLA model** — canonical-lint baseline shows 40 INVs marked CRITICAL severity but no TLC verification. | `specs/_audits/2026-05-15-canonical-consistency-baseline.md` | Orchestrator | T+30d (2026-06-14) | Wave 6 closed 1 (auth_revocation); 9 followup tickets exist. Need to dispatch 4-8 builders writing TLA+ models for remaining critical INVs (audit ordering, dual-approval, BYOK envelope AAD binding, etc.). Target ≥ 50% closure pre-GA gate. |
| DEBT-006 | **129 pre-existing dangling references** (validate_references baseline). [CLOSED 2026-05-15: actual baseline 269 → 0 via wt/debt-006-dangling-refs; see `specs/_audits/2026-05-15-dangling-refs-closure.md`.] | First surfaced in wave 4 R6-2 runbook completeness audit (commit `975d635`). | Orchestrator | T+14d (2026-05-29) | **CLOSED.** Validator now resolves 269 prior references via: (a) added `_runbooks/` + `05_runbooks/` to DEFINITION_SOURCES, (b) added `**ID** (` anchor pattern for SLO catalog, (c) bulk allowlist additions for PLANNED forward-looking IDs + NOISE template placeholders. Net delta: 269 → 0. |
| DEBT-007 | **R4-6 backlog: 56 untranslated source files** in pt-BR/es-419 → coverage stuck at 64.1% (target ≥ 80%). Pre-existing predates wave 7 i18n DE. | `wt/r-prep-i18n-de` commit `1fb2dc4` CAVEATS | Orchestrator | T+14d (2026-05-29) | Dispatch 1 Sonnet to MT-stub seed the 56 source files via existing TMX; bring pt-BR + es-419 to ≥ 80%. |
| DEBT-008 | **3 mutation testing crates with full sweep deferred** (audit-chain, pat, clerk — targeted tests written but full `cargo mutants` run not measured). | `wt/r-prep-mutants-expansion` commit `699ca0b` CAVEATS | Orchestrator | T+21d (2026-06-05) | First CI nightly run will land empirical baselines. Monitor; if any crate < 75% kill rate post-baseline, dispatch fix agent. |
| DEBT-009 | **4 remaining proptest density gap crates** (slack-real / admin-dry-run / cf-bindings / d1-migrations — ratio < 1 declared invariants per proptest). | `specs/_audits/proptest-followup-tickets.md` (commit `ab289ef`) | Orchestrator | T+21d (2026-06-05) | Dispatch 1 Sonnet to close all 4 in parallel (each ~3-5 proptests). |
| DEBT-010 | **11 CI optimization tickets** pending — 4 P1 + 4 P2 + 3 P3 from audit. ~52 min/PR savings projected. | `specs/_audits/ci-optimization-followup-tickets.md` (commit `2378dfe`) | Orchestrator | T+21d (2026-06-05) | Dispatch 1 Sonnet to implement P1 tickets (highest-ROI; ~30 min savings). P2/P3 deferred to post-GA. |
| DEBT-011 | **10 replication followup tickets** (3 P0 + 4 P1 + 3 P2) — RPO measurement gaps + Prometheus emit wiring for SLO-REPLICATION-LAG-*. | `specs/_audits/replication-followup-tickets.md` (commit `ed36bcd`) | Orchestrator | T+30d (2026-06-14) | Dispatch 1 Sonnet on the 3 P0 (RPO target unmet); P1/P2 with handler-crate skeleton (wave 7 in flight). |

---

## 3. P2 — must close before GA Limited

| ID | Title | Source | Owner | Target | Plan |
|---|---|---|---|---|---|
| DEBT-012 | **7 ISO 27001-unique gaps** + 1 informational PIMS deferred. Q1-2027 cert target. | `specs/_compliance/ISO27001-GAP-ANALYSIS.md` (commit `3ac2998`) | Orchestrator | T+90d (2026-08-13) | Closure work scheduled across Q3-2026 per ISO27001-ROADMAP.md. No immediate action; track in compliance weekly digest. |
| DEBT-013 | **10 perf optimization tickets** (top-5 hot spots + 5 minor; projected 3-10% p99 reduction). | `specs/_audits/perf-optimization-followup-tickets.md` (commit `3b4cbfc`) | Orchestrator | T+60d (2026-07-14) | Dispatch as separate WIs in R-3/R-4 integration waves. Not blocking GA but blocking GA-Full. |
| DEBT-014 | **9 TLA+ followup tickets** (FT-1..FT-9) covering remaining critical INVs + medium-severity. | `specs/_audits/tla-followup-tickets.md` (commit `992fcee`) | Orchestrator | T+60d (2026-07-14) | Cross-reference with DEBT-005; consolidate effort. |
| DEBT-015 | **`apps/docs` Node 22 build error** pre-existing on origin/main (`@theme/prism-include-languages` ESM `ERR_MODULE_NOT_FOUND` on Node 22; engine pin `<22`; CI uses Node 20). Surfaced by 5+ wave-1 thru wave-6 docs agents. | Multiple agent caveats (R3-7, R4-1, Trust Center, API Reference, etc.) | Orchestrator | T+21d (2026-06-05) | Dispatch 1 Sonnet to upgrade the Docusaurus theme dep or pin to Node-22-compatible variant. Low-risk. |
| DEBT-016 | **Statuspage URLs placeholder** — `status.corelink.dev` cited in 8+ trust pages but not live yet. | `wt/r-prep-statuspage-config` commit `a9de271` CAVEATS | Gustavo (Human) | T-7d pre-launch | Operator follows STATUSPAGE-INIT.md provisioning playbook. |
| DEBT-017 | **5 followup proptest WIs** (3.5d total effort) for low-priority gap crates. | `specs/_audits/proptest-followup-tickets.md` | Orchestrator | T+30d (2026-06-14) | Bundle with DEBT-009 closure. |
| DEBT-018 | **CodeQL/Semgrep upstream action SHAs are placeholder pins** (v3.27.0 / v1 tag references; need verified SHAs after first green run). | `wt/r-prep-codeql-semgrep` commit `a280fe1` CAVEATS | Orchestrator | T+7d (2026-05-22) | After first nightly green run, use `gh api` to pin to actual SHAs. Runbook §6 documents procedure. |
| DEBT-019 | **GitHub action SHAs not cross-verified against API** for wave 7/8 bot workflows. | `wt/r-prep-github-bots` commit `da4b473` CAVEATS | Orchestrator | T+7d (2026-05-22) | Single `gh api` cross-check + pin if drift. |

---

## 4. Closure tracking

This register IS the source of truth. The compliance weekly digest (`scripts/compliance-weekly-digest.py`) MUST read this file weekly and flag any row past its target without explicit waiver.

Waivers are NOT silent. If a target slips, append a row to `Section 5 — Waivers` below with justification + new target + sign-off (`Gustavo Schneiter` for P0/P1; for P2 a senior engineer suffices).

---

## 5. Waivers

(empty at register creation — 2026-05-15)

---

## 6. Cross-references

- `/techlead` skill v2.0.0 — Anti-pattern AP-5: "Letting validate_specs failures linger 'out of scope'". This register is the explicit institutional defense.
- ROADMAP-TO-GA.md §7 R-7 Gate — All P0+P1 debt rows MUST be closed (or waived in §5 here) before tagging GA-Engineering-Gate-Complete-V2.
- `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` — GAP register is the SOC-2-bound subset; this debt register is the superset (includes infra/quality/dev-velocity debts not flagged to SOC 2 auditor).
- `specs/_compliance/weekly-digests/README.md` — compliance digest reads this register every Monday.

## 7. Change log

| Date | Change | Author |
|---|---|---|
| 2026-05-15 | v1.0.0 — Register created post wave-7+wave-8 dispatch; 19 debts catalogued (P0:3 / P1:8 / P2:8). | Gustavo (via Claude Opus 4.7 orchestrator) |
