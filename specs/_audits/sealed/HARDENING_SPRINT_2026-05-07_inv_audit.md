---
id: "HARDENING-SPRINT-2026-05-07-INV-AUDIT"
type: "audit"
doc_status: "FROZEN"
audit_status: "COMPLETE"
version: "1.0.0"
created: "2026-05-07"
updated: "2026-05-07"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "invariants", "hardening", "p2-residuals"]
---

# Hardening Sprint — INV Dangling Audit + P2 Residuals

> **Date:** 2026-05-07
> **Scope:** Full invariant registry audit (136 INVs) + P2 sprint-close residuals S-06..S-10
> **Result:** 0 DANGLING | 4 THIN → WIRED | 2 P2 registry fixes applied | 1 P2 closed (S-09 P2-3)

---

## 1. INV Wiring Audit

### 1.1 Methodology

For each of the 136 canonical INV-* IDs in `specs/03_architecture/invariant_registry.md`, greps were
run across the entire repo (excluding `_archive/`, `target/`, `.git/`, and the registry file itself)
across `*.md`, `*.rs`, `*.toml`, `*.tla`, `*.py`, `*.yml`, `*.yaml`, `*.json`, `*.csv` files.

Categorization:
- **WIRED**: 2+ external file references
- **THIN**: exactly 1 external file reference
- **DANGLING**: 0 external file references

### 1.2 Summary Counts

| Status | Count | Notes |
|---|---|---|
| WIRED | 136 | All INVs wired post-fixes |
| THIN (pre-fix) | 4 | Fixed: INV-AC-PATH-KEY-MATERIALIZED, INV-DIGEST-VERIFICATION, INV-AC-ORPHAN-R2-CLEANUP-EVENTUAL, INV-AUTH-PAT-HMAC-SIG-VERIFIED |
| DANGLING | 0 | None found |

### 1.3 Pre-fix THIN INVs — Actions Taken

| INV | Pre-fix refs | Finding | Fix Applied |
|---|---|---|---|
| INV-AC-PATH-KEY-MATERIALIZED | 1 (ADR-0037 only) | Genuine missing wiring — materialized tenant_prefix is a critical path isolation mechanism documented in ADR-0037 but not surfaced in any runbook | Added reference + explanation to `RB-FM-303-ac-cross-tenant.md` References + Invariants section (DRAFT runbook; no version bump required) |
| INV-DIGEST-VERIFICATION | 1 (WI-S01-002 only) | Genuine missing wiring — write-path digest verification is foundational but absent from data_model.md §7 invariants table (alias INV-DATA-BLOB-HASH was present but not the canonical form) | Added canonical row to `data_model.md §7` invariants table alongside the legacy alias row |
| INV-AC-ORPHAN-R2-CLEANUP-EVENTUAL | 1 (R4-S04-audit review file only — counts as audit artifact, not wiring) | Genuine missing wiring — orphan R2 envelope cleanup is the explicit failure mode addressed by RB-FM-AC-TTL-DRIFT but the INV was not cited | Added INV reference to `RB-FM-AC-TTL-DRIFT.md` header callout (DRAFT runbook) |
| INV-AUTH-PAT-HMAC-SIG-VERIFIED | 1 (WI-S13-003 only) | Genuine missing wiring — HMAC fast-fail is the primary DDoS defense during auth storms, directly relevant to RB-FM-160 | Added INV reference + operational note to `RB-FM-160-auth-invalid-storm.md` header (DRAFT runbook) |

### 1.4 Full INV Status Table

