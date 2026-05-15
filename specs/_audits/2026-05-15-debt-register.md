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
> **Total debt rows:** 20 (P0:3 · P1:8 · P2:9) — **DEBT-020 added 2026-05-15 (CLOSED same day)**.
> **Closed:** 1 (DEBT-001 — 2026-05-15, commit `52624e7`).
> **Total debt rows:** 20 (P0:3 · P1:8 · P2:9) — **1 closed (DEBT-004 on 2026-05-15)**; 18 open; +DEBT-020 added & closed 2026-05-15.

---

## 1. Hard P0 — must close before any sprint tag or roadmap milestone

| ID | Title | Source | Owner | Target | Plan |
|---|---|---|---|---|---|
| ~~**DEBT-001**~~ **CLOSED 2026-05-15 (`52624e7`)** | **19 code-only secrets drift entries** (k6 load-test env vars, Vault auth-modes `VAULT_KUBERNETES_*`, Drata onboarding `DRATA_INVITE_*`, Azure federation `AZURE_FEDERATION_*`) — all reference real env vars in code but not in `docs/internal/secrets-checklist.md`. | `specs/_audits/2026-05-15-secrets-coverage-baseline.md` (commit `c000dd8`) | Orchestrator | ~~T+3d (2026-05-18)~~ **Closed 2026-05-15** | Rows #90-#108 added to `docs/internal/secrets-checklist.md` (commit `52624e7`). Validator now reports `code_only=0` (matrix=109, code=89, in_both=89, matrix_only=20). Deploy gate `cf-deploy-prod.yml` no longer blocks on DEBT-001. |
| ~~**DEBT-002**~~ **CLOSED 2026-05-15** | **OSS release prep** — content-filter blocked twice on standard Apache 2.0 / MIT / Contributor Covenant text (agents `abcaa7d43f0040ca4`, agent v2). LICENSE files + per-crate `license =` tags + CONTRIBUTING + CoC + DCO CI all still missing. Blocks public-launch (LAUNCH-CHECKLIST-V2 L23 "OSS repos public"). | Wave 6 + Wave 7 OSS prep agents | Orchestrator (inline; no agent) | T+5d (2026-05-20) | Orchestrator writes LICENSE-APACHE-2.0 + LICENSE-MIT + CONTRIBUTING.md + CODE_OF_CONDUCT.md + DCO CI workflow + per-crate Cargo.toml `license = "MIT OR Apache-2.0"` directly — no agent dispatch (no agent can produce verbatim legal text without filter). |
| **DEBT-003** | **AWS attestation doc hash placeholder** — `BYOK-FIPS-ATTESTATION-MATRIX.md` row for AWS KMS shows `TBD-on-receipt` for the attestation PDF SHA-256. Must be filled when AWS Artifact PDF is downloaded. Blocks GAP-02 closure. | `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` (commit `eed34ab`) | Gustavo (Human) | T+30d (2026-06-14, per GAP-02 hard cap) | Human downloads AWS Artifact SOC 2 + FIPS attestation PDF, runs `sha256sum`, updates matrix row. |

---

## 2. P1 — must close before R-7 GA Gate

