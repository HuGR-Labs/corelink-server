# D1 `ac_meta` Schema (WI-S04-002)

> Internal engineering note. Canonical source for the `ac_meta` table
> design + index rationale + CHECK-constraint rationale + governance
> + R2 bucket layout. Cross-link to
> [data_model.md §4.2](../../specs/03_architecture/data_model.md) and
> [ADR-0036](../../specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md).

## 1. Why a separate doc

The schema is the **physical foundation** of the Action Cache.
A bug in the PRIMARY KEY direction or in the `tenant_id NOT NULL`
constraint propagates catastrophically (FM-303 cross-tenant leak).
This doc is the column-by-column rationale behind every load-bearing
decision in `migrations/d1/0002_ac_meta.sql`.

## 2. Table

```sql
CREATE TABLE IF NOT EXISTS ac_meta (
  tenant_id           TEXT        NOT NULL,
  tenant_prefix       BLOB        NOT NULL,
  path_key_id         INTEGER     NOT NULL DEFAULT 1,
  action_digest       TEXT        NOT NULL,
  result_hash         TEXT        NOT NULL,
  blob_refs           TEXT        NOT NULL,
  blob_refs_count     INTEGER     NOT NULL,
  result_size_bytes   INTEGER     NOT NULL,
  created_at          INTEGER     NOT NULL,
  last_hit_at         INTEGER     NOT NULL,
  expires_at          INTEGER     NULL,
  sig_key_id          INTEGER     NOT NULL DEFAULT 1,
  sig_alg             TEXT        NOT NULL DEFAULT 'hkdf-sha256',
  region              TEXT        NOT NULL,
  created_by_pat_id        TEXT   NULL,
  created_by_request_id    TEXT   NULL,
  PRIMARY KEY (tenant_id, action_digest),
  -- 11 inline CHECK constraints (see §4).
);
```

## 3. Column rationale

| Column | Rationale |
|---|---|
| `tenant_id` (PK[1]) | Layer 4 storage envelope. **NEVER** NULLABLE. UUIDv7 canonical TEXT form per `data_model.md §2.1`. |
| `tenant_prefix` (BLOB(16)) | Materialized HMAC truncation per ADR-0035 H-3. Pre-computed at INSERT time so the cron worker (WI-S04-005) reads the column directly without TDK access — avoids cron trust-boundary expansion. |
| `path_key_id` | TDK rotation version for path derivation. Default 1; forward-compat S-14 cross-region rotation. |
| `action_digest` (PK[2]) | REAPI v2 `Digest.hash` of canonical Action proto. 64 hex chars (BLAKE3-256 OR SHA-256; v1 ships BLAKE3 only — handler validates). |
| `result_hash` | BLAKE3-256 of canonical merkle_root per ADR-0037. **Immutable** post-INSERT (handler enforces; ON CONFLICT DO UPDATE never lists `result_hash`). |
| `blob_refs` (TEXT) | JSON array of digests. D1/SQLite has JSON1 ext but no path indexing — filter on PK first (always tenant-scoped); JSON parse only in S-06 reconcile. |
| `blob_refs_count` | Denormalized count. CHECK enforces 0–4096. Cheap COUNT(*) per AC entry; reconcile detects drift. |
| `result_size_bytes` | Envelope payload size. Cost / quota observability. CHECK enforces ≤ 1 MiB. |
| `created_at`, `last_hit_at` | Unix epoch ms. CHECK enforces `last_hit_at >= created_at`. |
| `expires_at` | NULL = no expiry (free-tier default pre-S-07); `Some(ms)` = wall-clock TTL per ADR-0019. |
| `sig_key_id`, `sig_alg` | HKDF binding (WI-S04-004). `sig_key_id` ≥ 1 (0 reserved sentinel per ADR-0021). `sig_alg` whitelisted to `'hkdf-sha256'` v1; new alg requires ADR + new migration. |
| `region` | TEXT + CHECK constraint (D1/SQLite has no native ENUM; ADR-0036 Rule 3). 5 canonical regions: `'sam', 'iad', 'lhr', 'nrt', 'syd'`. |
| `created_by_pat_id`, `created_by_request_id` | Audit cross-check (WI-S03-007 chain). NULLABLE for legacy entries pre-WI-S03-002 PAT migration; modern entries always populate. |

## 4. CHECK constraints (11 total)

| Mnemonic | Predicate | Rationale |
|---|---|---|
| `chk_ac_action_digest_len` | `length(action_digest) = 64` | BLAKE3-256 / SHA-256 hex form. |
| `chk_ac_result_hash_len` | `length(result_hash) = 64` | BLAKE3-256 hex form. |
| `chk_ac_blob_refs_size` | `length(blob_refs) <= 10240` | 10 KiB max JSON array (REAPI guidance). |
| `chk_ac_blob_refs_count` | `0 <= blob_refs_count <= 4096` | denormalized; reconcile detects drift. |
| `chk_ac_result_size` | `0 <= result_size_bytes <= 1048576` | 1 MiB envelope cap. |
| `chk_ac_region` | `region IN ('sam','iad','lhr','nrt','syd')` | enforced 5 canonical regions; new region requires ADR + new migration. |
| `chk_ac_sig_alg` | `sig_alg = 'hkdf-sha256'` | v1 only; v2+ via ADR migration. |
| `chk_ac_lifecycle` | `last_hit_at >= created_at AND (expires_at IS NULL OR expires_at >= created_at)` | timestamp invariant. |
| `chk_ac_tenant_prefix_len` | `length(tenant_prefix) = 16` | ADR-0035 H-3 16-byte HMAC truncation. |
| `chk_ac_path_key_id_positive` | `path_key_id >= 1` | 0 reserved future use. |
| `chk_ac_sig_key_id_positive` | `sig_key_id >= 1` | 0 reserved sentinel per ADR-0021. |

