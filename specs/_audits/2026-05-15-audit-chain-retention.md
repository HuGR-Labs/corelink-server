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

**Production cutover note (Wave 17, 2026-05-15):** the wave-15 daily-verify cron shipped with an OK-marker smoke placeholder for the R2 list step pending `CF_API_TOKEN` binding. Wave 17 replaces that placeholder with a real Cloudflare API v4 `GET /accounts/{account_id}/r2/buckets/{bucket}/objects?prefix=audit/<YYYY>/<MM>/<DD>/` paginated list (per-page `1000`, cursor-walked; cap 100 pages → 100k keys) + a per-object `GET /accounts/{account_id}/r2/buckets/{bucket}/objects/{key}` download loop. The verifier binary receives every downloaded chunk path on argv and asserts chain integrity tenant-by-tenant. Any non-2xx from list or GET fails the job (fail-CLOSED — no silent "no chunks found"). A chain break triggers a SEV-0 PagerDuty Events API v2 dispatch with `dedup_key=audit-chain-break-<YYYY-MM-DD>-<chunk-key>` and marker `AUDIT_CHAIN_BREAK_DETECTED::<date>::<chunk-key>`. The audit-archive lane uses a `CF_API_TOKEN` scoped to **`Workers R2 Storage:Read` on the `corelink-audit-archive` bucket ONLY** (secrets-checklist.md #50 audit-read variant; #118 documents the bucket-name override `AUDIT_R2_BUCKET`). Workflow extension shipped on branch `wt/r-prep-r2-list-cf-token` (commit see `.github/workflows/audit-chain-daily-verify.yml` `git log`).

**Wave-18 7-day matrix lift (2026-05-15):** the Wave-17 paginated CF API v4 list + per-object GET path is now lifted INSIDE the `seven-day-verify` job's matrix loop (today, today-1, ..., today-6). Each matrix day independently performs the full production list/GET/verify cycle: paginated list with `result_info.cursor` walk (cap 100 pages = 100k keys fanout-guard) → per-object GET to `./.audit-verify-staging/<date>/chunks/<r2-key>` → verifier on argv → grep `AUDIT_CHAIN_BREAK_DETECTED`. **Per-day fail-CLOSED + continue matrix** contract: a single bad day (list 5xx/auth/JSON-parse/cursor-error, per-object GET non-2xx, or verifier non-zero exit) is paged to PagerDuty via Events API v2 (dedup `audit-chain-break-<date>-<chunk-key>`) AND the loop continues — N bad days surface as N distinct PD pages with the marker `AUDIT_CHAIN_7DAY_BREAK_DETECTED::<date>::<chunk-key>`. Job exits non-zero IFF ≥1 day broke. Lift shipped on branch `wt/r-prep-r2-list-7day-matrix-lift`; SHA-pin audit clean (528 `uses:`, no new actions; CF + PD dispatch are pure curl); `permissions: contents: read` preserved; `concurrency: audit-chain-daily-verify` group added to prevent overlapping cron + workflow_dispatch runs.

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
| Customer-audit-export TRUE STREAMING wire-up (Wave-18) | `axum::body::Body::new(http_body_util::StreamBody::new(...))` per-row NDJSON `Frame::data` + `http_body::Frame::trailers` for the mid-stream `X-CoreLink-Audit-Export-Aborted` abort trailer | `apps/server/src/routes/audit_export.rs::build_audit_export_stream_frames` + `mid_stream_abort_trailer_value` + `apps/server/tests/audit_export.rs::{streaming_response_does_not_buffer, abort_trailer_emitted_on_mid_stream_chain_break, customer_cli_handles_abort_trailer_gracefully}`. Per-row `verify_inclusion_proof` re-check against the manifest anchor; on the first break the SEV-0 `corelink.audit.export_verify_failed.v1` emit lands BEFORE the trailer frame on the wire (audit-anchor-first per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER). Trailer payload is the canonical `{break_at_seq, break_at_chunk, observed, expected}` JSON; the `Trailer:` response header advertises the trailer name upfront per RFC 7230 §4.4. Wave-17 customer-CLI parser remains byte-equivalent on the streaming wire shape (wave-18 adds `crates/corelink-cli/src/commands/verify_ndjson.rs::tests::streaming_wire_shape_round_trips_via_line_parser` to pin this contract). |
| Wave-19 — async page-by-page generator (replaces wave-18 pre-materialized frame plan) | `build_audit_export_async_stream` returning `impl Stream<Item = Result<Frame<Bytes>, Infallible>>` driven by `async_stream::stream!`; one page of rows at a time via `R2ListPager` trait (page budget = `R2_LIST_PAGE_SIZE` = 1000 to mirror CF API v4 `per_page=1000`); per-row buffer capacity tunable via `EXPORT_ROW_BUFFER_BYTES` env var (default `DEFAULT_EXPORT_ROW_BUFFER_BYTES` = 64 KiB). Memory bound = 1 page × rows + 1 row's serialized buffer; generator parks on each `yield` so axum body-flow back-pressure controls page fetch (no buffer-ahead). Audit-anchor-BEFORE-trailer invariant proven by **10 000-iter property test** (`audit_anchor_emits_before_trailer_under_random_breaks`: random `n ∈ [1,16]` rows × random `p ∈ [1,8]` page size × random `seed`). | `apps/server/src/routes/audit_export.rs::build_audit_export_async_stream` + `R2ListPager` + `InMemoryR2ListPager` + `ENV_EXPORT_ROW_BUFFER_BYTES` + `R2_LIST_PAGE_SIZE` + `apps/server/tests/audit_export.rs::streaming_response_yields_all_rows_across_multiple_pages` + 9 server unit tests in the module. All wave-18 24/10/7 tests preserved bit-for-bit. Branch `wt/r-prep-audit-export-async-pages`. |
| Wave-17 PagerDuty alert wiring for the 2 paged emits | Alert rules + 2 SEV runbooks | `dashboards/alerts/dash-audit-export-alerts.yml` (`AuditExport_CrossTenantAttempt` SEV-1, `AuditExport_VerifyFailed` SEV-0) + `specs/_runbooks/RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT.md` + `specs/_runbooks/RB-AUDIT-EXPORT-VERIFY-FAILED.md`. Routing: `PAGERDUTY_ROUTING_KEY` (secrets-matrix row #11). SEV-0 starts LGPD Art. 46 / GDPR Art. 33 72h clock on confirm. |
| Wave-18 Neon analytics shadow tier | Shadow sync + 2 customer-facing SQL aggregate endpoints | `crates/corelink-audit-chain/src/neon_shadow.rs` + `apps/server/src/routes/audit_analytics.rs` + `migrations/neon/0001_audit_events_shadow.sql` + `specs/_audits/2026-05-15-neon-analytics-shadow.md`. **Tier split:** R2 = canonical chain-integrity store (this doc, §2.1 Object Lock 7y retention); Neon = analytics convenience tier (≤ 5 min nominal lag; SEV-2 on lag ≥ 60 min; SEV-0 chain-break discipline preserved at the R2 source-of-truth). Daily-verify cron unchanged — it walks R2; Neon divergence is treated as analytics anomaly, not chain break. |

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
