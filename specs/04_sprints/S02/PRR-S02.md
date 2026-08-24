---
id: "PRR-S02"
type: "prr"
doc_status: "FROZEN"
work_status: "APPROVED"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-30"
updated: "2026-04-30"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
feature_wi: "WI-S02-006"
capabilities:
  - "CAP-CAS-004"
  - "CAP-CAS-005"
  - "CAP-CAS-006"
  - "CAP-CAS-007"
  - "CAP-CAS-008"
  - "CAP-CAS-009"
prod_target_date: "2026-06-12"
inherits_from:
  - "INVARIANT-REGISTRY"
  - "SECURITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "SLO-CATALOG"
  - "OBSERVABILITY-MODEL"
tags: ["prr", "s02", "high-risk", "production-readiness", "ship-gate"]
---

# PRR-S02 — Production Readiness Review · S-02 CAS Read Path + Client Verify

> **Sprint:** [S-02](./sprint.md) · **Lane:** HIGH_RISK · **Forcing factors:** FF-HR-002 (cross-tenant read = catastrophic blast radius), FF-HR-005 (CTRL-CAS-002 + CTRL-ISO-002 + CTRL-ISO-004 implementation)
> **Date opened:** 2026-04-30 · **Owner:** Gustavo Schneiter · **Final Approver:** Gustavo Schneiter

---

## 0. Purpose

PRR is the gate that authorizes promotion of the S-02 CAS read path to staging-stable + the trust boundary that lets a customer commit to a DPA. Per WI-S02-006 §0 + framework §33.5.4.3, the HIGH_RISK lane requires **11 sign-offs canonical**; this document captures the matrix, the residual risk register, the adversarial review summary, and the promotion gate decision.

This PRR is authored under the ADR-0034 solo-tier waiver (HIGH_RISK roles dual-hat to Owner / Architect / Security / Privacy / SRE while staffing is in flight). Each waived seat carries an explicit cross-reference + revalidation trigger; the corresponding canonical role is signed off by the dual-hat reviewer with the `(dual-hat per ADR-0034)` annotation.

## 1. Scope

This PRR covers **S-02 implementation phase** (sprint contract `_spec_contract.md` v1.15.0):

- **WI-S02-001** — REAPI ByteStream::Read + HTTP GET + tenant context propagation.
- **WI-S02-002** — GetBlob unary + BatchReadBlobs + FindMissingBlobs.
- **WI-S02-003** — `corelink-client-verify` SDK helper + ABI-stable interface.
- **WI-S02-004** — Constant-time 404 timing-padding middleware (3-arm Mann-Whitney + Šidák + bootstrap CI).
- **WI-S02-005** — Negative cache KV adapter (`ac_neg:<region>:<HMAC16>:<digest_hex>`).
- **WI-S02-006** — Cross-component property tests at 10k+ iter + bit-rot integration test + RB-FM-253 dry-run + this PRR.

Out of scope: external pentest (S-20 GA gate), customer-facing PRR sign-off (S-19 onboarding), staging E2E load test 50k QPS × 10 min (forward-looking once staging account provisioned).

## 2. Sign-off matrix (HIGH_RISK 11 canonical)

Per framework §33.5.4.3 + ADR-0034. The 11 canonical roles for HIGH_RISK lane:

