---
id: "AUDIT-REPLICATION-2026-05-15"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-prep / R-6 staging entry follow-on"
parent_wi: "R-PREP-REPLICATION-AUDIT"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "replication", "rpo", "cross-region", "dr-16", "failover", "active-failover", "r-prep", "r-6"]
---

# Cross-Region Replication Audit — Type-level RPO, topology, and inconsistency windows

> **doc_status:** REVIEW · **scope:** type-level (per data domain) audit
> of what is actually replicated across CoreLink's four GA regions
> (WNAM / ENAM / WEUR / SAM) given the current implementation in
> `crates/corelink-region`, `crates/corelink-replica-worker`,
> `crates/corelink-failover-router`, plus the migration tool in
> `apps/migrate-single-to-multi-region`. **No code changes** — this
> document is the source-of-truth for what DR-16 (active-failover
> drill, `specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md`) and DR-15
> (cold-restore drill, `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md`)
> can assume about replicated state on declaration day.
>
> **Anchors:**
> - Implementation: `crates/corelink-region/src/{region,event,migration}.rs`,
>   `crates/corelink-replica-worker/src/{replication,hot_blob,aggregator,region}.rs`,
>   `crates/corelink-failover-router/src/{router,probe,health}.rs`.
> - Migration tooling: `apps/migrate-single-to-multi-region/src/main.rs`.
> - Spec anchors: `specs/03_architecture/data_model.md` §3, §5, §6,
>   `specs/03_architecture/resilience_patterns.md` §3.4 + §3.6,
>   `specs/03_architecture/storage_semantics_matrix.md` §7.2,
>   `specs/03_architecture/slo_catalog.md` §4.18 / §4.19 / §4.22.
> - Drills consuming this audit: DR-16 (`ACTIVE-FAILOVER-DRILL-SPEC.md`)
>   + DR-15 (`COLD-RESTORE-DRILL-SPEC.md`).
> - ROADMAP cross-link: `ROADMAP-TO-GA.md` §6 (Wave R-6 staging) — DR-16
>   listed under "BCP / DR drill cadence" sub-section.

---

## 0. TL;DR

- CoreLink has **four data domains** that cross a region boundary at
  runtime: **R2 (CAS / AC / audit blobs)**, **D1 (per-region operational
  metadata)**, **KV (short-lived caches + presigned URL cache)**, and
  **DO state (rate / quota / config / multipart sessions)**. A fifth
  domain — **Neon Postgres control-plane** — is global by construction
  (US primary + EU read replicas) and is audited here for completeness.
- **Only one domain is actively cross-region-replicated by CoreLink
  code today: R2 *hot* blobs** (top-1% access, `corelink-replica-worker`
  cron) via the static acyclic `ResidencyGraph` (WNAM↔ENAM, WEUR↔SAM).
  All other R2 objects (cold + AC + audit) rely on **Cloudflare
  platform replication** (R2 cross-region replicas) configured at
  bucket level — outside CoreLink code.
- **D1 cross-region replication is read-only** (Cloudflare D1 read
  replicas). The write path is single-region pinned per-tenant.
  Failover requires a **write-lease handover** (DR-16) — there is no
  multi-master.
- **KV is global eventual** (≤ 60 s typical per Cloudflare docs) and
  **never source-of-truth** (`PAT-KV-TTL-001`). Stale-after-failover is
  bounded by KV TTLs (≤ 300 s).
- **DO state is single-region by design** (one instance per
  `tenant_id` / `flag_key`). DO migration is automatic *within* a
  region but **not cross-region**; cross-region DO recovery requires
  rebuild from D1 (`PAT-DO-MIGRATION-AWARE-001`).
- **RPO targets** documented today (4 + 1 global): R2-hot ≤ 60 s,
  R2-cold ≤ 24 h, D1 ≤ 6 h, KV ≤ 12 h, DO state ≤ 60 s (bounded by D1
  reconcile cadence; DO is cache). The DR drill catalog (`SLO-RPO-REGION`)
  asserts ≤ 60 s p99 *at active-failover declaration* — that is the
  *write-lease* RPO, not the per-domain physical-replication RPO.
