# D1 Multipart Schema (WI-S05-004)

> Internal engineering note. Canonical source for the `chunks` +
> `manifest_chunks` + `multipart_sessions` tables design + index
> rationale + CHECK-constraint rationale + governance + R2 bucket
> layout. Cross-link to
> [data_model.md §4.3](../../specs/03_architecture/data_model.md),
> [ADR-0036](../../specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md),
> [ADR-0035](../../specs/03_architecture/adrs/ADR-0035-ac-handler-invariants.md)
> H-3 (tenant_prefix materialization), and
> [ADR-0022](../../specs/03_architecture/adrs/ADR-0022-chunk-vs-part-size.md).

## 1. Why a separate doc

The schema is the **physical foundation** of S-05 multipart + chunking
+ Merkle. A bug in the PRIMARY KEY direction or in the partial-UNIQUE
on `multipart_sessions` propagates catastrophically (FM-303
cross-tenant chunk leak; multipart orphan accumulation FM-060). This
doc is the column-by-column rationale behind every load-bearing
decision in `migrations/d1/0003_multipart_chunks_manifest.sql`.

## 2. Three tables + R2 buckets at a glance

| Table | Role | PK | Partial UNIQUE / index notes |
|---|---|---|---|
| `chunks` | intra-tenant chunk dedup (CAP-CAS-010) | `(tenant_id, chunk_digest)` tenant-leftmost | partial idx on `refcount = 0` for S-06 GC |
| `manifest_chunks` | ordered chunk references per blob | `(tenant_id, blob_digest, chunk_index)` | reverse-lookup idx `(tenant_id, chunk_digest)` for refcount maintenance |
| `multipart_sessions` | R2 multipart upload tracking (FM-060) | `session_id` opaque PK + `tenant_id` binding column | **partial UNIQUE on `(tenant_id, blob_digest_expected) WHERE state = 'in_progress'`** (Lote 10.5bis P0 fix); orphan-sweep partial idx on `state = 'in_progress'` |

R2 buckets (consumed via wrangler bindings):

- `corelink-chunk-{sam,iad,lhr,nrt,syd}` — chunked CAS body parts.
- `corelink-manifest-{sam,iad,lhr,nrt,syd}` — Merkle root + ordered chunk list.

Wrangler bindings:
`CHUNK_BUCKET_{SAM,IAD,LHR,NRT,SYD}` + `MANIFEST_BUCKET_{SAM,IAD,LHR,NRT,SYD}`.

## 3. Column rationale — `chunks`

| Column | Rationale |
|---|---|
| `tenant_id` (PK[1]) | Layer 4 storage envelope. **NEVER** NULLABLE. UUIDv7 canonical TEXT form per `data_model.md §2.1`. |
| `chunk_digest` (PK[2]) | BLAKE3-256 hex of chunk bytes (length 64). Content-addressed: identical bytes ⇒ identical digest. Handler validates length + lower-hex on every INSERT path. |
| `tenant_prefix` (BLOB(16)) | Materialized HMAC truncation per ADR-0035 H-3. Pre-computed at INSERT time so the cron sweeper (WI-S05-006) reads the column directly without TDK access — avoids cron trust-boundary expansion. |
| `path_key_id` | TDK rotation version for path derivation. Default 1; forward-compat S-14 cross-region rotation. |
| `region` | One of 5 canonical regions per S-05 §5.1 + WI-S04-002 (overlapping list). CHECK constraint enumerates literally. |
| `r2_object_key` | Composed by handler as `chunk-<region>/<tenant_prefix_hex>/<chunk_digest>`; **never** client-supplied (`INV-MULTIPART-PATH-TENANT-SCOPED`). |
| `size_bytes` | 1..=4_194_304 (FastCDC max bound). Lower bound is **1 byte** because the final partial chunk of a small-tail blob may be < 1 MiB (Lote 10.5bis P0 fix; the original WI v1.0.0 said "1 MiB to 4 MiB" which inverts the intent). |
| `refcount` | Intra-tenant dedup count. INSERT inserts with `1`; `INSERT … ON CONFLICT DO UPDATE SET refcount = refcount + 1` increments on idempotent re-Split. S-06 GC decrements on manifest delete; `refcount = 0` is a candidate for chunk delete. CHECK enforces non-negative. |
| `created_at` / `last_referenced_at` | Lifecycle timestamps (unix ms). CHECK `last_referenced_at >= created_at`. |
| `created_by_pat_id` / `created_by_request_id` | Source attribution for audit cross-check; reuse of WI-S03-007 chain. |