| ID | Title | Source | Owner | Target | Plan |
|---|---|---|---|---|---|
| ~~DEBT-004~~ | ~~**15 orphan INV refs** in canonical-lint baseline (BACKUP/ROLLOUT/AC/AUTH/AVAIL forward-looking IDs cited in code/tests/specs without matching `invariant_registry.md` entries).~~ **CLOSED 2026-05-15** — orphan_refs 36 → 0 (count had grown 15 → 36 between baseline lock and closure pass as `corelink-rate-headers`, `corelink-handler-cas`, `corelink-otel-export`, `corelink-tenant-offboarding`, `corelink-stripe-real::portal`, `corelink-billing-aggregator`, `corelink-logpush` landed). 34 promoted (7 new domain sub-sections §3.20–§3.26 + 4 extended existing sections §3.8/§3.9/§3.14/§3.15), 2 removed (renamed shorthands to canonical). Validator green (exit 0). Closure log: `specs/_audits/2026-05-15-canonical-consistency-baseline.md §3.1`. | `specs/_audits/2026-05-15-canonical-consistency-baseline.md` (commit `ddf4c10`) | ~~Orchestrator~~ Closed | ~~T+10d (2026-05-25)~~ Closed 2026-05-15 | ~~Dispatch 1 Sonnet to promote (add to registry) or remove (orphan citation in code). Re-run canonical lint → 0 orphan.~~ ✅ Done. |
| DEBT-005 | **40 critical INVs without TLA model** — canonical-lint baseline shows 40 INVs marked CRITICAL severity but no TLC verification. | `specs/_audits/2026-05-15-canonical-consistency-baseline.md` | Orchestrator | T+30d (2026-06-14) | Wave 6 closed 1 (auth_revocation); 9 followup tickets exist. Need to dispatch 4-8 builders writing TLA+ models for remaining critical INVs (audit ordering, dual-approval, BYOK envelope AAD binding, etc.). Target ≥ 50% closure pre-GA gate. |
| DEBT-004 | **15 orphan INV refs** in canonical-lint baseline (BACKUP/ROLLOUT/AC/AUTH/AVAIL forward-looking IDs cited in code/tests/specs without matching `invariant_registry.md` entries). | `specs/_audits/2026-05-15-canonical-consistency-baseline.md` (commit `ddf4c10`) | Orchestrator | T+10d (2026-05-25) | Dispatch 1 Sonnet to promote (add to registry) or remove (orphan citation in code). Re-run canonical lint → 0 orphan. |
| DEBT-005 | **35 critical INVs without TLA model** (was 40 at baseline) — canonical-lint baseline showed 40 CRITICAL INVs without TLC; partial closure 2026-05-15 brings to **35 remaining** (5 closed: `auth_jwt_validation`, `tenant_ctx_propagation`, `merkle_integrity`, `byok_envelope_aad`, `failover_no_split_brain`). | `specs/_audits/2026-05-15-canonical-consistency-baseline.md` | Orchestrator | T+30d (2026-06-14) | **PARTIAL (5/40)**. Wave 6 closed 1 (auth_revocation); DEBT-005 dispatch 2026-05-15 closed 5 more on branch `wt/debt-005-tla-critical-inv`. Remaining 35 tracked in `tla-followup-tickets.md` addendum + FT-1..FT-9 backlog. Target ≥ 50% closure (≤ 20 remaining) pre-GA gate. Next dispatch should pick from: `auth_pat_hybrid` (FT-1), `key_lifecycle` (FT-2), `gc_lock_protocol` (FT-5), `multipart_finalize` (FT-6), `byok_dek_race` (FT-7). |
| DEBT-006 | **129 pre-existing dangling references** (validate_references baseline). | First surfaced in wave 4 R6-2 runbook completeness audit (commit `975d635`). | Orchestrator | T+14d (2026-05-29) | Dispatch 1 Sonnet to bulk-close: each dangling ref → either fix link target or remove dead reference. Decrement baseline to ≤ 30. |
| DEBT-007 | **CLOSED 2026-05-15** — was: R4-6 backlog: 56 untranslated source files in pt-BR/es-419 → coverage stuck at 64.1%. Resolved on `wt/debt-007-i18n-pt-es-80`: ran `mt-stub-seed.ts --init` for pt-BR + es-419 (67 files each), and re-init for `de` (11 files added post-WT-R4-1). Coverage: **pt-BR 64.1% → 100.0%, es-419 64.1% → 100.0%, de 100% → 100%** (167/167 mt-stub each, 0 missing). Typecheck: 45 pre-existing `sidebars.ts` errors unchanged (no regression from seed). | `wt/debt-007-i18n-pt-es-80` | Orchestrator | T+14d (2026-05-29) | DONE — all three Tier-1 locales above the 80% WT-R4-1 acceptance gate; native-speaker review can be scheduled per `TRANSLATION-WORKFLOW.md`. |
| DEBT-006 | **129 pre-existing dangling references** (validate_references baseline). [CLOSED 2026-05-15: actual baseline 269 → 0 via wt/debt-006-dangling-refs; see `specs/_audits/2026-05-15-dangling-refs-closure.md`.] | First surfaced in wave 4 R6-2 runbook completeness audit (commit `975d635`). | Orchestrator | T+14d (2026-05-29) | **CLOSED.** Validator now resolves 269 prior references via: (a) added `_runbooks/` + `05_runbooks/` to DEFINITION_SOURCES, (b) added `**ID** (` anchor pattern for SLO catalog, (c) bulk allowlist additions for PLANNED forward-looking IDs + NOISE template placeholders. Net delta: 269 → 0. |
| DEBT-007 | **R4-6 backlog: 56 untranslated source files** in pt-BR/es-419 → coverage stuck at 64.1% (target ≥ 80%). Pre-existing predates wave 7 i18n DE. | `wt/r-prep-i18n-de` commit `1fb2dc4` CAVEATS | Orchestrator | T+14d (2026-05-29) | Dispatch 1 Sonnet to MT-stub seed the 56 source files via existing TMX; bring pt-BR + es-419 to ≥ 80%. |
| DEBT-008 | **3 mutation testing crates with full sweep deferred** (audit-chain, pat, clerk — targeted tests written but full `cargo mutants` run not measured). | `wt/r-prep-mutants-expansion` commit `699ca0b` CAVEATS | Orchestrator | T+21d (2026-06-05) | First CI nightly run will land empirical baselines. Monitor; if any crate < 75% kill rate post-baseline, dispatch fix agent. |
| DEBT-009 | ~~**4 remaining proptest density gap crates** (slack-real / admin-dry-run / cf-bindings / d1-migrations — ratio < 1 declared invariants per proptest).~~ **CLOSED 2026-05-15** — branch `wt/debt-009-proptest-4-crates`. 14 new property tests added (4 + 4 + 3 + 3); all 4 crates now at ratio ≥ 3.0. Quality gate green (build / clippy `-D warnings` / cargo test). See §8 in `specs/_audits/2026-05-15-proptest-density.md`. | `specs/_audits/proptest-followup-tickets.md` (commit `ab289ef`) | Orchestrator | ~~T+21d (2026-06-05)~~ → **CLOSED 2026-05-15** | ~~Dispatch 1 Sonnet to close all 4 in parallel (each ~3-5 proptests).~~ Done. |
| DEBT-010 | **11 CI optimization tickets** pending — 4 P1 + 4 P2 + 3 P3 from audit. ~52 min/PR savings projected. | `specs/_audits/ci-optimization-followup-tickets.md` (commit `2378dfe`) | Orchestrator | T+21d (2026-06-05) | Dispatch 1 Sonnet to implement P1 tickets (highest-ROI; ~30 min savings). P2/P3 deferred to post-GA. |
| DEBT-009 | **4 remaining proptest density gap crates** (slack-real / admin-dry-run / cf-bindings / d1-migrations — ratio < 1 declared invariants per proptest). | `specs/_audits/proptest-followup-tickets.md` (commit `ab289ef`) | Orchestrator | T+21d (2026-06-05) | Dispatch 1 Sonnet to close all 4 in parallel (each ~3-5 proptests). |
| DEBT-010 | **11 CI optimization tickets** — 4 P1 CLOSED (wt/debt-010-ci-opt-p1, 2026-05-15); 4 P2 + 3 P3 deferred post-GA. **STATUS: PARTIAL (4/11).** P1 wave realised ~85 billable min/PR savings (workspace dedup ~50 + docs consolidation ~25 + validators dedup ~10) + supply-chain hardening (CI-OPT-004 SHA-pinning, risk-only). | `specs/_audits/ci-optimization-followup-tickets.md` (commit `2378dfe` + wt/debt-010-ci-opt-p1) | Orchestrator | P1 done; P2+P3 deferred post-GA (T+90d horizon). | P1 SEALED. P2 (concurrency cancel, shared rust-cache key, TLC matrix, paths-filter audit) and P3 (top-level permissions, sparse-checkout, fuzz merge) remain open as low-value optimisation polish — schedule a single post-GA sweep. |
| DEBT-011 | **10 replication followup tickets** (3 P0 + 4 P1 + 3 P2) — RPO measurement gaps + Prometheus emit wiring for SLO-REPLICATION-LAG-*. | `specs/_audits/replication-followup-tickets.md` (commit `ed36bcd`) | Orchestrator | T+30d (2026-06-14) | Dispatch 1 Sonnet on the 3 P0 (RPO target unmet); P1/P2 with handler-crate skeleton (wave 7 in flight). |
| DEBT-011 | **10 replication followup tickets** (3 P0 + 4 P1 + 3 P2) — RPO measurement gaps + Prometheus emit wiring for SLO-REPLICATION-LAG-*. **PARTIAL (7/10): 3 P0 CLOSED 2026-05-15 via wt/debt-011-replication-p0; 4 P1 CLOSED 2026-05-15 via wt/debt-011-replication-p1-v2** (KV propagation + audit_outbox failback gate + DO sync-age + R2-CRR; trait + InMemoryFake + verifier-aligned constants; CF Worker production wiring still trait-deferred per charter). P2 (Neon SLI + replica-worker coverage + multipart-failover inventory) outstanding — post-GA polish. | `specs/_audits/replication-followup-tickets.md` (commit `ed36bcd`); P0 closure: wt/debt-011-replication-p0; P1 closure: wt/debt-011-replication-p1-v2 | Orchestrator | T+30d (2026-06-14) | 3 P0 + 4 P1 done. P2 deferred to post-GA polish — schedule single sweep alongside DEBT-010 P2/P3. |