- **Three measured-RPO gaps** flagged as P0/P1 follow-ups (see
  `replication-followup-tickets.md`):
  1. **R2-hot p99 propagation delay is not measured** in staging; the
     `REPLICATION_LAG_P99_SLO_SECS = 60` constant is a *target*, not a
     verified SLI.
  2. **D1 read-replica lag** has no SLI exported to Prometheus today.
  3. **KV propagation delay** (the ≤ 60 s "typical" claim) is treated
     as a documentation assertion, not a measured per-region value.

---

## 1. Scope + non-goals

### 1.1 In scope

- Per data domain (R2 / D1 / KV / DO state / Neon): replication
  topology, RPO target, RPO measured (or "unmeasured — gap"),
  propagation-delay p99 (target + measured), conflict resolution
  policy, edge cases where replication can lag or miss.
- Failure modes during transient errors, network partitions, and
  schema migrations.
- One Mermaid diagram showing the four-region replication mesh.

### 1.2 Out of scope

- Backup replication (covered by `SLO-BACKUP-VERIFICATION` §4.22 +
  `scripts/backup-daily.sh` + `RB-BACKUP-VERIFICATION-FAILURE`). Backups
  are *cross-region encrypted snapshots*; replication is the *live*
  data path. The two are intentionally separate — DR-15 uses backups,
  DR-16 uses replication.
- Cross-region BYOK CMK availability (covered by DR-007 / DR-008 and
  the byok-kill-switch drill `2026-05-14-byok-kill-switch-drill-aws.md`).
- The data-residency-violation control surface (covered by
  `PAT-ROUTING-PINNED-001` + WI-S11-007); this audit assumes residency
  pinning works and only audits *intra-jurisdiction* replication.

---

## 2. Replication topology — one-page mental model

Four GA regions, two acyclic sibling pairs per
`ResidencyGraph::sibling()` (`crates/corelink-replica-worker/src/region.rs`
L229-L236):

- **US pair:** WNAM ↔ ENAM (PIPEDA/CCPA + CCPA/HIPAA-opt-in)
- **EU↔LGPD pair:** WEUR ↔ SAM (GDPR Art. 44/Schrems II + LGPD Art. 33)

Cross-jurisdiction replication is **statically forbidden** at the
graph level (`ResidencyGraph::is_allowed()` returns `ResidencyViolationInfo`
for any non-sibling target; verified by `is_acyclic()` 2-hop symmetry
check; `INV-REGION-NO-CROSS-LEAK`).

### 2.1 Mermaid: live replication mesh (per domain)

```mermaid
flowchart LR
  subgraph US["US jurisdiction"]
    WNAM[WNAM<br/>us-west]
    ENAM[ENAM<br/>us-east]
  end
  subgraph EUSA["EU↔LGPD jurisdiction"]
    WEUR[WEUR<br/>eu-west]
    SAM[SAM<br/>sa-east]
  end
  subgraph GLOBAL["Global (no jurisdiction restriction)"]
    NEON[(Neon Postgres<br/>US primary + EU read replicas)]
    KV{{KV<br/>global eventual<br/>≤60s typical}}
  end

  WNAM <-->|R2-hot via replica-worker<br/>RPO target ≤60s| ENAM
  WEUR <-->|R2-hot via replica-worker<br/>RPO target ≤60s| SAM

  WNAM -. R2 platform CRR<br/>RPO ≤24h .-> ENAM
  ENAM -. R2 platform CRR<br/>RPO ≤24h .-> WNAM
  WEUR -. R2 platform CRR<br/>RPO ≤24h .-> SAM
  SAM  -. R2 platform CRR<br/>RPO ≤24h .-> WEUR

  WNAM === D1WN[D1 wnam<br/>single-region write] -.read-replica.- ENAM
  ENAM === D1EN[D1 enam<br/>single-region write] -.read-replica.- WNAM
  WEUR === D1WE[D1 weur<br/>single-region write] -.read-replica.- SAM
  SAM  === D1SA[D1 sam<br/>single-region write] -.read-replica.- WEUR

  KV --> WNAM
  KV --> ENAM
  KV --> WEUR
  KV --> SAM

  NEON --> WNAM
  NEON --> ENAM
  NEON --> WEUR
  NEON --> SAM

  WNAM -. "DO state<br/>single-region, no cross-region replica<br/>rebuild from D1 on failover" .-> WNAM
  ENAM -. "DO state<br/>single-region, no cross-region replica<br/>rebuild from D1 on failover" .-> ENAM
  WEUR -. "DO state<br/>single-region, no cross-region replica<br/>rebuild from D1 on failover" .-> WEUR
  SAM  -. "DO state<br/>single-region, no cross-region replica<br/>rebuild from D1 on failover" .-> SAM

  classDef forbidden stroke:#c00,stroke-width:2px;
  WNAM -. "FORBIDDEN<br/>cross-jurisdiction<br/>ResidencyViolation" .x WEUR
  ENAM -. "FORBIDDEN<br/>cross-jurisdiction<br/>ResidencyViolation" .x SAM
```