## 4. Column rationale — `manifest_chunks`

| Column | Rationale |
|---|---|
| `tenant_id` (PK[1]) | Layer 4 storage envelope. NEVER NULLABLE. |
| `blob_digest` (PK[2]) | BLAKE3-256 hex of canonical blob root; logical FK to `cas_blobs.blob_digest`. |
| `chunk_index` (PK[3]) | 0-indexed chunk order. CHECK `0 <= chunk_index < 81920` (`MAX_CHUNKS_PER_BLOB = 160 GiB / 2 MiB exact` per Lote 10.5-tris P1-SR5-001 cross-crate alignment with `corelink-chunker`). |
| `chunk_digest` | Logical FK to `chunks.chunk_digest` under the same `tenant_id`. |

The PK btree direction `(tenant_id, blob_digest, chunk_index)` covers
the canonical ordered scan (`SELECT … WHERE tenant_id = ? AND
blob_digest = ? ORDER BY chunk_index`) without an additional index.

The reverse-lookup index `idx_manifest_chunks_tenant_chunk` on
`(tenant_id, chunk_digest)` powers refcount maintenance during
manifest delete (S-06 GC): "every blob row that referenced this
chunk".

## 5. Column rationale — `multipart_sessions`

| Column | Rationale |
|---|---|
| `session_id` (PK) | Host-minted opaque session id (production: UUIDv7); cryptographically random; not enumerable. |
| `r2_upload_id` | Opaque upload_id from R2 `InitiateMultipartUpload`. NULLable until R2 init returns; handler enforces non-null post-init. |
| `tenant_id` | Cross-tenant binding column. **Every** API method enforces `ctx.tenant_id == row.tenant_id`. NEVER NULLABLE. |
| `tenant_prefix` (BLOB(16)) + `path_key_id` | Same materialization rationale as `chunks` — sweeper reads sem TDK access. |
| `blob_digest_expected` | Hash the client claims at SplitBlob start; verified at Complete. Length-64 CHECK + the partial UNIQUE (see §6). |
| `region` / `bucket` / `object_key` | R2 location; bucket is one of `corelink-chunk-<region>` / `corelink-manifest-<region>`; object_key is server-composed (`INV-MULTIPART-PATH-TENANT-SCOPED`). |
| `started_at` / `last_activity_at` / `finalized_at_ms` / `expires_at_ms` | Lifecycle (unix ms). CHECK `last_activity_at >= started_at`; CHECK `expires_at_ms >= started_at`; CHECK `finalized_at_ms IS NULL OR finalized_at_ms >= started_at`. Default TTL is 7 days (`DEFAULT_SESSION_TTL_MS`); sweeper aborts orphans past `expires_at_ms`. |
| `state` | Enum domain `'in_progress' \| 'completed' \| 'aborted'`. CHECK enforces the **enum domain**; the **graph** (`in_progress → completed \| aborted`; never reverse) is enforced at handler level (a CHECK on column would not model the transition). The self-consistency invariant `(state = 'in_progress') = (finalized_at_ms IS NULL)` IS CHECK-enforced via `chk_multipart_finalized_iff_terminal`. |
| `created_by_pat_id` / `created_by_request_id` | Audit cross-check. `created_by_request_id` is NOT NULL on this table because every multipart session crosses an authenticated boundary. |

## 6. The partial UNIQUE INDEX (Lote 10.5bis P0 fix)

```sql
CREATE UNIQUE INDEX IF NOT EXISTS uq_multipart_sessions_in_progress
  ON multipart_sessions(tenant_id, blob_digest_expected)
  WHERE state = 'in_progress';
```