| INV | Files (external) | Status |
|---|---|---|
| INV-TENANT-ISOLATION | 169 | WIRED |
| INV-CAS-INTEGRITY | 69 | WIRED |
| INV-CAS-IDEMPOTENCY | 39 | WIRED |
| INV-CAS-IMMUTABILITY | 40 | WIRED |
| INV-AC-OUTPUTS-VALID | 29 | WIRED |
| INV-AC-TENANT-SCOPED | 17 | WIRED |
| INV-GC-001 | 64 | WIRED |
| INV-GC-002 | 8 | WIRED |
| INV-GC-003 | 13 | WIRED |
| INV-GC-004 | 63 | WIRED |
| INV-DATA-MONOTONIC-TS | 3 | WIRED |
| INV-DATA-BILLING-RECONCILE | 2 | WIRED |
| INV-DATA-ERASURE-COMPLETE | 21 | WIRED |
| INV-AUDIT-APPEND-ONLY | 90 | WIRED |
| INV-AUDIT-RETENTION | 6 | WIRED |
| INV-CONF-AT-REST | 9 | WIRED |
| INV-CONF-IN-FLIGHT | 8 | WIRED |
| INV-AVAIL-ISOLATION | 47 | WIRED |
| INV-BILLING-NO-LOSS | 42 | WIRED |
| INV-BILLING-NO-DUP | 45 | WIRED |
| INV-SUPPLY-SIGNED-DEPLOY | 12 | WIRED |
| INV-SUPPLY-SBOM-PRESENT | 9 | WIRED |
| INV-QUOTA-ENFORCEMENT | 18 | WIRED |
| INV-DIGEST-VERIFICATION | 2 | WIRED (was THIN → fixed) |
| INV-DATA-RESIDENCY | 32 | WIRED |
| INV-DEDUP-CONSISTENCY | 27 | WIRED |
| INV-RATE-LIMIT-PROPORTIONALITY | 22 | WIRED |
| INV-OBS-CARDINALITY-BUDGET | 64 | WIRED |
| INV-OBS-AUDIT-CHAIN-INTEGRITY | 52 | WIRED |
| INV-BILLING-APPEND-ONLY | 9 | WIRED |
| INV-BILLING-RECONCILE-3-LAYER | 24 | WIRED |
| INV-BILLING-REPLAYABLE-FROM-EVENTS | 16 | WIRED |
| INV-CONSENT-PROOF-VERIFIABLE | 26 | WIRED |
| INV-SUPPLY-PROVENANCE-IN-REKOR | 12 | WIRED |
| INV-SUPPLY-NO-YANKED | 11 | WIRED |
| INV-SUPPLY-LICENSE-ALLOWLIST | 11 | WIRED |
| INV-ADMIN-DUAL-APPROVAL | 13 | WIRED |
| INV-ADMIN-MFA-FRESHNESS | 10 | WIRED |
| INV-BYOK-CRYPTO-SOVEREIGNTY | 21 | WIRED |
| INV-REGION-NO-CROSS-LEAK | 20 | WIRED |
| INV-ERASURE-ATTESTATION-SIGNED | 13 | WIRED |
| INV-ONBOARD-DPA-FIRST | 17 | WIRED |
| INV-ONBOARD-ATOMIC-PROVISIONING | 15 | WIRED |
| INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE | 13 | WIRED |
| INV-KEY-NO-SKIP | 17 | WIRED |
| INV-KEY-OVERLAP | 21 | WIRED |
| INV-KEY-AUDIT | 4 | WIRED |
| INV-AUTH-JWT-VALIDATE-RS256-ONLY | 4 | WIRED |
| INV-AUTH-CLOCK-SKEW-BOUND | 5 | WIRED |
| INV-AUTH-ISS-EXACT-MATCH | 3 | WIRED |
| INV-AUTH-KID-RESOLUTION | 3 | WIRED |
| INV-AUTH-PAT-HASH-ARGON2ID-2024 | 3 | WIRED |
| INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED | 3 | WIRED |
| INV-AUTH-PAT-VERIFY-CONSTANT-TIME | 3 | WIRED |
| INV-AUTH-PAT-SALT-PER-TOKEN | 3 | WIRED |
| INV-AUTH-PAT-SCOPE-DB-IS-SOT | 3 | WIRED |
| INV-AUTH-TENANTCTX-IMMUTABLE | 7 | WIRED |
| INV-AUTH-5-LAYER-ORDERING | 5 | WIRED |
| INV-AUTH-SESSION-CACHE-KEY-CT | 3 | WIRED |
| INV-AUTH-SCOPE-MIDDLEWARE-LEVEL | 3 | WIRED |
| INV-AUTH-AUDIT-PRE-POST-ORDERING | 3 | WIRED |
| INV-AUTH-REVOCATION-IDEMPOTENT | 7 | WIRED |
| INV-AUTH-REVOCATION-SLO-60S | 6 | WIRED |
| INV-AUTH-NEON-IS-SOT | 6 | WIRED |
| INV-AUTH-MASS-REVOKE-ATOMIC | 7 | WIRED |
| INV-AUTH-PROPAGATION-AT-LEAST-ONCE | 8 | WIRED |
| INV-AUTH-SCHEMA-RLS-DEFAULT-ON | 4 | WIRED |
| INV-AUTH-PAT-HMAC-SIG-VERIFIED | 2 | WIRED (was THIN → fixed) |
| INV-AUTH-PII-ENCRYPTED | 6 | WIRED |
| INV-AUTH-MIGRATION-ADDITIVE | 8 | WIRED |
| INV-AUTH-CASCADE-DSR-COMPLETE | 4 | WIRED |
| INV-AUTH-AUDIT-PSEUDONYMIZATION | 4 | WIRED |
| INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN | 7 | WIRED |
| INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED | 4 | WIRED |
| INV-AUTH-WEBAUTHN-SIGN-COUNT-MONOTONIC | 4 | WIRED |
| INV-AUTH-WEBAUTHN-ORIGIN-EXACT | 4 | WIRED |
| INV-AUTH-WEBAUTHN-RP-ID-CANONICAL | 6 | WIRED |
| INV-AUDIT-NO-RAW-PII | 21 | WIRED |
| INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER | 118 | WIRED |
| INV-AUDIT-CHAIN-HASH-DETERMINISTIC | 7 | WIRED |
| INV-AUDIT-EVENT-TYPE-EXHAUSTIVE | 6 | WIRED |
| INV-AUDIT-RETENTION-HINT-ACCURATE | 6 | WIRED |
| INV-NEG-CACHE-MONOTONIC | 4 | WIRED |
| INV-NO-BODY-IN-LOGS | 4 | WIRED |
| INV-NO-PII-IN-LOGS | 4 | WIRED |
| INV-AC-IDEMPOTENT | 15 | WIRED |
| INV-AC-RESULT-HASH-IMMUTABLE | 16 | WIRED |
| INV-AC-NEG-CACHE-INVALIDATED-ON-UPDATE | 7 | WIRED |
| INV-AC-MERKLE-VALID | 7 | WIRED |
| INV-AC-MERKLE-DETERMINISTIC | 6 | WIRED |
| INV-AC-BOUNDED-PARSER | 5 | WIRED |
| INV-AC-CYCLE-FREE | 5 | WIRED |
| INV-AC-DUAL-SIDE-VERIFY | 5 | WIRED |
| INV-AC-DIGEST-SIGNED | 5 | WIRED |
| INV-AC-SIG-CONSTANT-TIME | 4 | WIRED |
| INV-AC-SIG-INFO-FIXED | 4 | WIRED |
| INV-AC-TDK-ZEROIZED | 4 | WIRED |
| INV-AC-CANONICAL-BYTES-STABLE | 8 | WIRED |
| INV-AC-EVICT-TENANT-SCOPED | 15 | WIRED |
| INV-AC-EVICT-CONSISTENCY | 8 | WIRED |
| INV-AC-TTL-MONOTONIC | 13 | WIRED |
| INV-AC-PATH-KEY-MATERIALIZED | 2 | WIRED (was THIN → fixed) |
| INV-AC-PATH-SIG-KEY-VERSION-INDEPENDENT | 2 | WIRED |
| INV-AC-ORPHAN-R2-CLEANUP-EVENTUAL | 2 | WIRED (was THIN → fixed) |
| INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY | 6 | WIRED |
| INV-AC-KEY-ROTATION-GRACE | 3 | WIRED |
| INV-MULTIPART-IDEMPOTENT | 16 | WIRED |
| INV-MULTIPART-MANIFEST-SIGNED | 6 | WIRED |
| INV-MULTIPART-CONCURRENCY-BOUNDED | 5 | WIRED |
| INV-MULTIPART-CHUNK-DETERMINISTIC | 14 | WIRED |
| INV-MULTIPART-BOUNDED-PARSER | 13 | WIRED |
| INV-MULTIPART-STREAMING-MEMORY | 17 | WIRED |
| INV-MULTIPART-ORPHAN-DETECTABLE | 11 | WIRED |
| INV-MULTIPART-PATH-TENANT-SCOPED | 16 | WIRED |
| INV-MULTIPART-STATE-MONOTONIC | 12 | WIRED |
| INV-MULTIPART-PATH-KEY-MATERIALIZED | 7 | WIRED |
| INV-MULTIPART-MANIFEST-VALID | 7 | WIRED |
| INV-MULTIPART-DUAL-SIDE-VERIFY | 11 | WIRED |
| INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST | 13 | WIRED |
| INV-MULTIPART-FINALIZE-IRREVOCABLE | 11 | WIRED |
| INV-GC-IDEMPOTENT-RERUN | 5 | WIRED |
| INV-GC-SINGLE-RUNNING-PER-TENANT-REGION | 5 | WIRED |
| INV-GC-PHASE-MONOTONIC | 6 | WIRED |
| INV-GC-MARK-STARTED-AT-IMMUTABLE | 15 | WIRED |
| INV-GC-DEGRADE-MODE-PROBE-PER-BATCH | 4 | WIRED |
| INV-GC-MARK-STARTED-AT-ATOMIC | 8 | WIRED |
| INV-GC-REACHABLE-SET-COMPLETE | 4 | WIRED |
| INV-GC-MARK-TENANT-SCOPED | 4 | WIRED |
| INV-GC-MARK-PHASE-BUDGETED | 4 | WIRED |
| INV-GC-MARK-D1-BOUNDED-BATCH | 4 | WIRED |
| INV-GC-SWEEP-AUDIT-FAIL-CLOSED | 4 | WIRED |
| INV-GC-SWEEP-IDEMPOTENT | 4 | WIRED |
| INV-GC-SWEEP-TENANT-SCOPED | 4 | WIRED |
| INV-GC-GRACE-RESPECTED | 5 | WIRED |
| INV-GC-PHYSICAL-DELETE-IDEMPOTENT | 4 | WIRED |
| INV-GC-GRACE-BOUNDARY-STRICT | 4 | WIRED |
| INV-GC-R2-D1-ORDERING | 4 | WIRED |
| INV-GC-DSR-BYPASS-AUTHORIZED | 5 | WIRED |
| INV-GC-RECONCILE-AUTO-FIX-BOUNDED | 7 | WIRED |
| INV-GC-RECONCILE-AUDIT-FAIL-CLOSED | 4 | WIRED |
| INV-GC-CI-GATE-ENFORCED | 4 | WIRED |
| INV-GC-PROPERTY-TEST-CROSS-VALIDATED | 4 | WIRED |
| INV-GC-30D-SUSTAINED-VERIFICATION | 4 | WIRED |
| INV-GC-DEGRADE-CORRECT | 3 | WIRED |
| INV-EVICT-SOFT-DELETE-FIRST | 11 | WIRED |
| INV-EVICT-CASCADE-PREVENTED | 10 | WIRED |
| INV-EVICT-TTL-CAP-RESPECTED | 8 | WIRED |
| INV-LRU-CONSISTENCY | 14 | WIRED |
| INV-QUOTA-RESERVATION-TTL | 12 | WIRED |