ASCII fallback (for non-Mermaid renderers):

```
                R2-hot (replica-worker, RPO ≤60s)
                R2-cold/AC/audit (R2 platform CRR, RPO ≤24h)
                D1 read replicas (read-only, no write replica)

   +-----------------------------+    +-----------------------------+
   |  US jurisdiction            |    |  EU ↔ LGPD jurisdiction     |
   |                             |    |                             |
   |  [WNAM] <======>  [ENAM]    |    |  [WEUR] <======>  [SAM]     |
   |    ^                ^       |    |    ^                 ^      |
   |    |                |       |    |    |                 |      |
   +----|----------------|-------+    +----|-----------------|------+
        |                |                  |                 |
        |    +--------------------+    +---------------------+|
        +----| KV (global ≤60s)   |----| Neon (US primary +  ||
             | DO (single-region) |    | EU read replicas)   ||
             +--------------------+    +---------------------+

         X---- FORBIDDEN cross-jurisdiction ----X
              (ResidencyGraph::is_allowed = Err)
```

---

## 3. Per-domain replication audit

### 3.1 R2 — CAS hot blobs (top-1% access)

| Field | Value |
|---|---|
| **Topology** | Active-passive sibling pair; primary writes, replica receives async copy via `corelink-replica-worker` cron. |
| **Sibling pairs** | WNAM↔ENAM, WEUR↔SAM (`ResidencyGraph`, acyclic 2-hop symmetric). |
| **RPO target** | **≤ 60 s p99** (`REPLICATION_LAG_P99_SLO_SECS = 60` in `crates/corelink-replica-worker/src/region.rs` L180). |
| **RPO measured** | **UNMEASURED — GAP-R1.** No Prometheus SLI exported today; the constant is a target, not an SLI. |
| **Propagation delay p99 target** | ≤ 60 s. |
| **Propagation delay p99 measured** | **Unmeasured (in-memory worker only; production wiring deferred to staging).** |
| **Conflict resolution** | **N/A — write-pinned.** Primary `primary_region` of a tenant is pinned (`tenant.primary_region` in Neon DDL; INV-DATA-RESIDENCY). Replica is read-only fallback; writes never go to replica. |
| **Integrity check** | `compute_hash(replica) == compute_hash(primary)` post-copy (`replication.rs` L161); mismatch triggers exponential-backoff retry (`MAX_REPLICATION_RETRIES = 5`); persistent mismatch ⇒ `RetryExhausted` + SEV-2. |
| **Audit invariant** | `INV-CAS-INTEGRITY` (hash matches digest path); `ReplicationStarted` audit emit **before** state mutation (fail-CLOSED, `replication.rs` L121-L131). |
| **Edge cases where replication can lag or miss** | (a) `ReplicaError::R2Copy` transient errors are logged but batch continues (non-fatal); blob retried next cron tick. (b) `HashMismatch` after 5 retries → `RetryExhausted` SEV-2 audit; blob remains in `Failed` state in `hot_blobs` table. (c) Aggregator window misses: only blobs with `access_count_30d` above hot-threshold are replicated — a sudden hot blob is **cold for the full aggregation window** (`AGGREGATION_WINDOW_DAYS`). (d) `ReplicaError::ResidencyViolation` is a hard error that halts the batch (cross-jurisdiction attempt — should never occur under static graph). (e) `audit.emit` failure → batch halted (`ReplicaError::Audit` fail-CLOSED). |