ALL CHECK constraints are **inline in CREATE TABLE** per ADR-0036 Rule 1 (D1/SQLite has no `ALTER TABLE ADD CONSTRAINT`). New constraint OR contents change requires a new migration via the SQLite 12-step recipe (ADR-0036 Rule 3).

## 5. Indices (3 total)

| Name | Definition | Use case |
|---|---|---|
| `idx_ac_meta_tenant_expires` | `ON ac_meta(tenant_id, expires_at) WHERE expires_at IS NOT NULL` | TTL eviction worker (WI-S04-005). Partial — most rows have NULL expires_at; index size halved. |
| `idx_ac_meta_tenant_last_hit` | `ON ac_meta(tenant_id, last_hit_at)` | LRU support + cache analytics. Not partial (every row has last_hit_at). |
| `idx_ac_meta_region` | `ON ac_meta(region)` | Cross-region migration support S-14; analytics queries. |

PK lookups (`WHERE tenant_id = ? AND action_digest = ?`) hit the
covering index built into the PK declaration; no separate index
needed.

## 6. R2 bucket layout (5 regions)

| Binding | Bucket | Region |
|---|---|---|
| `AC_BUCKET_SAM` | `corelink-ac-sam` | South America |
| `AC_BUCKET_IAD` | `corelink-ac-iad` | US East (Ashburn) |
| `AC_BUCKET_LHR` | `corelink-ac-lhr` | Europe (London) |
| `AC_BUCKET_NRT` | `corelink-ac-nrt` | Asia (Tokyo) |
| `AC_BUCKET_SYD` | `corelink-ac-syd` | Oceania (Sydney) |

Bindings declared in `wrangler.toml` (default + `[env.prod]`).
Provisioning is handled by `scripts/provision_ac_buckets.sh` —
idempotent (`wrangler r2 bucket create` returns ok if exists).

### Lifecycle policy

- Rule id: `abort-multipart-incomplete-7d`.
- Action: `AbortIncompleteMultipartUpload`.
- Days: 7 (REAPI guidance; CoreLink doesn't expose direct R2 upload to
  clients so the rule is mostly defensive against orphaned partials).

### CORS policy

- `rules: []` (empty) — no browser preflight allowed; all access via
  Worker binding (server-side).
- Applied via Cloudflare REST API (`PUT /accounts/.../r2/buckets/.../cors`).
- Re-asserted nightly by `.github/workflows/ac-bucket-acl-cron.yml`
  (also self-heals on drift detection).

### Public access

- OFF (Cloudflare default).
- Verified by `scripts/check_ac_infra.sh <env>` pre-deploy.
- Drift detection runbook: `RB-FM-AC-BUCKET-LEAK`.

## 7. Migration governance

- Idempotent: `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS`.
- Forward-only: `DROP TABLE` is anti-scope (sprint contract §10);
  rollback via dummy migration metadata flag (see RB-FM-AC-MIGRATION-BUG).
- Ran via `scripts/migrate_d1.sh <env> <region>`.
- Pre-deploy gate: `scripts/check_ac_infra.sh <env>`.
- Additive-only: scripts/check_migrations_additive.py CI gate.
- Schema simulator: `crates/corelink-ac-schema` (host-side fake; 10 k iter property tests over PK uniqueness + CHECK enforcement + tenant isolation).

## 8. Rollback

See [RB-FM-AC-MIGRATION-BUG](../../specs/05_quality/runbooks/RB-FM-AC-MIGRATION-BUG.md).

Summary: never `DROP TABLE` in prod. Bug detected → ship migration
0003 marking `ac_meta` DEPRECATED via metadata table; handler reads
the flag and short-circuits with `503 COR_AC_DEPRECATED`. Data is
preserved for manual recovery.

## 9. Sizing projection

| Population | Row size | Total size | Action |
|---|---|---|---|
| 100 tenants × 100 k actions = 10 M rows | ~270 bytes | ~2.7 GB | Fits free-tier 5 GB; paid tier billing kicks in only at growth. |
| 80 % D1 hard limit (~30 M rows) | ~270 bytes | ~8 GB | Sharding trigger per ADR-0036. |
| 1 B rows hypothetical | ~270 bytes | 270 GB | Mandatory sharding well before this scale. |

Storage growth metric `corelink.d1.ac_meta.size_bytes` alerts on >
50 % growth/quarter.

## 10. Cross-references

- `migrations/d1/0002_ac_meta.sql` — canonical SQL artefact.
- `crates/corelink-ac-schema/` — embedded migration + simulator.
- `specs/03_architecture/data_model.md §4.2` — schema canonical source.
- `specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md` — governance rules.
- `specs/03_architecture/adrs/ADR-0035-ac-handler-invariants.md` — H-3 tenant_prefix materialization.
- `specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md` — sig_key_id rationale.
- `specs/03_architecture/adrs/ADR-0037-merkle-action-protocol-result-hash.md` — `result_hash = BLAKE3(merkle_root)` rationale.
- `specs/04_sprints/S04/work_items/WI-S04-002-d1-ac-meta-r2-bucket.md` — WI canonical text.