---

## 2. P2 Sprint-Close Residuals Audit

### 2.1 Sources Reviewed

- `specs/_audits/2026-04-25-sonnet-r5-s06-wi-review.md` (OPUS-MISS-3, OPUS-MISS-4)
- `specs/_audits/2026-04-25-sonnet-r5-s07-wi-review.md` (P2-1, P2-2, P2-3)
- `specs/_audits/2026-04-25-sonnet-r5-s08-wi-review.md` (P2-1, P2-2, P2-3, P2-4)
- `specs/_audits/2026-04-25-sonnet-r5-s09-wi-review.md` (P2-1, P2-2, P2-3, P2-4)
- `specs/_audits/2026-04-26-sonnet-r5-s10-wi-review.md` (P2-1, P2-2, P2-3, P2-4, P2-5)

### 2.2 P2 Findings Table

| Finding | Sprint | Description | Quick Win? | Action |
|---|---|---|---|---|
| OPUS-MISS-4 | S-06 | INV-GC-RECONCILE-AUTO-FIX-BOUNDED missing percentage-floor | Already fixed in registry v0.2.0 (Lote 10.6-tris) | CLOSED (pre-existing) |
| OPUS-MISS-3 | S-06 | WI-007 §1.9 cost $0.000005 may be stale post-json_each | Doc-only; WI is FROZEN; no functional impact | SKIP (FROZEN WI; carry-forward to S-20 GA doc sweep) |
| P2-1 | S-07 | INV-LRU-CONSISTENCY "race-free" claim overstated — DO fire-and-forget creates bounded race window | YES — registry edit only | **FIXED**: INV-LRU-CONSISTENCY description updated to "bounded-drift correctness" with DO queue latency bound documented (≤ 100ms p99) |
| P2-2 | S-07 | Cascade prevention SQL missing evict_started_at_ms watermark | Already fixed (P0-6 in Lote 10.7bis — evict_started_at_ms added to WI-S07-002 §6.1.6) | CLOSED (pre-existing) |
| P2-3 | S-07 | DO hibernation/cold-start during quota check not analyzed | Addressed in WI-S07-003 §2 + risk register R-008 (DO cold start latency) | CLOSED (pre-existing) |
| P2-1 | S-08 | f64 precision drift concern understated in WI-S08-001 | Low risk; WI is FROZEN | SKIP (low risk; acknowledged in carry-forward) |
| P2-2 | S-08 | Alert count claim inconsistency WI-S08-006 (5 vs 6 SEV-2) | Doc cosmetic; WI is FROZEN | SKIP (carry-forward to S-20 GA doc sweep) |
| P2-3 | S-08 | Weight bias toward Bazel CPU workloads in abuse detection scoring | Complex calibration; WI is FROZEN | SKIP (>30min; architectural; carry-forward) |
| P2-4 | S-08 | `ratio_4xx == 1.0` exact float equality in suggest_block trigger | Explicitly deferred as "carry-forward to pre-launch advisory" in WI changelog v1.2.0 | SKIP (intentional carry-forward; annotated in WI) |
| P2-1 | S-09 | W3C Trace Context all-zeros trace-id rejection not specified | Already fixed in WI-S09-003 SEAL (implementation includes `reject all-zero trace_id/span_id per W3C §3.2.2.2/§3.2.2.3`) | CLOSED (pre-existing) |
| P2-2 | S-09 | IPv4-mapped IPv6 `::ffff:` prefix lost in redaction output | Complex codec change; WI is FROZEN | SKIP (>30min; edge case; carry-forward) |
| P2-3 | S-09 | Suspended/canceled tenant tier mapping undocumented | YES — registry note only | **FIXED**: INV-OBS-CARDINALITY-BUDGET updated with explicit suspended tenant policy (emit as Free tier; deliberate cardinality choice) |
| P2-4 | S-09 | Python cardinality validator regex is structurally fragile | Complex validator rewrite; WI is FROZEN | SKIP (>30min; architectural gap; carry-forward to S-20) |
| P2-1..P2-5 | S-10 | Various doc wording, GraceActive 5th state, chrono helper, f64 | All addressed in Lote 10.10bis + quaters cycles (see WI-S10-001 changelog) | CLOSED (pre-existing) |