**Rationale:** the original WI-S05-004 v1.0.0 specified a plain
`UNIQUE (tenant_id, blob_digest_expected, state)` constraint which
would have allowed multiple `(tenant=A, blob=X, state='completed')`
rows AND blocked a fresh upload of `blob=X` after a previous one
completed (the partial-key scope was wrong). The Lote 10.5bis P0 fix
drops the column-list UNIQUE and adds the partial UNIQUE INDEX scoped
to `state = 'in_progress'`. This achieves the intent:

- **At most one `in_progress` session per `(tenant, blob)` at a time**
  — a concurrent SplitBlob attempt for the same blob coalesces into
  the existing session (handler returns the pre-existing session_id
  per S3/R2 native multipart idempotency contract).
- **Multiple `completed` / `aborted` rows coexist as audit trail**
  — fresh re-uploads of a previously-finalized blob are NOT blocked.
- **Cross-tenant orthogonality** — Tenant A's in_progress for blob X
  does NOT block Tenant B's in_progress for blob X (the partial
  UNIQUE is keyed on `(tenant_id, blob_digest_expected)`).

## 7. CHECK constraint catalogue

19 inline CHECK constraints (per ADR-0036 Rule 1: SQLite/D1 has no
`ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at CREATE TABLE):

```
chunks (7):
  chk_chunks_digest_len:                length(chunk_digest) = 64
  chk_chunks_tenant_prefix_len:         length(tenant_prefix) = 16
  chk_chunks_path_key_id_positive:      path_key_id >= 1
  chk_chunks_region:                    region IN ('sam','iad','lhr','nrt','syd')
  chk_chunks_size_bytes:                1 <= size_bytes <= 4194304
  chk_chunks_refcount_non_negative:     refcount >= 0
  chk_chunks_lifecycle:                 last_referenced_at >= created_at

manifest_chunks (3):
  chk_manifest_blob_digest_len:         length(blob_digest) = 64
  chk_manifest_chunk_digest_len:        length(chunk_digest) = 64
  chk_manifest_chunk_index_bounded:     0 <= chunk_index < 81920

multipart_sessions (9):
  chk_multipart_blob_digest_len:        length(blob_digest_expected) = 64
  chk_multipart_tenant_prefix_len:      length(tenant_prefix) = 16
  chk_multipart_path_key_id_positive:   path_key_id >= 1
  chk_multipart_region:                 region IN ('sam','iad','lhr','nrt','syd')
  chk_multipart_state_domain:           state IN ('in_progress','completed','aborted')
  chk_multipart_lifecycle_activity:     last_activity_at >= started_at
  chk_multipart_lifecycle_expires:      expires_at_ms >= started_at
  chk_multipart_lifecycle_finalized:    finalized_at_ms IS NULL OR finalized_at_ms >= started_at
  chk_multipart_finalized_iff_terminal: (state = 'in_progress') = (finalized_at_ms IS NULL)
```

## 8. Indices

| Name | Table | Columns | Partial predicate | Use-case |
|---|---|---|---|---|
| `idx_chunks_tenant_last_ref` | chunks | `(tenant_id, last_referenced_at)` | — | LRU analytics + S-06 GC scan order |
| `idx_chunks_tenant_refcount_zero` | chunks | `(tenant_id, last_referenced_at)` | `refcount = 0` | S-06 GC candidate enumeration (small + cheap) |
| `idx_manifest_chunks_tenant_chunk` | manifest_chunks | `(tenant_id, chunk_digest)` | — | refcount maintenance during manifest delete |
| `idx_multipart_sessions_orphan_sweep` | multipart_sessions | `(last_activity_at)` | `state = 'in_progress'` | sweeper consumes (WI-S05-006); cheap because partial |
| `idx_multipart_sessions_tenant_state` | multipart_sessions | `(tenant_id, state)` | — | tenant analytics + state-distribution dashboards |

The PK btrees themselves serve every other canonical scan (manifest
ordered scan via `manifest_chunks` PK; chunk lookup by `(tenant_id,
chunk_digest)` via `chunks` PK; session lookup by `session_id` via
`multipart_sessions` PK).

## 9. Storage projection (sharding criteria — ADR-0040 forward)

`chunks` is the dominant table at scale: 10M tenants × 1M blobs/tenant
× 25 chunks/blob avg ≈ 250 G rows × ~150 bytes/row ≈ ~37 GB per shard
key. **D1 hard limit per database is 10 GB**. Sharding is mandatory
before the table approaches 80% of the hard limit. ADR-0040 (forward;
ratificada em WI-S05-006) documents the canonical sharding strategy
(per-region 5 shards as the v1 cut; per-tier as the v2 cut for hot
Enterprise tenants).

## 10. Refcount semantics

```sql
-- INSERT path (handler in WI-S05-001 SplitBlob):
INSERT INTO chunks (tenant_id, chunk_digest, …, refcount, created_at, last_referenced_at)
  VALUES (?, ?, …, 1, ?, ?)
  ON CONFLICT (tenant_id, chunk_digest) DO UPDATE
    SET refcount = chunks.refcount + 1,
        last_referenced_at = excluded.last_referenced_at;
```

The simulator in `corelink-multipart-schema::sim::MultipartSchema::upsert_chunk`
mirrors this byte-for-byte; the property test
`prop_chunks_pk_uniqueness` covers the idempotent path at 10k iter.

## 11. Migration governance

- File: `migrations/d1/0003_multipart_chunks_manifest.sql`.
- Versioning: schema_version = 3 (mirrors `multipart_schema_version()`
  in `corelink-multipart-schema`). Sequential within the D1 domain
  (1 = blob_meta, 2 = ac_meta, 3 = multipart). D1 + Postgres (Neon
  `auth` schema) are independent migration domains.
- Runner: `scripts/migrate_d1.sh <env> <region>` per the convention
  established in WI-S01-004 + WI-S04-002.
- Idempotency: every `CREATE TABLE` / `CREATE INDEX` / `CREATE UNIQUE
  INDEX` uses `IF NOT EXISTS`. The `migration_canonical` test crate
  asserts the property at compile-test scope.
- Additivity: `scripts/check_migrations_additive.py` rejects every
  destructive token (`DROP TABLE`, `RENAME`, `ALTER COLUMN … TYPE`,
  …); CI gate.

## 12. Forward references

- WI-S05-005 (Merkle manifest builder/verifier) consumes the
  `manifest_chunks` ordered-scan API.
- WI-S05-006 (sweeper + ship gate) consumes the
  `idx_multipart_sessions_orphan_sweep` partial index, the
  `MultipartAdapter::list_orphans` API in `corelink-r2-multipart`, and
  publishes the canonical `RB-FM-MULTIPART-MIGRATION-BUG` runbook +
  `scripts/check_multipart_infra.sh` deploy guard + `ADR-0040` D1
  sharding criteria.
- S-06 GC reads `idx_chunks_tenant_refcount_zero` to enumerate delete
  candidates; manifest_chunks delete decrements `chunks.refcount`.
- S-07 (cross-tenant dedup, post-launch) extends the
  `(tenant_id, chunk_digest)` PK with a global secondary index for
  reachability analytics; the canonical UNIQUE remains tenant-scoped.

## 13. Open follow-ups (deferred to WI-S05-006)

1. **Production D1 binding shim** — adapter wrapping
   `wrangler/miniflare` D1 client + maps every canonical CHECK /
   UNIQUE violation to the local error taxonomy. Trait surface lives
   in `corelink-multipart-schema::sim` (the simulator IS the trait
   spec); the production adapter SATISFIES the same semantic.
2. **Deploy guard `scripts/check_multipart_infra.sh`** — pre-deploy CI
   gate validating migration applied + 10 R2 buckets present + 10
   wrangler bindings configured + CORS empty + public-access OFF.
3. **`RB-FM-MULTIPART-MIGRATION-BUG.md`** runbook — rollback recipe
   (dummy migration 0003a marks DEPRECATED via `meta` table; never
   DROP).
4. **`ADR-0040`** — multipart D1 sharding criteria + per-region 5-shard
   v1 + per-tier v2 forward.
5. **EXPLAIN QUERY PLAN smoke** — assert PK seek for the 4 canonical
   query shapes (`chunks` get; `chunks` GC scan; `manifest_chunks`
   ordered scan; `multipart_sessions` orphan scan).