### 3.2 R2 — CAS cold blobs / AC entries / audit blobs

| Field | Value |
|---|---|
| **Topology** | Active-passive — relies on **Cloudflare R2 platform cross-region replication (CRR)** configured at bucket level (one rule per sibling pair). Not implemented by CoreLink code; verified at infra-Terraform level. |
| **RPO target** | **≤ 24 h** (per `SLO-BACKUP-VERIFICATION` row "R2 (RPO 24h)" in `slo_catalog.md §4.22`). |
| **RPO measured** | Backup-tier RPO measured daily by `scripts/backup-daily-verify.sh`; cross-region R2 CRR lag itself **is not directly measured** (gap GAP-R2 — see follow-up tickets). |
| **Propagation delay p99 target** | Cloudflare doesn't publish a hard SLO for R2 CRR; we use a 24 h ceiling as the *contractual budget* and rely on daily backup verification to catch silent drift. |
| **Propagation delay p99 measured** | Indirectly via backup-verify pass rate ≥ 99.5%. |
| **Conflict resolution** | **Last-write-wins** at object-key level — by construction, an R2 key is `tenant/<hmac>/<digest>/...` and digests are content-addressed, so two writes to the same key under the same content produce the same blob (idempotent). For non-CAS objects (AC envelopes, audit ndjson parts), keys include a ULID `event_id` or `action_digest` → effectively immutable per key; no conflict. |
| **Edge cases** | (a) Audit parts are append-only with `Object Lock Governance 7y`; cross-region replica of an Object-Lock-protected object is configured separately (per Cloudflare docs); failure to replicate Object-Lock metadata = SEV-1 audit-chain risk. (b) AC envelopes are short-TTL (S-04 worker); a missed replica entry causes a miss-on-failover (not a poisoning risk). (c) Multipart in-flight uploads are not replicated until commit (CTRL `PAT-SWEEPER-001` 7-day window). |

### 3.3 D1 — per-region operational metadata (blob_meta, ac_meta, audit_outbox, usage_counter, tenant_quota, tenant_storage_state)

