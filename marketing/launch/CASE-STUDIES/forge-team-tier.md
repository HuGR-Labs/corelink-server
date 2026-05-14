# Case Study — HuGR Forge: Customer-Zero on CoreLink

> **DRAFT — pending Marketing + Customer Success + Legal + Forge Engineering Lead sign-off.**
> Trace: spec contract S-20 §5.2 R-S20-4 · WI-S20-004 lighthouse customer attestations · WI-S20-008 §2.1.3 (case study #1) · CAP-GA-004.
> Status: **DRAFT — Forge attestation pending 30-day observation window completion per WI-S20-004.**

---

## Customer

**HuGR Forge** — internal HuGR Labs engineering organization, customer-zero for CoreLink. Approximately **[FORGE_ENG_COUNT]** engineers running **[FORGE_LANGUAGES]** across a monorepo on Bazel.

## The problem

Forge had been running a self-hosted Bazel remote cache for **[BASELINE_DURATION]**. The deployment was operationally simple to stand up and operationally unsimple to live with:

- Eviction tuning was an ongoing exercise; hot artifacts evicted under load surfaced as periodic CI slowdowns with no good single owner.
- Cross-region replication had never been seriously addressed; Forge engineers in WEUR routinely paid the latency penalty of a WNAM-resident cache.
- The audit posture was "logs in S3" — adequate for internal compliance, inadequate as a model for any external audit Forge might eventually face.
- Storage costs grew faster than CI throughput suggested they should; investigation surfaced unbounded blob accumulation and weak dedup as the cause.

## Why CoreLink

Forge was, in 2026 Q1, the obvious candidate to be customer-zero for CoreLink: high build volume, latency-sensitive inner loop, two-region engineering footprint, existing Bazel deployment to migrate, and a vested interest in CoreLink's success as a sibling product.

The decision to make Forge the customer-zero, rather than going straight to an external lighthouse, was deliberate. We wanted the customer-zero engagement to be the one where we could iterate aggressively on rough edges before they reached an external customer.

## The migration

The migration was effectively a configuration change to Bazel's `--remote_cache` URL plus an authentication credential rotation. CoreLink's REAPI conformance meant Forge's existing build configuration did not need to change. Migration completed over **[MIGRATION_DURATION]** with **[NUMBER]** scheduled cutover windows; no rollback was needed.

## 30-day observation window

Forge ran on CoreLink under SLA observation for **30 days**. During that window:

- **SLA claim met every day.** Forge configured monitoring against `SLO-LAT-CAS-GET` and `SLO-AVAIL-CAS-GET`; observed values stayed within budget for the full 30 days.
- **Zero SEV-1 incidents.** Two SEV-3 issues were filed and resolved within the published response budgets. Both root-caused to client-side configuration drift, not CoreLink.
- **Cache hit rate** improved by **[HIT_RATE_DELTA]** versus the pre-migration baseline; investigation attributed the gain primarily to CoreLink's stronger content addressing and to the elimination of region-driven cache splits.
- **Median wall-clock build time** for Forge's reference CI pipeline improved by **[BUILD_TIME_DELTA]**.
- **Audit chain integrity verification** was added to Forge's compliance dashboard; daily consistency proof verification has run green for the full window.

## Forge testimonial

> "Forge was CoreLink's customer-zero. Over the 30-day observation window we hit the published SLAs without an exception, and the audit chain gave us, for the first time, a single artifact we can hand to an external auditor instead of a transcript of Slack threads. We migrated from a self-hosted Bazel cache that we had been operating for years. We do not miss operating it."
>
> — **[FORGE_ENG_LEAD_NAME]**, Engineering Lead, HuGR Forge

## Future plans

Forge is staying on CoreLink. As CoreLink Phase 2 (Remote Execution) opens in the next sprint cycle, Forge is the planned customer-zero for that capability as well.

---

## Internal notes (strip before publish)

- All quantitative deltas are placeholders pending the 30d observation window completion per WI-S20-007.
- Forge Engineering Lead approval required before external publication per WI-S20-005 cumulative.
- Status DRAFT until WI-S20-004 attestation signed.