| # | Role | Signer | Date | Status | Rationale / Evidence |
|---|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | 2026-04-30 | ✅ APPROVED | WI-S02-001..006 SEALED in commits `5e297bf` / `070200e` / `e4ddaa5` / `32e2ff6` / `25d6527` / [this WI]. |
| 2 | Final Approver | Gustavo Schneiter | 2026-04-30 | ✅ APPROVED | Owner + Final Approver dual-hat per ADR-0034. |
| 3 | Architect (incl. Crypto SME specialization for BLAKE3 + Mann-Whitney methodology) | Gustavo Schneiter (dual-hat per ADR-0034) | 2026-04-30 | ✅ APPROVED (waived) | ADR-0023 (constant-time defense) + ADR-0028 (MissReason uniform 404 freeze) authored; BLAKE3 verify default-on + 3-arm Mann-Whitney + Šidák + bootstrap CI design reviewed. |
| 4 | Security Lead | TBD (dual-hat per ADR-0034) | 2026-04-30 | ⚠️ WAIVED (ADR-0034) | STRIDE delta `THR-I-002` / `THR-I-003` / `THR-T-001` covered by CTRL-ISO-004 + CTRL-CAS-002 + CTRL-ISO-002. Revalidation trigger: hire Security Lead OR external advisor onboarded. |
| 5 | SRE Lead | TBD (dual-hat per ADR-0034) | 2026-04-30 | ⚠️ WAIVED (ADR-0034) | RB-FM-253 host-side dry-run executed (`scripts/rb_fm_253_dry_run.sh`); staging E2E dry-run deferred until CF account provisioned. Revalidation trigger: SRE Lead hired OR staging account provisioned. |
| 6 | Engineer (S-02 implementation lead) | Gustavo Schneiter | 2026-04-30 | ✅ APPROVED | Implementation lead through WI-S02-001..006. |
| 7 | QA Lead | TBD (dual-hat per ADR-0034) | 2026-04-30 | ⚠️ WAIVED (ADR-0034) | Cross-component property tests at 10k+ iter (`prop_cas_read.rs`) + 100k round-robin schedule + bit-rot 10 scenarios + adversarial timing 3-arm × 10k × 3 trial all green release. Revalidation trigger: QA Lead hired. |
| 8 | Product | Gustavo Schneiter (dual-hat per ADR-0034) | 2026-04-30 | ✅ APPROVED (waived) | JTBD coverage (§3 sprint contract): Bazel/Buck2 cache HIT < 100ms warm; bit-rot detection client-side; tenant isolation cryptographic 5-layer; 404 timing-indistinguishable across MissReason variants. |
| 9 | Compliance Officer | TBD (dual-hat per ADR-0034) | 2026-04-30 | ⚠️ WAIVED (ADR-0034) | DPA versioning live (S-19 forward); SOC 2 gap analysis closes at S-20 GA gate; LGPD Art. 48 / GDPR Art. 33 breach notification path codified in RB-FM-253. Revalidation trigger: Compliance Officer hired. |
| 10 | Privacy Officer | Gustavo Schneiter (dual-hat per ADR-0034) | 2026-04-30 | ✅ APPROVED (waived) | LINDDUN delta zero (read path is pull-only; no PII surface introduced); error_taxonomy entries do not leak `tenant_id`. Cross-tenant masking via ADR-0028 uniform 404 freeze. Revalidation trigger: Privacy Officer hired. |
| 11 | AppSec advisor / Adversarial reviewer | TBD (dual-hat per ADR-0034) | 2026-04-30 | ⚠️ WAIVED (ADR-0034) | Adversarial review summary in §6 (this doc). External pentest = S-20 GA gate. Revalidation trigger: AppSec advisor / external pentest engaged. |

**Sign-off totals:** 11 / 11 (5 ✅ APPROVED + 6 ⚠️ WAIVED via ADR-0034 dual-hat). Per framework §33.5.4.3 the HIGH_RISK matrix requires 10–12 sign-offs; the 11-canonical row is met. ADR-0034 solo-tier waiver register entry required for each `WAIVED` row; revalidation triggers documented inline.

> **Crypto SME** (BLAKE3 verify methodology + Mann-Whitney + bootstrap CI statistics) and **Statistician** (3-arm × 10k × 3 trial p-value gating math) fold into Architect role per spec contract §6 NOTA. **Adversarial reviewer** folds into AppSec per spec contract §6 NOTA. Peer reviewers contribute in PR review without a separate canonical sign-off slot.

## 3. Definition of Done — implementation evidence

Per `_spec_contract.md` v1.15.0 §6 (DoD).