| Field | Value |
|---|---|
| **Topology** | **Active-passive (read-replica only).** Each region has its own primary D1 database (`corelink-meta-{region}`). Cross-region replication = Cloudflare D1 read-replicas (sibling region holds an async read replica of the primary's data). Writes are **single-region pinned** to the primary. |
| **RPO target** | **≤ 6 h** (per `SLO-BACKUP-VERIFICATION` row "D1 (RPO 6h)" in `slo_catalog.md §4.22`). Live replication lag target: **≤ 60 s p99 at write-lease declaration** (DR-16 RPO budget). |
| **RPO measured** | Backup verification: measured daily via `scripts/backup-daily-verify.sh`. Live read-replica lag: **UNMEASURED — GAP-R3.** No Prometheus SLI today. |
| **Propagation delay p99 target** | ≤ 60 s p99 (active-failover declaration trigger §1.3 of `ACTIVE-FAILOVER-DRILL-SPEC.md` requires replication lag ≤ 5 min RPO budget at declaration). |
| **Propagation delay p99 measured** | Unmeasured live; backed by Cloudflare D1 read-replica SLA (documented "seconds-typical"). |
| **Conflict resolution** | **No conflict possible** — single-writer per region by construction; Sessions API (`PAT-SESSION-CONSISTENCY-001`) gives sequential consistency on reads, bookmark-based. Cross-region writes are blocked by `INV-DATA-RESIDENCY` (tenant pinning enforced by WI-S14-002 insert checks). |
| **Edge cases** | (a) **Schema migration drift**: expand-migrate-contract per `PAT-MIGRATION-IDEM-001` — during the expand phase, new columns are nullable on both primary and replicas; replica lag during backfill can produce **transient column-not-found** on read replicas if the schema hasn't propagated; mitigated by `PAT-MIGRATION-IDEM-001 §2`. (b) Network partition between primary D1 region and CF control plane → writes stall (D1 strong consistency rejects); reads from local replica continue stale up to the lag horizon. (c) `audit_outbox` drain worker (S-09) reads from primary D1 only; if primary D1 is partitioned, outbox grows in primary until reachable — *no audit loss*, but emit latency grows; bounded by `PAT-RETRY-IDEMPOTENT-001`. (d) `tenant_storage_state.bytes_used` is maintained by a DO singleton that syncs every 5 min to D1; during a primary D1 partition, the DO holds the running counter in memory + DO storage until reconnect (PAT-DO-MIGRATION-AWARE-001). |

### 3.4 KV — short-lived caches (presigned URLs, nonces, PAT-valid, AC-neg)

| Field | Value |
|---|---|
| **Topology** | **Active-active globally** (Cloudflare KV is globally distributed by platform construction). All four regions read from a single logical namespace with eventual consistency. |
| **RPO target** | **≤ 12 h** for backup snapshot (per `SLO-BACKUP-VERIFICATION` row "KV (RPO 12h)"). Live propagation: **≤ 60 s typical** (per Cloudflare docs + `storage_semantics_matrix.md §4.3`). |
| **RPO measured** | Backup verification daily. Live propagation lag: **UNMEASURED — GAP-R4.** |
| **Propagation delay p99 target** | ≤ 60 s typical, ≤ 300 s pessimistic ceiling (KV TTL upper bound `PAT-KV-TTL-001`). |
| **Propagation delay p99 measured** | Unmeasured. |
| **Conflict resolution** | **Last-write-wins** within KV's eventual consistency model. KV is **never source-of-truth** (`PAT-KV-TTL-001`) — every KV entry is derivable from D1/R2; stale = acceptable; corruption is rebuildable from §7.3 `storage_semantics_matrix.md`. |
| **Edge cases** | (a) After region failover, KV entries written *in the now-degraded* primary may take up to TTL (≤ 300 s) to disappear from the new primary's read view — manifests as stale presigned URLs (still valid against R2, harmless) or stale `pat_valid` claims (already expired-fast at 60 s). (b) `ac_neg` (negative cache) is tenant-prefix-scoped — no cross-tenant leak risk during propagation. (c) During cold-restore (DR-15), KV can be rebuilt from D1 walking `cas_blobs` + `action_cache` in ≤ 4h for 10M entries (per `storage_semantics_matrix.md §7.3`). |

### 3.5 DO state — singletons (RateLimiter, TenantQuota, ConfigSingleton, MultipartSession)

| Field | Value |
|---|---|
| **Topology** | **Single-region by design** — one DO instance per `tenant_id` (rate, quota) or per `upload_id` (multipart) or per `flag_key` (config). No cross-region replica. |
| **RPO target** | **Effectively ≤ 60 s** — DO state is a *cache* on top of D1 (`PAT-DO-MIGRATION-AWARE-001`); critical writes are persisted to D1 every 5 min (TenantQuota: `tenant_storage_state.last_synced_at` cadence; data_model.md §4.2). At active-failover declaration, the DO can be rebuilt from D1 in ≤ 60 s. |
| **RPO measured** | DO sync-to-D1 cadence: measured implicitly via `corelink_quota_sync_age_ms`. Cross-region rebuild latency: **UNMEASURED — GAP-R5.** |
| **Propagation delay p99 target** | DO migration (intra-region) is sub-second; cross-region rebuild is **not migration** — it is rehydration from D1 on the new primary. Target ≤ 60 s. |
| **Propagation delay p99 measured** | Unmeasured cross-region. |
| **Conflict resolution** | **Actor-model serializable** within a single DO instance (single-threaded). Cross-region split-brain is **prevented by routing**: DO ID is opaque to the worker, and CF routes the DO to its home jurisdiction — at failover, the new primary instantiates a **fresh** DO and reads canonical state from D1. No two DOs hold the same tenant_id simultaneously (CF platform guarantee). |
| **Edge cases** | (a) Rate-limiter bucket state is *intentionally* lost on failover — the new region's bucket starts at `bucket_capacity` (intentional reset is documented as a graceful-degradation behavior; a tenant under attack might briefly get an extra burst budget, mitigated by edge CF rules in `PAT-RATE-LIMIT-001`). (b) MultipartSession in-flight at failover: the upload **must be restarted** by the client — multipart-id is region-bound; sweeper (`PAT-SWEEPER-001`) cleans up after 7 d. (c) ConfigSingleton (kill-switches, feature flags) lives in Neon ground-truth (consent ledger pattern); DO reload at failover hits Neon directly; up-to-5-min config propagation delay applies (`SLO-ADMIN-CONFIG-PROPAGATION` §4.14). (d) TenantQuota.used_bytes_now is a running counter; on failover the new DO reads `tenant_storage_state.bytes_used` from D1 — same staleness window as D1 replica (≤ 60 s). |

### 3.6 Neon Postgres — global control plane (Account, Tenant, User, PAT, Plan, Subscription, dsr_tickets, audit consent ledger)

| Field | Value |
|---|---|
| **Topology** | **Single primary (US) + EU read-replicas** (per `data_model.md §3` table). Writes go to US primary; reads in EU regions hit the EU read replica via Neon's branching/read-replica feature. |
| **RPO target** | Neon point-in-time recovery: **14 days** (per `storage_semantics_matrix.md §7.2`). Live read-replica lag: typical sub-second per Neon documentation. |
| **RPO measured** | Neon-platform-measured (managed service); CoreLink has no internal SLI for Neon replication today. **GAP-R6.** |
| **Propagation delay p99 target** | ≤ 1 s replica lag (Neon-managed). |
| **Propagation delay p99 measured** | Not measured by CoreLink. |
| **Conflict resolution** | Postgres MVCC + Serializable isolation on hot writes (billing, account, PAT issuance). Cross-region write conflict cannot occur (single primary). |
| **Edge cases** | (a) Neon primary outage = control plane down for ~5–15 min during managed failover (Neon SLA); CoreLink continues serving cached PAT-validation (60 s KV TTL) + cached tenant config in DO; new tenant signup blocked. (b) During Neon primary failover, EU read replicas may briefly serve stale config — affects `consent_revoked` propagation (mitigated by 5-min CTRL-PRIV-CONSENT-002 ceiling). (c) DSR ticket state machine is in Neon — during Neon partition, in-flight DSR steps queue idempotently via PAT-RETRY-IDEMPOTENT-001 dead-letter queue. |

---

## 4. Replication SLO landscape (current vs proposed)

### 4.1 What exists today

- `SLO-RTO-REGION-FAILOVER` (`slo_catalog.md §4.18`) — ≤ 30 min RTO at failover declaration.
- `SLO-RPO-REGION` (`slo_catalog.md §4.19`) — ≤ 60 s p99 measured *during DR drill*. This is a **drill-time measurement**, not a continuous SLI.
- `SLO-BACKUP-VERIFICATION` (`slo_catalog.md §4.22`) — daily backup-verify pass rate ≥ 99.5%; covers backup-tier RPO (R2 24h / D1 6h / KV 12h).

### 4.2 What is missing — proposed new SLOs (added to `slo_catalog.md §4.23..4.26`)

- **`SLO-REPLICATION-LAG-R2`** — R2-hot replication-worker p99 lag (continuous SLI).
- **`SLO-REPLICATION-LAG-D1`** — D1 read-replica lag (continuous SLI).
- **`SLO-REPLICATION-LAG-KV`** — KV cross-region propagation lag (continuous SLI; KV ≤ 60 s typical).
- **`SLO-REPLICATION-LAG-DO`** — DO-to-D1 sync age (proxy for cross-region DO RPO).

Details in `specs/03_architecture/slo_catalog.md §4.23..4.26` (this audit creates them).

---

## 5. Inconsistency windows (per declaration scenario)

The DR-16 active-failover drill spec §1.3 declares the **trigger conditions** for failover. This audit answers the symmetric question: **given the trigger fires at t=T0, how stale is the new primary's view per domain?**

| Domain | Worst-case stale-at-declaration | Trigger to surface this |
|---|---|---|
| R2-hot blobs (replica-worker top-1%) | Up to **5 min** writes lost (DR-16 RPO 5 min budget = a few replica-worker cron ticks). Bounded by `corelink-replica-worker` cron cadence in production wiring. |
| R2-cold + AC + audit | Up to **24 h** lost in worst case (R2 platform CRR ceiling). In practice cold blobs are not on the critical path of failover (re-fetchable from source); audit gaps would breach `INV-AUDIT-APPEND-ONLY` and trigger SEV-1. |
| D1 (blob_meta, ac_meta, audit_outbox, usage_counter) | Up to **60 s** (read-replica lag at declaration). Outbox drain: any not-yet-emitted events in primary's D1 are *replayed* by drain-worker once new primary takes the write lease — no audit loss as long as outbox itself replicated. |
| KV (presigned URLs, nonces, PAT-valid, ac_neg) | Up to **60 s typical / 300 s ceiling**. Stale presigned URLs still resolve against R2; stale PAT-valid expires fast (60 s TTL). |
| DO state (rate, quota, multipart, config) | RateLimiter: intentional reset (budget loss is graceful-degradation acceptable). TenantQuota: ≤ 60 s of usage-counter under-count (rebuilds from D1). Multipart: in-flight uploads ABORTED (client re-upload). ConfigSingleton: ≤ 5 min stale (SLO-ADMIN-CONFIG-PROPAGATION). |
| Neon (control plane) | Sub-second typical. Affects PAT issuance, billing, DSR — all out-of-band of the hot path. |

### 5.1 The hidden hazard — audit_outbox during partition

A nuanced case worth calling out for DR-16 dry-run:

If the primary region D1 is **partitioned but not destroyed** (the active-failover scenario), `audit_outbox` rows continue to accumulate in the primary's D1. The drain-worker on the *new* primary reads from the *new* primary's outbox — it does **not** drain the old primary's stalled outbox. When the old primary recovers (failback) those events must be drained before any new writes — otherwise `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` is preserved (events not lost) but `INV-AUDIT-CHAIN` ordering can be perturbed (events emitted out-of-order relative to wall-clock).

This is **already covered** by RB-ACTIVE-FAILOVER step 7 (Reverse) — failback runbook explicitly requires draining old-primary outbox before re-engaging writes. Adding a test for it is captured in follow-up ticket **R-PREP-REPL-P1-002**.

### 5.2 The hidden hazard — replica-worker silent skip

`replicate_batch` (`crates/corelink-replica-worker/src/replication.rs` L205-L228) **continues** the batch on non-residency errors (e.g., `R2Copy`, `HashMismatch`, `Audit`). Individual blob failures are audit-emitted as `ReplicationFailed` but the batch counter does **not** flag the partial failure to a higher level. In production wiring, the cron job should compare `len(input_batch)` vs `replicate_batch` return value and emit a Prometheus counter — captured in follow-up ticket **R-PREP-REPL-P0-001**.

---

## 6. Edge cases — replication during schema migrations

D1 schema migrations during failover-drill window are explicitly forbidden by the change-freeze posture during DR drills, but the audit must answer "what if a migration is mid-flight when DR-16 declares?":

1. **Expand phase (add nullable column)**: the new column exists in primary D1 *and* in replica (D1 platform propagates DDL). Failover handover proceeds normally — the new primary already has the column.
2. **Migrate phase (dual-write + backfill)**: the dual-write transactional pattern means both new and old columns are populated; even if the new primary takes the lease mid-backfill, no row is half-written. Backfill resumes from `corelink_migration_progress_ratio` checkpoint on the new primary.
3. **Contract phase (drop old column)**: this is the risky window — a not-yet-propagated `DROP COLUMN` on the old primary, if the lease moves before propagation, would leave the new primary with the old column still present. Mitigated by sequencing: `DROP COLUMN` is only allowed in a separate release after the new code is fully deployed *and* the column is unread for ≥ 24h (per `PAT-MIGRATION-IDEM-001`).

For Neon, migrations follow the same expand-migrate-contract; the global control-plane is unaffected by region failover.

---

## 7. Verification — how to keep this audit honest

The `scripts/verify-replication-lag.py` script (added this sprint) does a **type-level lag check** runnable daily:

- Queries replica heartbeat from each region (or fakes via in-memory `InMemoryReplicationWorker` when staging endpoints aren't available).
- Reports lag per domain (R2 / D1 / KV / DO).
- Exits non-zero if any domain exceeds its RPO budget (defaults from `slo_catalog.md §4.23..4.26`).

Wire into the staging cron (R-6 wave): runs daily at 02:00 UTC alongside `scripts/backup-daily.sh`. Failure emits SEV-3; two consecutive fails → SEV-2; three consecutive fails → SEV-1 (mirrors `SLO-BACKUP-VERIFICATION` alert ladder).

---

## 8. Cross-links

| Spec | Section | Relevance |
|---|---|---|
| `specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md` | §1.3 | DR-16 declaration trigger requires replication lag ≤ 5 min RPO budget — this audit documents what "lag" means per domain. |
| `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` | §1.1 | DR-15 assumes backups (not live replication) — this audit clarifies the boundary. |
| `specs/03_architecture/slo_catalog.md` | §4.18-§4.19 + new §4.23..§4.26 | RTO/RPO drill measurement vs the new continuous replication-lag SLIs. |
| `specs/03_architecture/data_model.md` | §3, §4.2, §5, §6 | Per-domain layout = the substrate this audit reasons about. |
| `specs/03_architecture/resilience_patterns.md` | §3.4 + §3.6 | PAT-REGION-FAILOVER-001 + PAT-DO-MIGRATION-AWARE-001 + PAT-KV-TTL-001 are the patterns enforcing the topology audited here. |
| `specs/03_architecture/storage_semantics_matrix.md` | §7.2 | Pre-existing per-backend RPO statements; this audit reconciles them with implementation. |
| `ROADMAP-TO-GA.md` | §6 (Wave R-6) | DR-16 is listed under "BCP / DR drill cadence"; this audit is the type-level companion. |
| `specs/_audits/sealed/replication-followup-tickets.md` | All | The actionable backlog generated from this audit's gaps. |

---

## 9. Sign-off requirements

- **SRE Lead** (replication-lag SLOs + verifier-script ownership)
- **Architect** (residency-graph + cross-domain reasoning)
- **Privacy Officer** (residency invariant alignment for WEUR↔SAM and US pair)
- **Compliance Lead** (SOC 2 A1.2 / CC7.5 / CC9.1 alignment with DR-16 + DR-15)

Staffing-blocked per `slo_catalog.md` reviewers convention; this audit is `doc_status: REVIEW` until ≥ 2 reviewers nomeados.

---

## 10. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo Schneiter (via Claude Opus 4.7) — R-prep replication audit | Initial type-level replication audit produced as input to DR-16 active-failover dry-run. Anchors at `corelink-region` + `corelink-replica-worker` + `corelink-failover-router`. Identifies 6 measured-RPO gaps (R1..R6) materialized in `replication-followup-tickets.md`. Proposes 4 new SLOs (`SLO-REPLICATION-LAG-{R2,D1,KV,DO}`) added in same commit to `slo_catalog.md`. |

---

**Fim de AUDIT-REPLICATION-2026-05-15.** Mudanças em topologia (sibling graph, cross-jurisdiction policy, write-pinning) exigem ADR + Privacy Officer review + Compliance review. Mudanças em RPO targets exigem ADR + ≥ 30 dias de baseline medido.