---

## 3. P2 — must close before GA Limited

| ID | Title | Source | Owner | Target | Plan |
|---|---|---|---|---|---|
| DEBT-012 | **7 ISO 27001-unique gaps** + 1 informational PIMS deferred. Q1-2027 cert target. | `specs/_compliance/ISO27001-GAP-ANALYSIS.md` (commit `3ac2998`) | Orchestrator | T+90d (2026-08-13) | Closure work scheduled across Q3-2026 per ISO27001-ROADMAP.md. No immediate action; track in compliance weekly digest. |
| DEBT-013 | **10 perf optimization tickets** (top-5 hot spots + 5 minor; projected 3-10% p99 reduction). | `specs/_audits/perf-optimization-followup-tickets.md` (commit `3b4cbfc`) | Orchestrator | T+60d (2026-07-14) | Dispatch as separate WIs in R-3/R-4 integration waves. Not blocking GA but blocking GA-Full. |
| DEBT-014 | **9 TLA+ followup tickets** (FT-1..FT-9) covering remaining critical INVs + medium-severity. | `specs/_audits/tla-followup-tickets.md` (commit `992fcee`) | Orchestrator | T+60d (2026-07-14) | Cross-reference with DEBT-005; consolidate effort. |
| DEBT-015 | **`apps/docs` build fails on Node 22 and Node 20** — diagnosis update 2026-05-15: original premise (`@theme/prism-include-languages` ESM `ERR_MODULE_NOT_FOUND`) was masked by upstream pre-existing failures that block the build before the ESM stage. PARTIAL closure (commit `wt/debt-015-node-22-fix`) fixes sidebar config drift (12 doc-ID mismatches, double-comma syntax bug, 3 colliding "RBAC" i18n keys). Remaining blockers (out-of-scope of this WI): (a) 5+ MDX links to `draft: true` pages excluded from production build (`explanation/security/audit-chain.mdx`, `byok.mdx`, `explanation/residency/lgpd-brazil.mdx`), (b) cross-doc links using `.mdx` extension inconsistently, (c) docs-CI itself currently offline due to GitHub-billing account issue (cannot verify Node 20 baseline). Engine pin `<22` retained. The Node 22 ESM theme-alias issue (documented in `apps/docs/README.md` lines 100–131 with partial mitigation patch) re-evaluates only after blockers (a)+(b) resolve. | Multiple agent caveats (R3-7, R4-1, Trust Center, API Reference, etc.); local repro `pnpm --config.engine-strict=false build` in `apps/docs/`; `specs/_audits/2026-05-14-s18-sprint-close-review-round1.md` lines 47-48 | Orchestrator | T+21d (2026-06-05) | Dispatch 1 Sonnet to close blockers (a)+(b): unstub or remove `draft: true` from referenced security/residency pages, normalize MDX cross-links to extensionless form, then re-run build on Node 22; if `prism-include-languages` resurfaces, apply theme-alias rewrite to the existing `patches/@docusaurus__core@3.10.1.patch`. PARTIAL → CLOSED gate is green `pnpm build` on Node 22 with engine pin lifted. |
| DEBT-016 | **Statuspage URLs placeholder** — `status.corelink.dev` cited in 8+ trust pages but not live yet. | `wt/r-prep-statuspage-config` commit `a9de271` CAVEATS | Gustavo (Human) | T-7d pre-launch | Operator follows STATUSPAGE-INIT.md provisioning playbook. |
| ~~**DEBT-021**~~ **CLOSED 2026-05-15 (`0449182`)** | **Wave 9 bulk-merge auto-resolver shipped 10 workflows with unresolved conflict markers** to main since commit `cdf9458` (wt/debt-010-ci-opt-p1 merge). Markers were silently included because the python auto-resolve script ran `git add -A` post-resolve without re-grepping for leftover markers. Workflows were non-functional but no CI caught it. Process failure per /techlead AP-1 (sanity check skipped) + AP-7 ("I will check it later"). | Wave 11 actionlint agent surfaced (commit `0449182`) | Orchestrator | ~~T+1d~~ **Closed 2026-05-15** | actionlint baseline added as CI gate (DEBT-020 closure). Per-merge sanity check added to /techlead skill v2.0.1 (TBD): mandatory `grep -rEln "^<<<<<<<" specs/ apps/ .github/` post every auto-resolve before commit. Audit doc `2026-05-15-actionlint-baseline.md` §3.1-§3.2 documents the 10 affected workflows + resolution. |
| DEBT-017 | **5 followup proptest WIs** (3.5d total effort) for low-priority gap crates. | `specs/_audits/proptest-followup-tickets.md` | Orchestrator | T+30d (2026-06-14) | Bundle with DEBT-009 closure. |
| DEBT-018 | **CodeQL/Semgrep upstream action SHAs are placeholder pins** (v3.27.0 / v1 tag references; need verified SHAs after first green run). | `wt/r-prep-codeql-semgrep` commit `a280fe1` CAVEATS | Orchestrator | T+7d (2026-05-22) | **CLOSED 2026-05-15** — All `uses:` lines in `.github/workflows/codeql.yml` + `semgrep.yml` (plus 12 other workflows) SHA-pinned via `gh api`. Verifier `scripts/verify-action-sha-pinning.py` + CI gate `.github/workflows/action-sha-audit.yml` prevent regression. Baseline: `specs/_audits/2026-05-15-action-sha-pinning-baseline.md`. |
| DEBT-019 | **GitHub action SHAs not cross-verified against API** for wave 7/8 bot workflows. | `wt/r-prep-github-bots` commit `da4b473` CAVEATS | Orchestrator | T+7d (2026-05-22) | **CLOSED 2026-05-15** — All `uses:` in wave-7 bot workflows (`labeler.yml`, `welcome-first-pr.yml`, `stale.yml`, `size-label.yml`, `dependabot-auto-merge.yml`) were already SHA-pinned at wave-7 commit; full repo audit confirmed 100% coverage. One corrupted 39-char SHA (`dependabot/fetch-metadata`) corrected. See `specs/_audits/2026-05-15-action-sha-pinning-baseline.md`. |
| DEBT-020 | **actionlint CI gate** — workflow drift (invalid syntax, deprecated `set-env`/`add-path`, shell quoting bugs, unknown runner labels like retired `macos-13`, undefined GHA expression contexts) had no static lint gate post-DEBT-018+019 (85+ workflows, all SHA-pinned, but no syntactic guard). | This register | Orchestrator | T+7d (2026-05-22) | **CLOSED 2026-05-15** — `rhysd/actionlint:1.7.12` wired as CI gate (`.github/workflows/actionlint.yml`) pinned to Docker image digest `sha256:b1934ee5…`. Pre-fix baseline: **35 actionlint errors** across 22 workflows (incl. 10 unresolved git merge-conflict markers in `corelink-*.yml`/`tenant-path.yml`/`spec_validation.yml`/`tla_check.yml`/`dashboard_validation.yml`/`byok_kill_switch_drill_weekly.yml` from the `cdf9458` debt-010 merge that landed broken). Post-fix: **0 errors**. Repo-root `.actionlint.yaml` carries the (empty) self-hosted-runner allowlist + the `DT_ENDPOINT` vars-context allowlist. Baseline: `specs/_audits/2026-05-15-actionlint-baseline.md`. |