| DoD item | Status | Evidence |
|---|---|---|
| 6 / 6 WIs SEALED | ✅ | Commits `5e297bf` / `070200e` / `e4ddaa5` / `32e2ff6` / `25d6527` / [this WI]. |
| Property test 100k iter tenant isolation green | ✅ | `crates/corelink-reapi/tests/prop_cas_read.rs::cross_tenant_read_round_robin_100k`; release run 0.49 s. |
| TLA+ verdes em CI (`tenant_isolation` + `cas_integrity` + `audit_immutability` + `gc_correctness`) | ✅ | Inherited from S-01 SEAL (commit `926877c`). Gate: `.github/workflows/tla_check.yml`. |
| Load test 50k QPS read × 10 min staging | ⚠️ DEFERRED | Forward-looking; staging account TBD. Revalidation trigger: staging account provisioned + S-19 onboarding starts. |
| Client verify default-on (`corelink-client-verify` Rust) | ✅ | `VerifyConfig::new()` enforces `enabled: true`; bit-rot 10 scenarios caught (`integration_bit_rot.rs`). |
| RB-FM-253 dry-run | ✅ (host-side) | `scripts/rb_fm_253_dry_run.sh`; staging dry-run deferred (see DEFERRED row above). |
| SBOM + signed release (CycloneDX 1.5+) | ✅ | Inherited from S-01 WI-S01-007 SEAL (commit `3339e1d`); `.github/workflows/cas_foundation.yml`. |
| Negative cache probe-storm cost reduction ≥ 80 % | ✅ | WI-S02-005 §10.5.2 + `prop_neg_cache.rs::cross_tenant_isolation_concurrent_100k` validates correctness; cost reduction quantified in WI-005 cycle 9 SEAL audit. |
| Streaming memory bounded by S-01 5 MiB cap | ✅ | WI-S02-001 §10.1.3; `R2Reader::get` returns materialized `Bytes`; multipart read deferred to WI-S05-005 (per `_spec_contract.md` §6 DoD partial bullet). |
| Bit-rot integrity test 10 / 10 scenarios | ✅ | `integration_bit_rot.rs::bit_rot_10_scenarios_all_caught_by_client_verify`. |
| PRR HIGH_RISK 11 sign-offs canonical | ✅ | This document §2. |
| Cost regression gate | ✅ | Inherited from S-01 SEAL; `.github/workflows/cas_foundation.yml`. |

**DoD totals:** 11 / 12 ✅; 1 / 12 ⚠️ DEFERRED (staging load test, forward-looking gate not blocking S-02 SEAL per §4 below).

## 4. Promotion gate decision

**DECISION: PROMOTE TO STAGING-STABLE.**

Rationale:

1. All 6 WIs SEALED with quality gates verde (clippy `-D warnings`, validators, debug + release tests, codex / Sonnet review where applicable).
2. Cross-component property tests at 10k+ iter (incl. 100k round-robin schedule) green; 0 cross-tenant leaks observed across 100 000 attempts.
3. Bit-rot detection 10 / 10 scenarios caught by `corelink-client-verify` default-on (CTRL-CAS-002 invariant binary).
4. RB-FM-253 host-side dry-run executes 6 runbook steps + drift detection without error.
5. Side-channel timing parity (3-arm 404 MissReason, ADR-0028) verified by `timing_indistinguishability::three_arm_indistinguishability_with_padding` (8 / 8 release pass; pairwise Mann-Whitney + Šidák all 9 tests `p > 0.005 685 8`).
6. Staging load test (50k QPS × 10 min) DEFERRED — not a S-02 SEAL blocker per spec contract §6 DoD partial-bullet pattern; revalidation trigger documented (staging account provisioned).

The waiver-bearing seats (Security / SRE / QA / Compliance / AppSec) are dual-hat per ADR-0034 with explicit revalidation triggers. Sprint S-02 SEALs at HIGH_RISK lane standard via the documented waiver path.

## 5. Residual risk register (post-mitigation)

Per spec contract §15 + sprint.md §10. After WI-S02-001..006 implementation the residual risk profile is:

| Risk | Pre-mitigation impact | Mitigation in S-02 | Residual | Owner |
|---|---|---|---|---|
| R-S02-001 — Side-channel timing leaks blob existence | HIGH | Tower middleware + 3-arm Mann-Whitney + Šidák + bootstrap CI; INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE pinned. | LOW | AppSec |
| R-S02-002 — Streaming memory leak (FM-403) | MEDIUM | S-01 single-blob 5 MiB cap; multipart read deferred to WI-S05-005. | LOW | SRE Lead |
| R-S02-003 — Client verify default-off bug | CRITICAL | `VerifyConfig::new()` const default `enabled: true`; opt-out builder + warning log; CI gate enforces default. | LOW | Crypto SME (Architect) |
| R-S02-004 — Cross-tenant read (FM-253) | CRITICAL | TLA+ + property test 100k + 5-layer defense + RB-FM-253 dry-run host-side. | LOW | Architect |
| R-S02-005 — Bit-rot R2 silent corruption (FM-051) | HIGH | Default-on client verify; 10 / 10 bit-rot scenarios caught. | LOW | Security Lead |
| R-S02-006 — Negative cache TTL too long → stale 404 | MEDIUM | Cache invalidation hook on S-01 PUT; TTL 300 s upper bound; manual invalidation API. | LOW | Tech Lead |
| R-S02-007 — PAT auth stub diverges from S-03 real | LOW | Stub interface frozen Lote 9.4; integration test covers transition. | LOW | Tech Lead |
| R-S02-008 — Probe storm cost without negative cache | MEDIUM | KV TTL 300 s + per-IP rate limit (S-08 soft dep). | LOW | SRE Lead |
| R-S02-009 — REAPI ByteStream protocol edge cases | LOW | REAPI v2 spec compliance test suite; reject invalid params. | LOW | Engineer |
| R-S02-010 — TLS termination at CF Edge | LOW | Cloudflare handles; documented; TLS 1.3 only path. **[SUPERSEDIDO 2026-07-19: o piso da zona é TLS 1.2 — ADR-0072]** | LOW | SRE Lead |
| R-S02-011 — Cost regression > 10 % baseline | MEDIUM | Cost regression gate `_spec_contract.md` §14.10; CI benchmark per-op cost. | LOW | Tech Lead |
| R-S02-012 — Mann-Whitney false positive | MEDIUM | 3 trial replication + Šidák correction (per-test α' ≈ 0.005 685 8); bootstrap CI gate. | LOW | AppSec |

All residuals = LOW after mitigation. No risk requires escalation.

## 6. Adversarial review summary (internal pentest)

Per WI-S02-006 §6.1.5. Internal pentest scope (not external — that is S-20 GA gate):

1. **Cross-tenant attempt scenarios.** Driven by `prop_cas_read.rs::cross_tenant_read_round_robin_100k` + `prop_cross_tenant_read.rs::concurrent_cross_tenant_isolation_100k`. **Result:** 0 leaks across 200 000 combined cross-tenant attempts; 0 R2 GET fired on the cross-tenant branch (CTRL-ISO-002 + CTRL-ISO-005 short-circuits at meta layer).
2. **Side-channel timing analysis.** Mann-Whitney supplementary run via `timing_indistinguishability::three_arm_indistinguishability_with_padding` (3 arms × 10 000 samples × 3 trials = 90 000 measurements; 9 pairwise Mann-Whitney U tests; Šidák-corrected per-test α' ≈ 0.005 685 8). **Result:** all 9 tests `p > 0.005 685 8` (full conjunction at familywise α = 0.05); `|Δmedian| ≤ 1 ms` point estimate AND `ci_upper ≤ 1 ms` via bootstrap CI.
3. **Bit-rot detection rate.** Driven by `integration_bit_rot.rs::bit_rot_10_scenarios_all_caught_by_client_verify`. **Result:** 10 / 10 scenarios caught at the `ClientVerifier::default_on()` seam.
4. **Probe storm enumeration cost.** WI-S02-005 §10.5.2 negative cache effectiveness: probe storm 1k QPS unknown digests → cost reduction ≥ 80 % vs no-cache baseline. **Result:** validated under negative cache adapter unit + property tests; deeper end-to-end measurement = staging E2E (forward-looking).
5. **Capability scope strictness.** `cache-r` vs `cache-find-missing` separately enforced (WI-S02-002 cycle 1 codex P1 fix). **Result:** scope strictness wins over "read implies discovery" shortcut.

The internal review surfaced no CRITICAL findings during S-02 implementation. The codex / Sonnet adversarial review across cycles 1..13 of S-02 closed all P0 + P1 findings with documented changelog entries; the residual P2/P3 findings are stylistic / forward-looking and do not block SEAL.

A canonical audit doc for this internal pentest is filed under `specs/_audits/sealed/2026-04-30-pentest-s02-internal.md` (wired in the same Lote as this PRR).

## 7. Observability live status

Per `_spec_contract.md` §11 + sprint.md §11. Metrics emitted by S-02 code:

- `corelink_cas_get_requests_total{plan, region, outcome}` — wired in `corelink-reapi::http_read` + `corelink-worker::storage::r2::MetricsObserver`.
- `corelink_cas_get_duration_seconds_bucket{outcome, blob_size_bucket}` — wired in `corelink-worker::storage::r2`.
- `corelink_cas_get_bytes_total{plan, region}` — egress tracking; wired in `corelink-reapi::audit` + `corelink-reapi::handler` chunked-read drop guard.
- `corelink_cas_side_channel_timing_diff_ms` — emitted by `corelink-worker::middleware::timing_padding` (gated by `tower-middleware` feature).
- `corelink_cas_client_verify_fail_total{reason}` — emitted by SDK consumers (S-15 forward-looking).
- `corelink_cas_negative_cache_hits_total{plan}` + `corelink_cas_negative_cache_invalidation_total{trigger}` — wired in `corelink-worker::cache::negative`.
- `corelink_cas_streaming_peak_bytes{operation_id}` — bounded by 5 MiB single-blob cap; metric stub deferred to multipart read (S-05).

Dashboards `DASH-CAS` + alerts (SEV-1 / SEV-2 / SEV-3 thresholds) defined in spec contract; live wiring against Grafana = S-09 forward-looking observability stack.

## 8. Knowledge transfer + tech-talk

Per WI-S02-006 §27. KT artifacts produced by S-02 SEAL:

- `PRR-S02.md` (this doc) — canonical decision record.
- `docs/internal/side-channel-defense.md` (WI-S02-004 SEAL) — Tower middleware design + 3-arm Mann-Whitney + Šidák + bootstrap CI cookbook.
- `ADR-0023-constant-time-timing-padding.md` — constant-time defense decision.
- `ADR-0028-missreason-uniform-404-freeze.md` — uniform 404 freeze + cross-tenant masking decision.
- `ADR-0034-solo-tier-waiver.md` (inherited) — dual-hat reviewer policy.

Tech-talk "S-02 Ship Gate: 100k Property Tests + Adversarial Review" (30 min) — recorded as part of sprint review prep.

## 9. Outbound dependencies cleared by S-02 SEAL

- **S-03** (auth real Clerk) — S-02 PAT scope stub + 5-layer defense path frozen; S-03 implementations honor the interface.
- **S-04** (AC read path) — S-02 read primitives consumed by AC read.
- **S-07** (eviction) — S-02 read patterns drive LRU eviction tier.
- **S-15** (CLI / SDK) — `corelink-client-verify` Rust crate + ABI stable; FFI wrappers + integration tests Python pyO3 / Go cgo / JS WASM are S-15 deliverables.
- **S-20** (GA) — S-02 SLO targets `SLO-AVAIL-CAS-GET ≥ 99.9 %` + `SLO-LAT-CAS-GET p99 < 300 ms cold / < 100 ms warm` are sustained 30 d at GA gate.

## 10. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-04-30 | Gustavo Schneiter (via Claude Opus 4.7) | Initial PRR-S02 authored as part of WI-S02-006 SEAL Lote. 11 sign-off matrix populated under ADR-0034 solo-tier waiver. Promotion decision: STAGING-STABLE. |

---

**End PRR-S02 v1.0.0.**
