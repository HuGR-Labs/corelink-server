# Audit Chain Retention Mechanism — 2026-05-15

> **Doc kind:** evidence / audit attestation (no canonical front matter required — `_audits/` is excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Owner:** Gustavo Schneiter (Security Lead).
>
> **Trigger:** GA Wave 15 — Audit-Chain R2 NDJSON Archive Producer + 7-year retention enforcement.
>
> **Related WIs:** WI-S09-004 (CloudEvents emitter + R2 hash chain + daily verifier), WI-R-PREP-AUDIT-EXPORT (customer audit export), WI-S09-008 (this wave — customer-audit-export + retention enforcement).
>
> **Related controls:** CTRL-AUDIT-001 (R2 Object Lock Governance Mode 7y retention), CTRL-COMPLIANCE-SOC2-CC72 (immutable audit log evidence chain), INV-AUDIT-APPEND-ONLY (CRITICAL; TLA+ proven Lote 6.2), INV-OBS-AUDIT-CHAIN-INTEGRITY (HIGH; daily verifier).

## 1. Scope

This audit memo documents the mechanism CoreLink uses to enforce a **7-year minimum retention SLA** on every audit event written to R2 by the `corelink-audit-chain::archive_producer::ArchiveProducer`. The 7-year SLA is the long pole of the SOC 2 CC7.2 + ISO 27001 A.5.28 + GDPR Art. 30 + LGPD Art. 37 evidence-retention requirements; the wave-15 release ships the *enforcement* mechanism alongside the *storage* mechanism (the producer itself).

## 2. Retention mechanism (chosen)

CoreLink enforces 7-year retention via **two complementary mechanisms** layered defence-in-depth:

### 2.1 Primary — R2 Object Lock Governance Mode 7y (CTRL-AUDIT-001)

The production `audit-events-<region>` R2 buckets are provisioned in Cloudflare R2 with **Object Lock Governance Mode** and a **7-year minimum retention** policy. The IaC binding lives in the `corelink-iac` Terraform module (cloudflare provider; `cloudflare_r2_bucket` resource with `object_lock_configuration { mode = "GOVERNANCE"; retain_until_days = 2557 }` — 7 × 365 + 2 leap days). The `wrangler.toml` `[[r2_buckets]]` entry binds the bucket to the worker; the bucket-level Object Lock policy is set BEFORE any Worker can put an object via the binding (CF API enforces the order).

**Why Governance Mode and not Compliance Mode?**

Compliance Mode is irreversible — even the root account cannot delete an object inside the retention window. Governance Mode permits a `kms-admin` IAM principal (with explicit `s3:BypassGovernanceRetention` privilege) to extract an object for legitimate dual-approved deletion (GDPR Art. 17 erasure right; LGPD Art. 18 "anonimização" right). The dual-approval workflow lives in `WI-S16-005-admin-ops-ui-audit-viewer-dual-approval`; the bypass requires (a) signed Legal Hold release, (b) two distinct human approvers in the `kms-admin` group, (c) audit emit `corelink.audit_chain.retention_bypass_invoked` (SEV-0) before the bypass executes. The TLA+ spec `gc_sweep_audit_fail_closed.tla` (DEBT-005 batch 2) covers the analogous discipline for the GC sweep path.

### 2.2 Secondary — Property test on chunk-list age cardinality (Wave 15 NEW)

The `corelink-audit-chain::archive_producer` module + the daily-verify CLI assert as a **load-bearing invariant** that no chunk with a `<YYYY>/<MM>/<DD>` slot date `>= 7 years ago` can be *missing* from the R2 list. The daily-verify cron (`.github/workflows/audit-chain-daily-verify.yml`) walks the most recent 7 days of chunks and asserts:

1. Every chunk parses as valid NDJSON (Wave-15 archive shape).
2. Every chunk's chain links recompute to the running head.
3. The chunk count *monotonically increases* day-over-day (the cron compares to the previous day's count + asserts no negative delta).

Negative delta = SEV-0 alert `RB-AUDIT-CHAIN-RETENTION-VIOLATION`. The page-out wakes Security Lead immediately.

**Property test stub (lives in `crates/corelink-audit-chain/src/archive_producer.rs::tests` — see `chunk_keys_are_lexicographically_sortable_by_sequence` and the chain-head continuity tests):**

```text
property: ∀ chunks c1, c2 in R2 list ordered by key:
  c1.sequence_anchor < c2.sequence_anchor ⇒
    c2.prev_hash_anchor == c1.chain_head_after
    AND c2.first_sequence_number == c1.last_sequence_number + 1
```

Pinned by `chain_head_continuity_persists_across_two_chunks` (10k iter PR gate via the existing `prop_chain_verify_passes_on_unmodified` property-test budget).

## 3. TLA+ spec stub (informational; future Lote)

The retention discipline composes with `audit_immutability.tla` (invariant registry §3.7 — INV-AUDIT-APPEND-ONLY). The TLA+ spec stub for the wave-15 retention path is:

```tla
MODULE AuditChainRetention
EXTENDS Integers, Sequences, FiniteSets

CONSTANTS Tenants, RetentionDaysMin    \* 2557 days per CTRL-AUDIT-001

VARIABLES r2_objects, current_day

TypeOK ==
  /\ r2_objects \in [Tenants -> SUBSET [date : Nat, seq : Nat, hash : STRING]]
  /\ current_day \in Nat

InvNoDeleteWithinRetention ==
  \A t \in Tenants : \A o \in r2_objects[t] :
    (current_day - o.date) < RetentionDaysMin =>
      o \in r2_objects'[t]   \* every object inside the window MUST persist
```

Full proof + integration with `audit_immutability.tla` is a follow-on Lote (no PR-blocking dependency; the IaC + cron mechanism is the load-bearing enforcement).

## 4. Audit evidence

| Item | Evidence | Where |
|---|---|---|
| R2 bucket Object Lock policy | Terraform plan output captured in `corelink-iac` CI | `_archive/iac-r2-object-lock-evidence-2026-05-15.json` (TODO — Lote 15.2 cross-link) |
| Daily-verify cron pinning | SHA-pinned workflow | `.github/workflows/audit-chain-daily-verify.yml` |
| Property test pinning chain continuity | Rust unit + property tests | `crates/corelink-audit-chain/src/archive_producer.rs::tests::chain_head_continuity_persists_across_two_chunks` |
| Bypass dual-approval discipline | UI + audit emit | `WI-S16-005-admin-ops-ui-audit-viewer-dual-approval` |
| TLA+ spec stub | This document §3 | `specs/_audits/2026-05-15-audit-chain-retention.md` |
| Customer-facing audit-export endpoint (Wave-15.3) | axum route + integration test | `apps/server/src/routes/audit_export.rs` + `apps/server/tests/audit_export.rs` (7 tests: happy / cross-tenant reject / empty range / verify-failed SEV-0 / 401 / 429 / 503-audit-fail) |

## 5. SOC 2 CC7.2 mapping

| CC7.2 control | CoreLink mechanism | Evidence row |
|---|---|---|
| Detect security events / failures | INV-OBS-AUDIT-CHAIN-INTEGRITY daily-verify cron | `.github/workflows/audit-chain-daily-verify.yml` |
| Respond to identified events | RB-AUDIT-CHAIN-VERIFY runbook + SEV-0 page | `specs/_runbooks/RB-AUDIT-CHAIN-VERIFY.md` (Lote 15.3 follow-on) |
| Customer-initiated audit retrieval (SOC 2 CC7.2 + GDPR Art. 15+20 portability) | `GET /v1/audit/export` (Wave-15.3) | `apps/server/src/routes/audit_export.rs` |
| Communicate disposition | dual-approval audit-viewer UI | WI-S16-005 |
| Resume normal operations | resumable verifier via `verify_checkpoint` mirror | `crates/corelink-audit-chain/src/chain.rs::HashChainBuilder::resume` |

## 6. ISO 27001:2022 Annex A mapping

| Control | Mechanism |
|---|---|
| A.5.28 — Collection of evidence | R2 Object Lock 7y + NDJSON archive + chain-head continuity |
| A.8.15 — Logging | CloudEvents 1.0 audit emitter + R2 NDJSON archive |
| A.8.34 — Protection of information systems during audit testing | Governance Mode bypass requires dual-approval + audit emit |

## 7. Sign-off

- **Mechanism choice:** R2 Object Lock Governance Mode 7y (primary) + daily-verify cron with monotonic chunk-count check (secondary).
- **Mechanism owner:** Gustavo Schneiter (Security Lead).
- **Review cadence:** Annual (mirrors SOC 2 audit cycle).
- **Next review:** 2027-05-15.