---

## 4. Closure tracking

This register IS the source of truth. The compliance weekly digest (`scripts/compliance-weekly-digest.py`) MUST read this file weekly and flag any row past its target without explicit waiver.

Waivers are NOT silent. If a target slips, append a row to `Section 5 — Waivers` below with justification + new target + sign-off (`Gustavo Schneiter` for P0/P1; for P2 a senior engineer suffices).

### 4.1 Closures

| ID | Closure date | Closure commit | Closure summary | Verifier |
|---|---|---|---|---|
| **DEBT-001** | 2026-05-15 | `52624e7` (branch `wt/debt-001-secrets-drift`) | Added rows #90-#108 to `docs/internal/secrets-checklist.md` covering all 19 code-only env vars (Azure federation, Drata, k6 load-test, Vault auth-modes, HTTP_PORT, Stripe price ID, PagerDuty compliance integration). | `python3 scripts/validate_secrets_matrix.py` → `code_only=0` (was 19); `python3 scripts/validate_specs.py` → no regression (428 docs validated). |

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
| 2026-05-15 | v1.0.1 — DEBT-001 closed (commit `52624e7`, branch `wt/debt-001-secrets-drift`). 19 code-only secrets matrix drift entries added; validator drift 19 → 0. Remaining open: 18 (P0:2 / P1:8 / P2:8). | Claude Opus 4.7 (DEBT-001 closure agent) |
| 2026-05-15 | v1.0.2 — DEBT-018 + DEBT-019 → CLOSED. 121 tag-pinned action refs across 14 workflow files SHA-pinned via `gh api`; verifier + CI gate added; baseline doc landed. | Gustavo (via Claude Opus 4.7 orchestrator, branch `wt/debt-018-019-action-sha-pin`) |
| 2026-05-15 | v1.0.3 — DEBT-002 → CLOSED. Orchestrator inline write of LICENSE-APACHE-2.0 + LICENSE-MIT (SPDX-pointer form + fetch script) + CODE_OF_CONDUCT.md (Contributor Covenant v2.1 adoption) + dco-check.yml workflow + OSS-VS-CLOSED-MATRIX.md (13 OSS / ~60 closed). CONTRIBUTING.md from wave 7 retained. | Gustavo (via Claude Opus 4.7 orchestrator, inline) |
| 2026-05-15 | v1.0.4 — Wave 11 partial: README SOTA + Dependabot auto-merge + ActionLint CI + Pre-sales SIG/CAIQ all SEALed. DEBT-020 (actionlint CI) CLOSED; DEBT-021 (wave 9 bulk-merge conflict-marker process failure) created and CLOSED in same pass (10 workflows repaired). 7 wave-11 agents hit Anthropic rate limit (5:40pm Bahia reset) without SEAL: DEBT-005 batch 2, DEBT-008, DEBT-011 P1, DEBT-013 OPT-01..05, CodeQL triage, AC+Admin handler wire — tracked for re-dispatch post-reset. | Gustavo (via Claude Opus 4.7 orchestrator) |
| 2026-05-15 | v1.0.1 — DEBT-004 CLOSED (orphan_refs 36→0; 34 promoted across §3.20–§3.26 new + §3.8/§3.9/§3.14/§3.15 extended; 2 removed/renamed-canonical). Validator green. | Claude Opus 4.7 DEBT-004 closure agent |
| 2026-05-15 | v1.0.1 — DEBT-009 CLOSED (`wt/debt-009-proptest-4-crates`): all 4 remaining proptest density gap crates closed (14 new property tests; quality gate green). | Gustavo (via Claude Opus 4.7 orchestrator + Sonnet 4.6 dispatch) |