### 2.3 P2 Items Closed in This Sprint

| ID | What | File Changed |
|---|---|---|
| P2-1 (S-07) | INV-LRU-CONSISTENCY claim weakened to "bounded-drift" | `specs/03_architecture/invariant_registry.md` |
| P2-3 (S-09) | Suspended tier policy documented in INV-OBS-CARDINALITY-BUDGET | `specs/03_architecture/invariant_registry.md` |

---

## 3. Items Needing Follow-Up (Not Addressable Here)

| Item | Sprint | Why Skipped | Recommended Action |
|---|---|---|---|
| S-08 P2-4 `ratio_4xx >= 0.99` | S-08 | Intentional carry-forward in WI changelog; WI FROZEN | Must address before S-08 CF Workers production binding ships (pre-launch gate) |
| S-09 P2-2 IPv4-mapped IPv6 redaction | S-09 | FROZEN WI; codec change >30min | Address in S-20 GA security hardening sweep |
| S-09 P2-4 Python cardinality validator regex | S-09 | FROZEN WI; architectural refactor >30min | Consider Rust build.rs integration test as bypass-proof alternative (per audit recommendation) |
| S-08 P2-2 alert count (5 vs 6 SEV-2) | S-08 | FROZEN WI; cosmetic | Address in S-20 GA doc sweep |
| S-06 OPUS-MISS-3 cost estimate stale | S-06 | FROZEN WI; no functional impact | Address in S-20 GA doc sweep |

---

## 4. Files Modified

| File | Change | Why |
|---|---|---|
| `specs/03_architecture/invariant_registry.md` | v0.2.0 → v0.2.1; INV-LRU-CONSISTENCY description updated; INV-OBS-CARDINALITY-BUDGET suspended-tier policy added | P2 residual fixes |
| `specs/03_architecture/data_model.md` | §7 invariants table: INV-DIGEST-VERIFICATION row added alongside legacy alias | THIN INV wiring |
| `specs/05_quality/runbooks/RB-FM-303-ac-cross-tenant.md` | References section: INV-AC-PATH-KEY-MATERIALIZED added with operational note | THIN INV wiring |
| `specs/05_quality/runbooks/RB-FM-AC-TTL-DRIFT.md` | Header: INV-AC-ORPHAN-R2-CLEANUP-EVENTUAL added to CTRLs + callout box | THIN INV wiring |
| `specs/05_quality/runbooks/RB-FM-160-auth-invalid-storm.md` | Header: INV-AUTH-PAT-HMAC-SIG-VERIFIED added to INVs + callout box | THIN INV wiring |
| `specs/_audits/HARDENING_SPRINT_2026-05-07_inv_audit.md` | NEW — this document | Deliverable |
