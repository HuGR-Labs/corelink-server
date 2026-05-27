# Agent R4 — Lote 10.4 S-04 Part 1 (WIs 001–003) WI Review

**Reviewer**: Agent R4 (Claude Opus 4.7, 1M context, independent adversarial review — round 4)
**Date**: 2026-04-25
**Scope**: WI-S04-001 (REAPI ActionCache handlers), WI-S04-002 (D1 ac_meta + R2 bucket), WI-S04-003 (corelink-ac Merkle dual-side)
**Source files**:
- `/Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/04_sprints/_sealed/S04/work_items/WI-S04-001-reapi-actioncache-handlers.md`
- `/Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/04_sprints/_sealed/S04/work_items/WI-S04-002-d1-ac-meta-r2-bucket.md`
- `/Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/04_sprints/_sealed/S04/work_items/WI-S04-003-corelink-ac-merkle-dual-side.md`
**Cross-references**: `_spec_contract.md` v1.1.0 (S-04), `data_model.md §4.2/§5.2`, `security_model.md §CTRL-AC-001/002`, `error_taxonomy.md §3.2`, `invariant_registry.md §3.3`, ADR-0019, S-01 lib `corelink-tenant-path`, S-03 part1+part2 R4 reviews (calibration baseline 7.6 / 7.95; best-in-class WI-S03-003 = 8.5).

---

## Veredito Geral

The three WIs are dense, ambitious, and clearly an evolution beyond the S-03 calibration bar — they reach the §22-§32 density target and successfully internalize the R4 lessons (13-row sign-off, ADR whitelisting, Mann-Whitney 3-prong, STRIDE+LINDDUN delta, 11 chaos experiments, 14-row risk register, full TCO 12m). WI-S04-003 in particular is the strongest of the three and a credible candidate to **match or beat WI-S03-003's 8.5 best-in-class** — the BLAKE3 + RFC 6962 domain-separation reasoning is concrete, the dual-side defense-in-depth case is well argued, the bounded parser bounds are explicit and incrementally-enforceable, and the Crypto SME mandatory-emphatic sign-off is correctly emphasized. WI-S04-001 is comprehensive but has **a class of cross-document drift defects** that will cost it half a point: it cites a non-existent `WI-S01-005` for `audit_outbox` (the table actually lives in WI-S01-004), it appropriates the Postgres `with_tenant_ctx!` / `SET LOCAL` macro from WI-S03-005 onto a D1 (SQLite) schema where neither RLS nor `SET LOCAL` exist as Postgres-style primitives, it changes the negative-cache TTL from 300s to 60s and re-keys it from `ac_neg:<digest>` to `ac_neg:<tenant_prefix>:<digest>` without amending `data_model.md §6.1`, and it invents 8+ new `COR_AC_*` error codes that are absent from the canonical `error_taxonomy.md §3.2` (which lists only 3 AC codes). WI-S04-002 has **two ship-blocker SQL bugs**: (a) D1/SQLite does not support `ALTER TABLE … ADD CONSTRAINT chk_*` for CHECK constraints (CHECK must be inline at CREATE TABLE), (b) the schema **omits the path-derivation column** entirely — the canonical R2 layout in `data_model.md §5.2` is `ac-<region>/<hmac(tenant_key, tenant_id)[:16]>/<action_digest>.json` (16-byte HMAC prefix), but the D1 row has only `tenant_id` + `action_digest` and never persists the prefix. WI-002 also restates the same false `with_tenant_ctx!` claim. Net: ambition is correctly calibrated and most adversarial surfaces are surfaced, but **technical inaccuracies are dense enough on the storage layer that a real DBA + Architect review will surface them**, and the WI-001/WI-002 pair has internal-consistency drift that fails the "prior-WI consistency" axis. WI-003 is comparatively clean.

**Average part1 score: 8.05/10** (7.7 / 7.6 / 8.6 across 001/002/003 with WI-003 leading).

**Verdict aggregate**: pass-with-fixes. WI-001 + WI-002 must absorb a Lote 10.4bis P0 patch pass before promotion to SEAL; WI-003 is pass-with-fixes-light (P0 small).

---

## Per-WI Numerical Score

| WI | Score | Cripto | Complete | Clarity | SOTA | Internal | Prior-WI | Customer | Verdict |
|---|---|---|---|---|---|---|---|---|---|
| WI-S04-001 | **7.7** | 7.5 | 8.5 | 8.5 | 8.0 | 6.5 | 6.5 | 8.5 | pass-with-fixes (P0) |
| WI-S04-002 | **7.6** | 7.0 | 8.0 | 8.0 | 8.0 | 6.0 | 6.5 | 8.5 | pass-with-fixes (P0) |
| WI-S04-003 | **8.6** | 9.0 | 9.0 | 8.5 | 9.0 | 8.0 | 8.5 | 8.5 | pass-with-fixes-light |

Average: **8.05/10**.

Breakdown axes (0-10):
- **Cripto rigor**: BLAKE3 + RFC 6962 domain sep + bounded parser justifications (003 leads); HKDF integration boundary + Mann-Whitney 3-prong (001 reasonable; sig delegation correct).
- **Completeness**: 32 sections present; chaos ≥ 10; risk ≥ 10 rows 6-col; Mann-Whitney 3-prong; cost TCO 12m. All three meet the bar.
- **Clarity**: prose dense but readable; sequence flows in WI-001 are unambiguous; spec doc plan in WI-003 (Annex A/B test vectors) is exemplary.
- **SOTA-adherence**: 13-row sign-off; advisory Crypto SME; cost regression gate; 100k nightly + 10k PR property; Šidák 3-trial; Cohen's d=0.2 power 1−β≥0.80; bootstrap 95% CI. Adheres throughout.
- **Internal consistency** (this is where 001/002 fail): citations of WI-S01-005, the `with_tenant_ctx!` claim on D1, divergent negative-cache key/TTL, undeclared error codes.
- **Prior-WI consistency** (cross-doc drift): same as above; data_model.md §5.2 path layout drift.
- **Customer-facing readiness**: persona narratives are concrete; SLA addenda explicit; SLSA Level 3 alignment in WI-003 is a customer-facing differentiator.

---

## P0 Findings (must-fix before promotion to Lote 10.4bis)

### Cross-WI / WI-S04-002 — D1 ALTER TABLE ADD CONSTRAINT does not work in SQLite

**Severity**: ship-blocker. **WI**: WI-S04-002 §1.

The migration SQL in §1 emits 6 `ALTER TABLE ac_meta ADD CONSTRAINT chk_*` statements. **D1 is built on SQLite. SQLite does not support `ALTER TABLE … ADD CONSTRAINT`** for CHECK constraints — only for FOREIGN KEY in very recent versions, and the CHECK constraint syntax is unsupported as `ALTER` form. The standard SQLite path is one of:
1. Define CHECK inline at `CREATE TABLE` time (`tenant_id TEXT NOT NULL CHECK (length(tenant_id) > 0)`), or
2. Use the SQLite "12-step recipe" (CREATE new table with constraints → INSERT SELECT old → DROP old → RENAME) — incompatible with the WI-002 anti-scope "❌ DROP TABLE in prod".

The migration as written **will fail at apply time** with a syntax error; staging will reject. Fix: rewrite the migration to define all 6 CHECK constraints inline within the `CREATE TABLE ac_meta (...)` block. Add a positive integration test: `wrangler d1 migrations apply --env staging` succeeds end-to-end on a fresh D1 instance. (The Gherkin scenario "Migration 003 applies idempotently" trivially passes if you reach that line, but only because the bug is at the SQL parser level — `IF NOT EXISTS` short-circuits subsequent applies, masking the break on first apply.)

This single defect would have been caught by even a 30-second `sqlite3 :memory: < migrations/003_ac_meta.sql` dry run, but the WI as drafted has no such CI gate. Add one to §6.1.6 deploy guard.

### Cross-WI — `tenant_prefix` HMAC column is missing from `ac_meta` and from the storage layout in WI-S04-002

**Severity**: P0. **WIs**: WI-S04-002 §1 (schema), WI-S04-001 §1 (handler R2 path build).

`data_model.md §5.2` canonically defines AC layout as:
```
ac-<region>/<hmac(tenant_key, tenant_id)[:16]>/<action_digest_hex>.json
```
i.e. R2 path includes the 16-byte HMAC prefix derived via `corelink-tenant-path::derive_prefix(tenant_id)` (S-01 WI-S01-001).

WI-S04-001 §1.1 correctly references this: "tenant_prefix = HMAC(TDK, tenant_id)[:16] reused via corelink-tenant-path".

But WI-S04-002 §1 schema **does not persist this prefix** as a column on `ac_meta`. It carries `tenant_id`, `region`, `action_digest`, `result_hash`, `blob_refs`, etc., but no `tenant_prefix` column. So the R2 path derivation is **stateful at the handler layer only** — every read/write recomputes HMAC at request time.

This is acceptable performance-wise (HMAC is fast), but it has consequences the WIs do not address:
1. **TDK rotation** invalidates the historical R2 path. After tenant_key rotation, the 16-byte prefix changes; existing R2 objects under the old prefix become unaddressable from D1 unless dual-read with old/new prefix. The WI-002 schema includes `sig_key_id INTEGER` for HKDF rotation, but **no equivalent `path_key_id` column** for the path-derivation key version. This is an INV-AC-OUTPUTS-VALID-adjacent problem at TDK rotation time.
2. The WI-001 path build (`r2::get(ac-<region>/<tenant_prefix>/<action_digest>.json)`) silently assumes the current TDK; no documented migration story for rotation cuts.

Fix options (must pick one):
- (a) Add `path_key_id INTEGER NOT NULL DEFAULT 1` to schema, mirroring `sig_key_id`, and document handler must select correct key for derivation per-row.
- (b) Persist the materialized `tenant_prefix TEXT NOT NULL` (32 hex chars / 16 bytes) and CHECK length, so the path is stable across key rotations and the schema enforces consistency.
- (c) Document explicitly in the schema and WI-001 that TDK rotation requires a one-shot R2 re-key migration (S-14 / S-07 forward), and add an INV-AC-PATH-KEY-FIXED-PER-TDK-ERA invariant.

Currently the WIs are silent on this dimension; the rotation story is incomplete.

### WI-S04-001 / WI-S04-002 — `with_tenant_ctx!` macro and `SET LOCAL app.current_tenant` are Postgres/Neon primitives, not D1/SQLite

**Severity**: P0 critical (touches INV-AC-TENANT-SCOPED enforcement claim). **WIs**: WI-S04-001 §1.1, WI-S04-002 §1, §2.

WI-S04-001 §1.1: "Wrapper `with_tenant_ctx!` macro (S-03 WI-S03-005) garante `SET LOCAL app.current_tenant`".
WI-S04-002 §1: "tenant_id TEXT NOT NULL — UUID v7 string (S-03 WI-S03-005); sqlx `with_tenant_ctx!` macro guarantees `SET LOCAL app.current_tenant`."
WI-S04-002 §2.6: "sqlx prepared statement type-check; `with_tenant_ctx!` macro asserts."

This is a **categorical error**:
1. `SET LOCAL app.current_tenant` is a **Postgres** transaction-local GUC binding consumed by **Postgres RLS**. Neither exists in SQLite. D1 has no RLS, no GUCs, no `SET LOCAL` semantic.
2. The `with_tenant_ctx!` macro defined in WI-S03-005 wraps a Neon/Postgres connection in a transaction and binds `app.current_tenant` for RLS policies on `account`, `user_account`, `webauthn_credentials`, etc. It does not — and cannot — apply to a D1 SQLite query.
3. WI-S04-001/002 thus claim a Layer-2 RLS-style defense for `ac_meta` that does not exist. The actual Layer 2 defense for D1 must be **prepared-statement-level WHERE clause discipline + sqlx compile-time query checking**, plus the Layer-4 path HMAC. There is no SQL-engine-level enforcement.

Fix:
- Strike both `with_tenant_ctx!` references in WI-S04-001 §1.1 / §6.1.5 / §9.2 and WI-S04-002 §1 / §2.6.
- Replace with the actual D1 enforcement story: (i) sqlx prepared statement compile-time tenant_id parameter binding, (ii) handler-level mandatory `WHERE tenant_id = ctx.tenant_id` clause, (iii) clippy custom lint forbidding `&str` SQL literals, (iv) integration test that runs query without binding and asserts compile failure.
- Document explicitly that **D1 has no RLS** and Layer 4 (HMAC path prefix) carries proportionally more weight in the 5-Layer Defense for AC than for the auth tables (which have Postgres RLS as Layer 2).
- Add a paragraph to `auth_model.md §8.1` (5-Layer Defense) explaining the asymmetry: Postgres tables get RLS Layer 2; D1 tables (blob_meta, ac_meta) do not — Layer 2 there is sqlx + handler discipline.

This is the **single highest-severity** defect across the three WIs because the security-property claim is misstated. INV-AC-TENANT-SCOPED enforcement story currently rests on a primitive that does not exist on the target storage engine.

### WI-S04-001 — `audit_outbox` is created by WI-S01-004, not WI-S01-005 (citation error in 6 places)

**Severity**: P0 (citation correctness; Lote 10.4bis cross-reference gate). **WI**: WI-S04-001.

WI-001 cites `WI-S01-005 audit_outbox` in §1 sequence, §2.8, §6.1.2.4, §9.8, §18 dependencies. Actual file: `WI-S01-004-d1-schema-blob-meta.md` defines `CREATE TABLE audit_outbox` at line 70-83. WI-S01-005 is "REAPI BatchUpdateBlobs". Fix: replace all 6 citations with WI-S01-004; update §18 hard blocker. Same fix for WI-S04-002 §13/§18 if any (none found, but verify).

### WI-S04-001 — 8+ new `COR_AC_*` error codes are not defined in `error_taxonomy.md §3.2`

**Severity**: P0. **WI**: WI-S04-001 §1, §6.1, §23.

`error_taxonomy.md §3.2` defines exactly 3 AC error codes: `COR_AC_ACTION_NOT_FOUND`, `COR_AC_TTL_EXPIRED`, `COR_AC_MERKLE_INVALID`.

WI-001 invents and uses (without amending taxonomy):
1. `COR_AC_OUTPUTS_MISSING` (422)
2. `COR_AC_SIG_INVALID` (422)
3. `COR_AC_BACKEND_UNAVAILABLE` (503)
4. `COR_AC_RESULT_HASH_MISMATCH` (409)
5. `COR_AC_DIGEST_MISMATCH` (422)
6. `COR_AC_BATCH_TOO_LARGE` (400)
7. `COR_AC_PAYLOAD_TOO_LARGE` (413)
8. `COR_AC_INTERNAL` (500 — implied via `AcError::Internal`)
9. `COR_AC_DEPRECATED` (503 — appears in WI-002 §8 rollback Gherkin)

These codes appear in the AcError enum (§1), §6.1.X HTTP mappings, §23 API contract, and §28 STRIDE risks, but the canonical taxonomy is not amended. CI gate (`scripts/validate_references.py`) and the SDK exception generator will fail at code-time. Fix: amend `error_taxonomy.md §3.2` in this Lote to add all 9 codes with HTTP/retryable/SDK exception/customer_message/next_action fields, and reference the amendment in WI-001 §13 artifacts as a deliverable. Document each in ADR-0035 if needed (small registry-extension ADR, not a separate one).

### WI-S04-001 — `ac_neg` negative cache key and TTL drift from `data_model.md §6.1`

**Severity**: P0. **WI**: WI-S04-001 §1 sequence, §6.1.5, §9.3.

`data_model.md §6.1` (KV layout): `ac_neg:<digest>` value `flag` TTL **300s**.
WI-001 §1 / §6.1.5: key `ac_neg:<tenant_prefix>:<action_digest>`, TTL **60s**.

Both differences may be defensible (the tenant_prefix scoping is correct defense-in-depth; the 60s TTL is justified in §9.3 as build-retry-burst-aware), but the canonical doc is not amended. Either:
- (a) Amend `data_model.md §6.1` to match WI-001 — preferred, since the new key is strictly safer (per-tenant scoping prevents cross-tenant negative-cache leak/poisoning) and the new TTL has a written rationale.
- (b) Justify why the WI deviates and how the deviation is reconciled.

Currently WI-001 ignores the canonical entry. Fix: amend data_model.md §6.1 in this Lote; cross-reference from WI-001 §1.

### WI-S04-001 — Mann-Whitney rationale targets the wrong arm (information disclosure scope)

**Severity**: P0 cripto-methodology. **WI**: WI-S04-001 §6.1.10, §8 Gherkin "Mann-Whitney timing", §9.9.

The chosen arms are: "action_not_found (no entry anywhere)" vs "action_found_but_wrong_tenant (entry exists for Tenant B; Tenant A asks)". Both return 404; the goal is to prove the attacker cannot distinguish "doesn't exist" vs "exists for someone else."

This is a **valid information-disclosure scope** (tenant existence enumeration), but the methodology has two issues that the Crypto SME will flag:

1. **The two paths exit D1 at different points.** "wrong_tenant_404" performs a successful row-fetch in D1 with `tenant_id` filter mismatching, returning empty result set. "not_found" performs the same query with `(tenant_id, action_digest)` not present at all. SQLite query execution time depends on **B-tree traversal cost**, which differs between (a) "key not present in PK index" and (b) "key under different tenant_id but same action_digest also not present in current tenant subtree". Realistically these two are **already statistically indistinguishable on a balanced B-tree**, but the WI doesn't show the analysis. The Crypto SME will ask for the timing model derivation, not just the test.
2. **Negative cache short-circuit asymmetry.** §6.1.3 step [1] populates `ac_neg:<tenant_prefix>:<digest>` on miss; the second time you query the same not-found digest, you hit KV and skip D1 entirely. The wrong_tenant_404 path may also populate negative cache (because step [1] runs before step [2]). The 10k samples need to control for negative-cache state — either pre-warm both arms with negative cache populated, OR clear KV between samples. The WI doesn't specify.

Fix:
- Add a paragraph to §9.9 deriving the timing model: B-tree miss latency vs negative-cache hit latency; explain why they should coincide post-warmup.
- Amend §6.1.10 to specify negative-cache state control: "10k samples per arm, all post-negative-cache populated for the digest"; OR drop negative cache from this Mann-Whitney scenario and pick "D1-cold" path explicitly.
- Cite the Mann-Whitney sample size rationale (10k @ d=0.2 = power ≥ 0.80) consistently with WI-S03-003 baseline; current §10.s04.001.2 is correct, just needs the timing-model paragraph.

This is a **methodology rigor** issue, not a math error. Other WIs in the Sprint pile re-use the same statement; if Lote 10.4bis fixes WI-001 here, the same fix template applies to WI-003's "valid vs tampered" arm (which is cleaner — both paths run the full Merkle re-hash, so timing model is symmetric by construction).

### WI-S04-001 — REAPI `BatchUpdateActionResult` semantics drift

**Severity**: P0 conformance. **WI**: WI-S04-001 §1, §6.1, §9.6, §28 R-014.

§9.6 says "100 batch limit (REAPI BatchUpdateActionResult); D1 batch op limit ~100; Bazel client default batch = 50-100."

This conflates two REAPI concepts:
1. **REAPI v2 has no `BatchUpdateActionResult` RPC**. ActionCache service exposes `GetActionResult` and `UpdateActionResult` only (singular). The batch RPC at REAPI v2 is on **ContentAddressableStorage**: `BatchUpdateBlobs` and `BatchReadBlobs`. There is no batch on ActionCache.
2. Batching for AC is therefore client-side only (Bazel issues parallel `UpdateActionResult` RPCs); server-side "batch cap 100" doesn't apply because there is no server-side batch RPC to cap.

The WI cites the conformance suite (`bazelbuild/remote-apis`) — which test exactly this. If the WI actually adds a non-existent server-side batch endpoint, conformance suite fails. Fix:
- Strike "BatchUpdateActionResult" from §1 attacker scenario, §6.1, §9.6, §28 R-014.
- If batch-write semantics are wanted (e.g. for SDK convenience), reroute to `BatchUpdateBlobs` (which lives in CAS, not AC) and document explicitly that AC has no batch RPC in REAPI v2.
- The "100 batch cap" is still relevant for `BatchUpdateBlobs` in S-01/S-02; cite the existing WI rather than re-inventing it here.

This is a REAPI-spec correctness defect; the conformance suite gate (10.s04.1) catches it eventually but the WI should not ship with the wrong API name.

### WI-S04-002 — D1 sizing math has a 4-orders-of-magnitude error

**Severity**: P0 cost-accuracy. **WI**: WI-S04-002 §6.1.13, §22.

§6.1.13: "10M unique actions/tenant × 100 tenants = 1B rows = ~250 GB"
§22: "10M rows × 250 bytes = 2.5 GB"

These numbers are inconsistent within the same WI. §6.1.13 says 1B rows; §22 says 10M rows. Pick a single sizing model:

- 1B rows × 250 B = 250 GB ✓ (matches §6.1.13 stated total)
- 10M rows × 250 B = 2.5 GB ✓ (matches §22 stated total)
- 10M actions × 100 tenants ≠ 10M rows; the multiplier 100 is dropped in §22.

Cost gate at §22 ("D1 storage ≤ $0.01/GB/mo cap") is also **inconsistent with the assumed CF D1 paid pricing**. Cloudflare D1 storage pricing as of 2026 (and during the assumed 12-month TCO window) is approximately $0.75/GB-month for storage above the included 5 GB tier, not $0.01/GB/mo. At 250 GB, that's ~$2,250/yr just for D1 storage — **not $24/yr**. The 250 GB also exceeds D1's **per-database 10 GB hard limit** (paid tier as of CF docs cutoff); sharding becomes mandatory, not optional, well before 1B rows.

Fix:
- Pick one sizing target (say, "100 tenants × 100k actions = 10M rows ≈ 2.5 GB" for S-04 GA, with a clear sharding trigger at 10 GB / 80M rows).
- Re-derive cost: 2.5 GB stays under the 5 GB free allowance OR 5–10 GB at $0.75/GB-month = ~$45-90/yr at the upper bound.
- Document the **D1 per-database hard limit** explicitly (10 GB current; CF policy changes possible) and make sharding triggered at 80% of limit, not "ADR-0036 sharding criteria" hand-wave.
- Re-do §22 "Comparison vs alternatives" with corrected numbers; the current Postgres ($60/yr) and DynamoDB ($200/yr) figures need verification against the same workload.

This is a real customer-cost-disclosure issue if the cost gate fails in production due to underestimated storage.

### WI-S04-002 — `wrangler r2 bucket lifecycle set` flag is wrong

**Severity**: P0 ops correctness. **WI**: WI-S04-002 §1.

The provisioning script in §1 uses:
```bash
wrangler r2 bucket lifecycle set "$bucket" --rule '...'
wrangler r2 bucket cors set "$bucket" --rules '[]'
```

As of `wrangler` 3.x and 4.x (the Lote's likely toolchain), the lifecycle subcommand is `wrangler r2 bucket lifecycle add` (or `wrangler r2 bucket lifecycle list/delete`). The `set` form does not exist in the published CLI; you must `add` rules one at a time. Similarly `wrangler r2 bucket cors set` is **not a supported subcommand** — CORS for R2 buckets is configured via the Cloudflare dashboard or REST API as of cutoff; CLI support is limited.

This is similar to the `wrangler r2 bucket lifecycle set` form claim — verify against current `wrangler --version` matrix and update. The script as written **will fail** at first invocation. Fix:
- Replace `lifecycle set` with `lifecycle add` plus correct args.
- Replace `cors set --rules '[]'` with the actual current CLI form (`wrangler r2 bucket cors put` if available, else REST API + curl).
- Add an integration test in CI that runs the provisioning script end-to-end against a CF preview environment.

### WI-S04-003 — Outputs validation `FOR SHARE` semantics on D1 are unimplementable

**Severity**: P0 design clarity. **WI**: WI-S04-003 §2.6.

§2.6 (Narrative): "validation uses `SELECT ... FOR SHARE` semantics OR repeatable-read snapshot (D1 SQLite has limited semantics); fall-back: reconcile diário catches drift."

`SELECT ... FOR SHARE` is a **Postgres** row-level lock. SQLite has no row-level lock primitives (locks are database-level). D1 specifically doesn't expose any locking primitive; transactions are serializable-by-the-engine but you cannot re-claim a row.

The WI correctly hedges with "fall-back: reconcile diário", but the way it's written sounds like FOR SHARE is the primary; in fact it's the **non-existent option**, and reconcile is the **only option**. Re-word:
- §2.6: "INV-AC-OUTPUTS-VALID is point-in-time best-effort. D1 has no row-level locking; we accept TOCTOU between handler outputs check and S-06 GC tombstone, and rely on reconcile diário (S-06) to detect and fix drift. The race window is bounded by GC mark-phase + S-06 outbox emission window."
- Add `INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY` to invariant registry, or document the eventual-consistency tier explicitly.
- Property test or chaos test: simulate the race; assert reconcile catches within 24h SLA.

This is a P0 because the Crypto SME / Architect will read "FOR SHARE" and either accept it (incorrectly) or reject the WI. The honest formulation strengthens the design.

### WI-S04-003 — Determinism claim conflicts with `prost` / Protobuf canonicalization reality

**Severity**: P0 cripto-determinism. **WI**: WI-S04-003 §1, §2.4, §6.1.3, §9.5.

The WI repeatedly asserts "protobuf canonical serialization (deterministic)" as the basis for byte-stable Merkle root.

**Protobuf serialization is not canonical by default.** Specifically:
1. **`prost`** does **not** guarantee deterministic encoding for messages with map fields or repeated fields with reordered inputs. The `prost` API has no "canonical" or "deterministic" flag like the Java protobuf API's `setDeterministic(true)`.
2. **REAPI v2 ActionResult contains repeated fields** (`output_files`, `output_directories`, `output_symlinks`, `output_file_symlinks`, `output_directory_symlinks`) and **map fields** (`execution_metadata`'s nested types contain auxiliary maps). The repeated fields preserve the order the **client sent them in**; if Bazel client serializes in different order between two builds, raw protobuf bytes differ even if the logical content is identical.
3. The WI's mitigation "lex sort outputs" handles the **Merkle tree input order** but not the **protobuf bytes underlying `result_hash`** (if `result_hash = SHA-256(canonical_proto_bytes)` per WI-001 §1 / WI-002 schema, then non-canonical proto yields different result_hash for logically identical input).

Fix:
- Specify the canonicalization scheme explicitly. Options:
  - (a) Custom canonical encoder (e.g. lex-sort all repeated fields by deterministic key, sort map entries, omit unknown fields). This is **non-trivial** in `prost`; you need to either (i) re-encode after lex-sort or (ii) use a wrapper canonicalization library like `protoscope` or hand-rolled.
  - (b) Use `serde_jcs` (RFC 8785 JCS) on a JSON projection of ActionResult; bypass protobuf for the canonical bytes; `result_hash` = SHA-256(JCS bytes). REAPI does not require result_hash to be over protobuf bytes; you can pick the canonical form.
  - (c) Define `result_hash = BLAKE3(merkle_root)` or `result_hash = merkle_root` directly, eliminating the proto-bytes dependency. This is the cleanest option but changes the semantic.
- The current Annex A test vectors gate (10.s04.003.5) is the right CI gate but the test will fail intermittently if the canonicalization isn't fixed first.
- Add to `ADR-0037` the canonicalization decision with rationale.
- WI-S03-007 (audit chain) had the same `serde_json` non-determinism finding; the same `serde_jcs` recommendation applies.

This is the **second-highest cripto-rigor defect** in the package (after the `with_tenant_ctx!` Postgres/D1 confusion). Without it, the determinism property test (10.s04.003.6: 1000 ActionResults × 100 builds = byte-identical) **will fail** on inputs with unordered map/repeated field iteration.

### WI-S04-003 — `MerkleError::CycleDetected` is referenced but not in the error enum

**Severity**: P0 (concrete inconsistency; trivial fix). **WI**: WI-S04-003 §1, §2.5.

§1 error enum lists: DepthExceeded, FanoutExceeded, NodeCountExceeded, PayloadExceeded, RootMismatch, InvalidDigestFormat, VersionUnsupported, DecodeError. **No `CycleDetected` variant**.

§2.5: "cycle detection mandatory (else infinite recursion). Mitigação: bounded parser tracks visited node digests; cycle → `MerkleError::CycleDetected` (added to error enum); test 100 crafted cyclic protos rejected."
§8 Gherkin: "Then error MerkleError::CycleDetected"

Fix: add the variant to the enum in §1. Trivial but the error enum is the public API surface; the WI as drafted doesn't compile.

### WI-S04-003 — `verify_full` "structure check still catches even if sig forged" claim is overstated

**Severity**: P0 cripto-clarity. **WI**: WI-S04-003 §2.2, §8 Gherkin "verify_full detects structure tamper even if sig valid", §9.4.

The Gherkin scenario reads: "Given envelope E with byte flip in result; sig invalidated AND attacker also forges sig (chave compromise scenario) When verify_full(E, sig_verifier_with_wrong_key_accepted) Then verify_structure path catches MerkleError::RootMismatch."

This is **only true under a specific threat model**: the attacker compromises the HKDF key, **but does not also recompute the Merkle root** in the envelope (since envelope.merkle_root is signed alongside the rest). If the attacker has the HKDF key (full chave compromise), they can:
1. Modify result.output_files[0].digest to point to a malicious blob.
2. Recompute the Merkle root over the modified result.
3. Set envelope.merkle_root to the recomputed root.
4. Re-sign the envelope with the compromised HKDF key.

Now `verify_structure` finds matching merkle_root (because the attacker also updated it) and `verify_full` returns OK. Defense-in-depth fails in full chave compromise.

The actual defense-in-depth scenario where Merkle catches things sig misses is **partial** chave compromise — e.g. attacker can sign but cannot read the structure (because it's in a different layer), or attacker can flip bytes in storage but doesn't have the HKDF key (in which case sig alone catches; Merkle is redundant).

The honest framing: **dual-side verify catches storage-tier tampering even when the writer-side has been compromised mid-pipeline**. Merkle binding ensures the envelope's content-hash is consistent; sig binds tenant + key version. Both fail together in full chave compromise.

Fix:
- Re-word §2.2 (point 2) and §9.4 to specify **"partial compromise"** scenarios (storage-tier insider with R2 write but no HKDF key; or wire-tampering between handler and R2).
- Strike or amend the §8 Gherkin "verify_full detects structure tamper even if sig valid" — under full chave compromise this does NOT catch (because attacker also recomputes merkle_root). Under partial compromise (sig verifier returns OK due to delegation bug, e.g.) it does catch.
- Add explicit **threat model in `merkle_protocol.md`** spec doc enumerating which adversary capabilities each layer defends against.

The cripto SME will catch this. The hand-wavy "even if sig forged" is the kind of language that breaks down under formal threat-modeling. Tighten it.

### WI-S04-001 — `R2-first then D1 INSERT` rationale needs orphan-cleanup SLA and reconcile lag bound

**Severity**: P0. **WI**: WI-S04-001 §9.5, §15.

§9.5: "R2-first: crash before D1 INSERT → orphan R2 (recoverable; GC reconcile cleans); D1 absence = retry-safe Update."

Two issues:
1. **GC reconcile in S-06 doesn't yet exist**. The orphan-cleanup story relies on a future sprint; this WI's DoD shouldn't claim "recoverable via GC reconcile" without an explicit deferred-INV record. Add to invariant registry as `INV-AC-ORPHAN-R2-CLEANUP-EVENTUAL` with SLA bound (e.g., "orphan R2 envelopes cleaned within 24h via S-06 reconcile cron"; explicit forward-dependency).
2. **Cost of orphan storage**. If the crash window is small (1 in 10⁵ requests), at 1M UPDATEs/day, that's 10/day orphan envelopes × 5 KB = 50 KB/day = 18 MB/yr. Acceptable. But if a deployment incident causes many crashes mid-flight, orphan count can spike. Cost gate (§22) should include orphan-rate alerting (current §22 doesn't list it).

Fix: add `INV-AC-ORPHAN-R2-CLEANUP-EVENTUAL` and a metric `corelink.ac.r2.orphan_rate` (gauge; alert at >1% of UPDATE rate).

### WI-S04-001 / WI-S04-002 — Inconsistency on `expires_at` semantics

**Severity**: P0 INV consistency. **WIs**: WI-S04-001 §6.1.3 step [2], §8 Gherkin "GetActionResult expired"; WI-S04-002 §1 column comment, §8 Gherkin "lifecycle violation".

WI-001 says expires_at NULL = "no expiry; ADR-0019 per-tier override". GetActionResult lookup is `WHERE … AND (expires_at IS NULL OR expires_at > NOW())`.
WI-002 says expires_at NULL = "no expiry; per-tier override S-07/ADR-0019" (consistent at column level), but the CHECK constraint `chk_ac_lifecycle` says `expires_at IS NULL OR expires_at >= created_at`.

Now: ADR-0019 mandates **per-tier defaults** (free=7d, solo=30d, team=90d, business=365d, enterprise=customer-configurable). The S-04 default per ADR-0019 is "supersedido por S-07"; pre-S-07 ship, the WI-002 schema defaults `expires_at NULL` and the WI-001 handler "TTL refresh-on-hit" sets `expires_at = now() + tier_ttl`.

The combined story:
1. UPDATE handler: writes `expires_at = now() + tier_ttl` per tier (where does tier_ttl come from at S-04 ship-time before S-07 SEALED?).
2. GET handler: refresh-on-hit sets `expires_at = now() + tier_ttl`; otherwise returns 410 if expired.

But **at S-04 GA, S-07 isn't shipped yet**. The "tier_ttl" lookup is non-existent. The de-facto behavior is "no expiry" (NULL) until S-07 lands. That's defensible but the WIs talk as if tier_ttl is already available. Fix:
- WI-001 §6.1.3 step [6]: explicitly "tier_ttl from `tenant_quota.ac_ttl_default` if present, else NULL (no expiry); per ADR-0019 S-07 supersedes."
- WI-002 should note `expires_at` defaults to NULL pre-S-07 and document the migration story when S-07 ships per ADR-0019 §"Migration plan".
- Cross-check that WI-S04-005 (TTL worker) is the authoritative source for the tier_ttl lookup; current WI-001 §1 sequence runs refresh inline, but WI-005 is the cron worker. Don't duplicate logic; reference WI-005's tier-ttl lookup as the contract.

### WI-S04-001 — Compile-time TenantCtx-only enforcement is not enforceable via clippy lint as described

**Severity**: P0 honesty. **WI**: WI-S04-001 §10.s04.001.5.

"static-checked via clippy custom lint that warns on `tenant_id: TenantId` parameter in handler module".

Clippy custom lints require:
- A clippy plugin (compile-time; needs nightly or `clippy_utils` build-on-release).
- A `dylint` runtime drive (nightly-toolchain).
- Or a `cargo-deny`-style AST walker.

This is a **2-3 sprint engineering investment**, not a §10 acceptance line. WI-S03-007 (audit chain redaction lint) had the same problem; the recommended alternative was a wrapper newtype (`RedactedPrincipal`) that derives Debug to print `[REDACTED]` and force all audit emit paths to take the newtype.

Same alternative applies here: define `pub struct TenantCtx { tenant_id: TenantId, … }` and **make `TenantId` not constructable from a public API** (`#[non_constructible]` pattern via private inner field + private constructor only callable from auth middleware). Handler signatures can only receive `&TenantCtx`, not `TenantId` directly, because `TenantId` cannot be created outside the auth middleware.

Fix:
- Strike "clippy custom lint" claim; replace with type-system enforcement strategy.
- Add a §13 artifact: `corelink-auth/src/tenant_id.rs` newtype with private constructor.
- Update §10.s04.001.5 to "`TenantId` is constructable only via S-03 auth middleware private constructor; handler signature `&TenantCtx`; integration test compiles → fails to construct `TenantId` directly in handler-test code."

Type-system enforcement is **strictly stronger** than a clippy lint and easier to ship. Make this the canonical pattern for the workspace.

### Cross-WI — Sign-off table 13 vs 12 + advisory mismatches sprint contract

**Severity**: P0 governance. **WIs**: all three.

Spec contract §6 DoD: "PRR HIGH_RISK + adversarial review: SRE + Security + Engineer + QA + Product + Compliance + Privacy + Architect + AppSec + 2 peers + Crypto SME (HKDF review) (EVT-031)" — that's 12 mandatory roles + 1 advisory Crypto SME = 13 (matches WI-001 / WI-002 / WI-003 §30 row count).

But WI-001 §30 row 13 says "Crypto SME (advisory)" — consistent with contract.
WI-002 §30 row 13 says "DBA (advisory)" — **substitutes Crypto SME with DBA**. The sprint contract mandates Crypto SME. WI-002 silently swaps roles. Either:
- (a) Add DBA as row 14 (WI-002 needs both DBA + Crypto SME for HKDF integration boundary review; the sig_key_id rotation column is cripto-relevant).
- (b) Keep Crypto SME mandatory; relegate DBA to "consulted" (not a sign-off role).

The sprint contract is the canonical governance source; WI-002 can't unilaterally swap. Fix by adding DBA as row 14, marking the schema-design components as DBA-mandatory while keeping HKDF integration as Crypto SME-mandatory.

### Cross-WI — `INV-AC-IDEMPOTENT` etc. promoted to registry §3.15 but registry doesn't have §3.15

**Severity**: P0 invariant registry hygiene. **WIs**: WI-S04-001 §12, WI-S04-002 §12, WI-S04-003 §12.

WI-001 promotes 3 NEW invariants: INV-AC-IDEMPOTENT, INV-AC-NEG-CACHE-INVALIDATED-ON-UPDATE, INV-AC-RESULT-HASH-IMMUTABLE — all citing "registry §3.15 Lote 10.4bis".
WI-003 promotes 5 NEW: INV-AC-MERKLE-VALID, INV-AC-MERKLE-DETERMINISTIC, INV-AC-BOUNDED-PARSER, INV-AC-CYCLE-FREE, INV-AC-DUAL-SIDE-VERIFY — also "registry §3.15".

`invariant_registry.md` currently has §3.3 (security) listing INV-AC-OUTPUTS-VALID and INV-AC-TENANT-SCOPED. **There is no §3.15.** The WIs reference a section that does not yet exist. Either:
- (a) Lote 10.4bis amends `invariant_registry.md` to add §3.15 with the 8 new INVs (3 from WI-001 + 5 from WI-003) plus owner/derivation/test refs.
- (b) The WIs cite the actual section that exists (probably §3.3 expansion or a new §3.4 "Action Cache").

This is a "promised but not delivered" invariant promotion. Fix: amend the registry in this Lote; WIs §12 stay as-is.

### WI-S04-001 — `outputs_check::warn-if-missing` on GET runs even on every cache hit

**Severity**: P0 perf+cost. **WI**: WI-S04-001 §6.1.3 step [5], §9.4.

§6.1.3 step [5]: "warn-log if any output_file digest tombstoned; NÃO bloqueia GET (INV-AC-OUTPUTS-VALID é reconcile-eventual)".

If this runs on every GET, on a 10M-GET/day workload with average 4 output_files per ActionResult, that's 40M D1 SELECT/day on `blob_meta(tenant_id, digest)` just for the warn-only path. Cost analysis (§22) doesn't include this; latency budget (p99 ≤ 150ms warm) is at risk.

Fix:
- Sample-based warn check (e.g. 1% of GETs run the outputs check; metric corelink.ac.outputs.tombstoned_warning_total reflects sampled rate).
- Or: rely entirely on reconcile diário (S-06) and S-04 GET does not check outputs at all; only logs `corelink.ac.outputs.unverified_get_total` for visibility.
- Update §22 with the cost model for whichever option is chosen; check p99 budget.

This is a concrete cost-budget defect; the warn-only path is hot.

---

## P1 Findings (should-fix this sprint)

### WI-S04-001 — sequence flow in §1 omits negative-cache short-circuit for UpdateActionResult

§1 UpdateActionResult flow steps [1]-[10] include negative_cache::invalidate at step [9]. But it doesn't explicitly say what happens if negative_cache is populated **before** UPDATE: step [9] invalidates. OK. But what if UPDATE happens during negative-cache TTL — does the next GET have to wait? Trace through: [9] invalidate is post-D1 INSERT, so subsequent GETs see the new D1 entry; race window (step [9] vs concurrent GET) is bounded by KV.delete latency (~10ms). Document this race window in §6.1.5 explicitly.

### WI-S04-001 — Conformance suite invocation via subprocess is fragile

§6.1.11 / ST-013: "invoke bazelbuild/remote-apis test suite via subprocess." This implies a Go binary or shell harness. Specify:
- Which test binary (path + version pin).
- How conformance is mapped to PR vs nightly CI (Lote 10.3 had similar issues with FIDO Alliance suite that was paid; verify bazelbuild/remote-apis suite is open-source — it is, at github.com/bazelbuild/remote-apis).
- How a partial failure (90% passing) is handled — is it red on the first failure, or red on regression-from-baseline? The WI says "100% green nightly" but the spec contract §15 waiver allows "95% with explicit waiver list per check." Reconcile.

### WI-S04-001 — REST surface specifies JSON body, but REAPI defines ActionResult only as protobuf

REAPI v2 spec (`build/bazel/remote/execution/v2/remote_execution.proto`) does not define a JSON wire format for ActionResult. The REST surface in §6.1.13 says:
- `Accept: application/json` mandatory; `application/x-protobuf` for binary.

But Bazel/Buck2 do not natively speak the JSON form; the REST companion is **CoreLink-only convention**. Document this clearly:
- gRPC = REAPI canonical; what Bazel/Buck2 speak.
- REST/JSON = CoreLink dashboard/CLI convenience; NOT REAPI conformance-testable.
- Conformance suite (§10.s04.1) tests only gRPC.
- Add explicit non-goal in anti-scope.

### WI-S04-001 — Mass Assignment / OWASP API Top 10 §10.s04.001.11 needs concrete PR mapping

§10.s04.001.11 says "OWASP API Security Top 10 checklist pass (BOLA + Mass Assignment + Improper Inventory)". Concrete: list which top-10 items map to which Gherkin/property test:
- API1 BOLA → tenant isolation Gherkin scenarios.
- API3 Excessive Data Exposure → ActionResult metadata redaction policy (LINDDUN).
- API6 Mass Assignment → body tenant_id ignored.
- etc.

Without the mapping the audit deliverable in §13 (`2026-XX-XX-owasp-api-top10-ac.md`) is hand-wavy.

### WI-S04-001 — No explicit "instance_name" handling

REAPI v2 has an `instance_name` field on every request (used by BuildBuddy for multi-tenancy at the namespace level). §3 Persona 2 mentions it: "Buck2 sends `instance_name` field different from Bazel default; handler must accept any instance (S-04 não enforce instance namespace; S-13 admin plane forward)."

But the gRPC handler signature in §1 omits instance_name. The Gherkin tests don't reference it. If a customer issues `--remote_instance_name=foo`, what does the handler do? Reject? Accept and ignore? The WI is silent. Fix: add explicit "instance_name accepted as opaque label in S-04; passed to audit emit metadata; not used for namespace; S-13 enforces" + Gherkin test.

### WI-S04-002 — `idx_ac_meta_region` is rare; explain why it exists

§1 / §6.1.1: "Index: region (rare; cross-region migration support; not hot path)" — but if it's rare, it costs storage. Cost-benefit: index size at 10M rows × 4 bytes/region × 1.5 overhead = ~60 MB. Acceptable, but document the read pattern that justifies it (analytics queries? S-14 migration? CI cron audit? Currently §14.s04.002.10 says only "1 specific use"). If the only use is audit/analytics, mark as removable post-S-14 ship.

### WI-S04-002 — sample data seeder leaks no PII?

§6.1.7: "Sample data seeder for staging (DEV ONLY); ~100 ac_meta entries for testing." Verify:
- The seeder uses synthetic tenant_ids (UUID v7 random, not tied to real PAT issuance).
- `created_by_pat_id` NULL or synthetic.
- `command_line` / env_vars are dummy fixtures, not pulled from real Bazel CI.

Add an §10.s04.002.X line: seeder data passes the LINDDUN Identifiability test (no real customer data).

### WI-S04-002 — `chk_ac_sig_alg IN ('hkdf-sha256', 'hkdf-blake3')` hard-codes future format

`hkdf-blake3` doesn't exist as a standard sig_alg name (HKDF can use any HMAC primitive; if you want BLAKE3-keyed HMAC you'd write `hkdf-hmac-blake3` or pick a clear naming). The forward-compat hint is good intent, bad spec. Drop `hkdf-blake3` from the CHECK; require ADR + migration to add new alg. Consistency: WI-001 sig flow uses HKDF-SHA256 throughout.

### WI-S04-002 — Migration `BEGIN; … COMMIT;` is inert in D1 wrangler migrations

`wrangler d1 migrations apply` runs each migration in its own implicit transaction. The `BEGIN;` / `COMMIT;` in the SQL file are redundant or potentially conflicting (depending on wrangler version). Verify against current wrangler docs and remove if unnecessary.

### WI-S04-003 — `serde_json` with sorted keys vs `serde_jcs` (RFC 8785) — pick one

§6.1.7: "`serde_json` with sorted keys; `serde_jcs` (RFC 8785 JCS) NOT used here (envelope is internal binding, not inter-service event); `serde_json` with sorted is sufficient."

This is the same `serde_json` non-determinism finding as WI-S03-007. `serde_json` does not natively sort keys; you'd need to implement a `BTreeMap` wrapper or custom serializer. The "with sorted" formulation glosses over the implementation detail. Recommendation:
- Use `serde_jcs` (battle-tested RFC 8785 implementation) for envelope JSON.
- The "envelope is internal binding" rationale doesn't justify weaker canonicalization — the envelope IS the cripto binding (signed input), so it must be byte-stable.
- Cite WI-S03-007 cross-ref for the same recommendation; consistency across cripto code paths.

This dovetails with the prost determinism P0; both routes (proto bytes or JCS bytes) need to be locked down.

### WI-S04-003 — `MAX_OUTPUT_FILES = 4096` is below typical Bazel large-build numbers

§6.1.5: `MAX_OUTPUT_FILES = 4096`. Bazel large monorepo builds (e.g. Google-scale) have actions with output_files counts in the 10k+ range. 4096 is a reasonable S-04 GA cap but needs business validation:
- What's the 99th percentile output_files count from real Bazel customer data? Sprint contract §5 §16 doesn't show this.
- If a legitimate customer hits 4096 cap, error is `MerkleError::FanoutExceeded` (422); customer build fails.
- Forward path: configurable cap per-tier (enterprise can lift to 16k); ADR-0037 documents.

Add §6.1 line: "MAX_OUTPUT_FILES = 4096; per-tier override via tenant_quota.ac_max_outputs forward S-13."

### WI-S04-003 — Cargo-fuzz harness 1h is single-target; consider multi-target

§6.1.13 / ST-013: "fuzz/fuzz_targets/decode_envelope.rs". One target. Recommended: also fuzz the Merkle builder (`fuzz_build_merkle.rs`) — different surface. And fuzz the outputs validator (`fuzz_validate_outputs.rs`). Total 3 targets × 1h CI nightly = 3h budget; still <$0.20/nightly.

### WI-S04-003 — Test vectors Annex A + B should include cross-implementation reference vectors

§14: 50 valid + 50 invalid envelopes is good. **Reference vectors that other implementations can verify against** are the gold standard:
- BLAKE3 official test vectors for the underlying hash.
- RFC 6962 Merkle test vectors for the tree structure (modulo the BLAKE3 substitution).
- Bazel's own `bazel-remote-apis` test cases for ActionResult round-trip.

Cite these explicitly in `merkle_protocol.md` so external SLSA Level 3 reviewers can trace.

### Cross-WI — WI-S04-003 outputs validator runs in WI-S04-001 handler context but the BlobMetaReader trait has no tenant_prefix scoping

WI-003 §1 trait:
```rust
async fn batch_check_alive(&self, tenant_id: &TenantId, digests: &[Digest]) -> Result<Vec<bool>, _>;
```

The implementation in WI-001 handler will issue a D1 `SELECT … WHERE tenant_id = ? AND digest IN (...) AND deleted_at IS NULL`. That's the right enforcement at the SQL layer. But the trait doc/spec should say: "implementations MUST scope by tenant_id; the batch is single-tenant only; mixing tenants in one call is a logic error."

Add a unit test: passing an empty digests list returns empty Vec; passing a digest that exists for a different tenant returns false (alive=false from the scoped reader's view).

### Cross-WI — Operational realism: 13 sign-offs with 9 TBD (same finding as S-03)

All three WI §30 tables show 8-9 _TBD_ rows. Same staffing risk as flagged in S-03 part2 review (WI-S03-008 finding #6). For S-04, additionally:
- Crypto SME emphatic-mandatory in WI-S04-003 — must be confirmed availability before sprint start.
- DBA mandatory in WI-S04-002 — must be confirmed.
- Architect emphatic across all 3 — single person, scheduling bottleneck.

Add to sprint-level risk register: "S-04 sign-off staffing — Crypto SME + DBA + Architect availability not yet confirmed; mitigation TBD."

---

## P2 Findings (nice-to-have)

- **WI-S04-001 §22**: "BuildBuddy Cloud: $99-299/seat/mo × 50 seats × 12 = $60-180k/yr" — cite the BuildBuddy public pricing source (URL). Customer-facing cost claims need provenance.
- **WI-S04-001 §27**: "Tech talk (1.5h)" + "Workshop (2h)" + "Onboarding test (5 questions)" — line up with Knowledge Transfer schedule; specify deliverable artifacts (slide decks at `docs/internal/tech-talks/`).
- **WI-S04-002 §1**: `result_size_bytes INTEGER NOT NULL` — clarify whether this is the proto size, envelope size, or output_files total size. Three different metrics; likely you want envelope size (payload cost) plus a separate `outputs_total_bytes` for cost analytics.
- **WI-S04-002 §1**: `created_by_pat_id TEXT NULL` — if NULL is "legacy entries", document the migration cutover date. Better: NOT NULL with a sentinel value (`'__legacy__'`) for clarity; NULL fields that mean "legacy" tend to drift in semantics over years.
- **WI-S04-002 §10**: Property `prop_ac_meta_lifecycle_invariant` — cover the exact CHECK constraint; verify proptest input space generates edge cases (created_at = INT64_MAX, last_hit_at = 0, etc.).
- **WI-S04-003 §1**: `pub struct AcEnvelope { ... }` shows `pub created_at_ms: u64` — Unix ms in u64 will overflow in year ~292 million. Use i64 (allows negative for pre-1970 sentinel) or u32 epoch hours. Probably fine; just call out.
- **WI-S04-003 §6.1.5**: `prost has recursion limit feature (default 100); set to 32 for output_directories.` — this is correct. Add CI test: a deeply-nested proto with 33 levels fails to decode with `prost::DecodeError`. Currently §15 chaos #4 covers depth 10000; add depth=33 boundary case.
- **WI-S04-003 §22**: "BLAKE3 (this impl): $9.1k/yr at 10M/dia" vs "SHA-256 (Bazel default): ~$30k/yr". The 4× perf differential is plausible; cite the BLAKE3 paper's measured throughput on AVX-512 vs SHA-256 NI.
- **All WIs**: §31 Change Log has 1 row each. Forward audit trail needed; add field for "Lote where change applied" so SEAL reviewer can trace per-Lote diff history.

---

## Cross-WI Consistency Check

### Pattern alignment (where WIs do agree)
- **TenantCtx flow**: WI-001 enforces TenantCtx-only; WI-003 outputs validator takes `&TenantId` from caller. Aligned at API surface.
- **R2-first then D1**: WI-001 §9.5 articulates rule; WI-003 builder is consistent (build envelope first, sig later, R2 put before D1 INSERT). Aligned.
- **Audit emit via outbox**: WI-001 §9.8 + §6.1.7; consistent with WI-S03-007 outbox pattern.
- **Error code naming convention**: `COR_AC_*` prefix uniform.
- **Mann-Whitney 3-prong methodology**: identical methodology in WI-001 (timing 404 paths) and WI-003 (timing valid/tampered). Power 1−β≥0.80, Šidák 3-trial, bootstrap 95% CI, Cohen's d=0.2.

### Pattern drift (failures of consistency)
1. **`with_tenant_ctx!` Postgres-on-D1** (P0 above) — WI-001 + WI-002 both make the same false claim.
2. **Negative cache key/TTL** (P0 above) — WI-001 silently changes from canonical data_model.md.
3. **Error taxonomy** (P0 above) — WI-001 uses 8+ undefined codes; WI-002 uses `COR_AC_DEPRECATED` undefined.
4. **Audit_outbox citation** (P0 above) — WI-001 cites WI-S01-005, actual is WI-S01-004.
5. **INV registry §3.15** (P0 above) — WI-001 + WI-003 both cite a section that doesn't exist.
6. **R2 path materialization** (P0 above) — WI-001 builds path with `tenant_prefix`; WI-002 schema doesn't store it.
7. **Sign-off table 13th row** (P0 above) — WI-002 silently swaps Crypto SME for DBA.
8. **TTL semantics pre-S-07** (P0 above) — WI-001 / WI-002 both write as if S-07 tier_ttl is available; it isn't.
9. **Batch cap semantics** (P0 above) — WI-001 invents `BatchUpdateActionResult` which doesn't exist in REAPI.
10. **Determinism guarantee source** (P0 above) — WI-003 says "protobuf canonical"; protobuf is not canonical by default.

The cross-WI consistency axis is the **single weakest dimension** of this Lote. WI-001 and WI-002 together accumulate ~6 cross-doc drift defects; WI-003 has 1 (the §3.15 promotion).

---

## Comparison to Canonical SOTA Bar (WI-S03-003 = 8.5)

| Axis | WI-S03-003 (baseline) | WI-S04-003 (this Lote leader) | Delta |
|---|---|---|---|
| Cripto rigor | 5-Layer Defense narrative + Mann-Whitney 3-prong + dummy Argon2 path | BLAKE3 + RFC 6962 domain sep + bounded parser + dual-side defense-in-depth + cycle detection | +0.3 (more cripto-primitive depth) |
| Property tests | 4 props × 100k iter | 6 props × 10k PR + 100k nightly + cargo-fuzz 1h | +0.0 (parity) |
| Mann-Whitney | 3-prong on auth-success/failure | 3-prong on verify timing (1ms target, tighter than WI-001's 5ms) | +0.0 |
| Test vectors / reproducibility | Implicit | Explicit Annex A (50 valid) + B (50 invalid) | +0.3 |
| Public API stability | semver | `#[non_exhaustive]` + semver discipline + crate spec doc | +0.1 |
| Threat model documentation | LINDDUN delta | LINDDUN + Annex A/B + merkle_protocol.md spec | +0.2 |
| Customer-facing readiness | OK | SLSA Level 3 alignment + external blog post planned | +0.2 |

**WI-S04-003 raw score 8.6** is plausibly best-in-class IF the determinism / canonicalization P0 is fixed. With the fix, it credibly reaches 9.0. The Crypto SME emphatic-mandatory + Annex A/B test vectors + bounded parser at decode time + cycle detection are SOTA features WI-S03-003 didn't have.

WI-S04-001 (7.7) and WI-S04-002 (7.6) **fall below** the WI-S03-003 baseline due to the cross-doc drift defects above.

---

## Verdict per WI

| WI | Verdict | Conditions |
|---|---|---|
| WI-S04-001 | **pass-with-fixes (P0)** | Must fix: (a) audit_outbox citation, (b) `with_tenant_ctx!` claim, (c) error taxonomy amendment, (d) negative-cache key/TTL data_model amendment, (e) BatchUpdateActionResult strike, (f) Mann-Whitney negative-cache control, (g) clippy-lint → newtype enforcement, (h) outputs_check on GET cost model, (i) tier_ttl pre-S-07 story, (j) instance_name explicit handling. ≥10 P0s; non-trivial Lote 10.4bis pass. |
| WI-S04-002 | **pass-with-fixes (P0)** | Must fix: (a) ALTER TABLE SQL bug, (b) tenant_prefix column / path-key rotation story, (c) `with_tenant_ctx!` strike, (d) sizing math reconciliation, (e) D1 storage cost re-derivation, (f) wrangler CLI flag fixes, (g) sign-off table 13th row Crypto SME, (h) BEGIN/COMMIT redundancy. ≥8 P0s. |
| WI-S04-003 | **pass-with-fixes-light** | Must fix: (a) protobuf canonicalization or JCS substitute, (b) `MerkleError::CycleDetected` enum variant, (c) `verify_full` chave-compromise framing, (d) `FOR SHARE` D1 reword, (e) registry §3.15 amendment. ≥5 P0s but more localized. |

**Aggregate verdict**: **GO with Lote 10.4bis P0 patch pass before SEAL**. The package is high-quality but fails internal-consistency at the WI-001/WI-002 boundary; that's fixable in a focused 1-2 day Lote 10.4bis pass. WI-003 is a short-list candidate to lift the Sprint mean above WI-S03-003's 8.5 baseline once canonicalization is locked.

---

## Aggregate Recommendations for Lote 10.4bis P0 Fixes

In priority order (do these first):

1. **Schema integrity**: rewrite `migrations/003_ac_meta.sql` to put all 6 CHECK constraints inline at CREATE TABLE; verify with `sqlite3 :memory: < 003.sql` dry-run; add CI gate. (WI-002 §1)

2. **Tenant isolation story for D1**: strike all `with_tenant_ctx!` / `SET LOCAL` references from WI-001 + WI-002; document the actual D1 enforcement (sqlx prepared statement + WHERE-clause discipline + Layer-4 HMAC); amend `auth_model.md §8.1` to call out the D1 vs Postgres asymmetry. (WI-001 §1.1, §6.1.5; WI-002 §1, §2.6)

3. **R2 path materialization / TDK rotation**: pick (a) persist `tenant_prefix TEXT NOT NULL`, (b) add `path_key_id`, or (c) document explicit re-key migration. Update WI-002 schema and WI-001 path build accordingly. (WI-002 §1)

4. **Error taxonomy amendment**: amend `error_taxonomy.md §3.2` to add 9 new `COR_AC_*` codes with full HTTP/retryable/SDK/customer_message/next_action fields. Reference from WI-001 §13. (WI-001)

5. **Data model alignment**: amend `data_model.md §6.1` for the new `ac_neg:<tenant_prefix>:<digest>` key + 60s TTL; cite WI-001 rationale. (WI-001 §1)

6. **Citation fix**: replace WI-S01-005 with WI-S01-004 for audit_outbox in 6 places in WI-001 (§1, §2.8, §6.1.2.4, §9.8, §18). (WI-001)

7. **Invariant registry §3.15**: add §3.15 "Action Cache" to `invariant_registry.md` with the 8 promoted INVs (3 from WI-001 + 5 from WI-003) plus owner, derivation, and test refs. (WI-001 §12, WI-003 §12)

8. **Determinism canonicalization**: pick (a) custom canonical proto encoder, (b) `serde_jcs` for envelope JSON, or (c) result_hash = merkle_root direct. Document in ADR-0037; add to Annex A test vectors. (WI-003 §1, §2.4, §9.5)

9. **REAPI BatchUpdateActionResult strike**: remove all references; relocate batch discussion to BatchUpdateBlobs in CAS WIs. (WI-001 §1, §6.1, §9.6, §28)

10. **Mann-Whitney negative-cache state control**: amend WI-001 §6.1.10 to specify the cache-state preconditions; add timing-model paragraph to §9.9. (WI-001)

11. **MerkleError::CycleDetected** variant: add to error enum in WI-003 §1.

12. **`verify_full` chave-compromise framing**: re-word WI-003 §2.2 (point 2) and §9.4 to specify partial vs full compromise scenarios; update §8 Gherkin accordingly.

13. **Sign-off table 13th row**: WI-002 add DBA as row 14 (or re-label row 13 as Crypto SME + DBA); WI-002 must not silently swap Crypto SME out.

14. **D1 sizing math**: pick one consistent number (10M rows = 2.5 GB) and derive cost against actual D1 paid pricing (~$0.75/GB-mo above 5 GB free); update §22. (WI-002)

15. **Wrangler CLI commands**: verify `wrangler r2 bucket lifecycle add` syntax against current published wrangler; replace `set` with `add`; update CORS provisioning to actual CLI form OR REST API curl. (WI-002 §1)

16. **TenantId newtype**: replace clippy-lint approach in WI-001 §10.s04.001.5 with type-system enforcement (private constructor, only auth middleware can mint). Add §13 artifact `corelink-auth/src/tenant_id.rs`.

17. **outputs_check on GET cost**: pick sample-based or rely-on-reconcile-only; update WI-001 §6.1.3 step [5] and §22.

18. **tier_ttl pre-S-07 story**: WI-001 §6.1.3 step [6] add explicit "tier_ttl from tenant_quota.ac_ttl_default if present, else NULL" with cross-reference to ADR-0019 migration plan.

19. **instance_name handling**: WI-001 add explicit acceptance + Gherkin scenario; document opaque-pass-through in S-04, S-13 enforces.

20. **`FOR SHARE` reword**: WI-003 §2.6 strike Postgres FOR SHARE; document point-in-time best-effort + reconcile diário as the only mechanism.

After these 20 P0 fixes, the package is **substantively SOTA**. Without them, WI-001 and WI-002 will fail the Architect/DBA/Crypto SME independent reviews; WI-003 will fail Crypto SME on the canonicalization + chave-compromise framing.

---

## Strengths Worth Preserving (do not lose in Lote 10.4bis)

- WI-001's tenant_id-confusion adversarial scenario in §2 + property test prop_ac_get_tenant_isolation 100k iter — best-in-class adversarial framing.
- WI-001's R2-first then D1 INSERT-or-idempotent rationale (§9.5) — correct ordering with clear failure-mode reasoning.
- WI-001's negative-cache invalidation atomicity invariant + Gherkin scenario.
- WI-001's gRPC + REST single trait impl (§9.1) — DRY enforced via type system.
- WI-002's deploy guard `check_ac_infra.sh` pre-deploy CI gate concept — good ops discipline (just needs the actual CLI flags fixed).
- WI-002's bucket ACL drift CI cron (§1, §6.1.12, §15 chaos #2) — proactive defense against the most catastrophic FM-303-adjacent scenario (public R2 bucket).
- WI-002's NEVER DROP TABLE + dummy migration rollback policy + ADR-0036 — mature schema governance.
- WI-003's bounded parser at decode time (not post-parse validate) (§9.3) — correct DoS defense order.
- WI-003's RFC 6962-style domain separation `\x00`/`\x01` prefixes (§9.2) — well-cited; correct cripto reasoning.
- WI-003's dual-side verify defense-in-depth narrative (§9.4) — fixes WI-S03-003's gap.
- WI-003's Annex A / Annex B test vectors plan + cargo-fuzz 1h harness — SOTA reproducibility / external auditability.
- WI-003's Crypto SME emphatic-mandatory sign-off — recognition that Merkle protocol design is cripto-load-bearing.

---

## Final Numerical Roll-up

- WI-S04-001: 7.7 / 10 (pass-with-fixes; ~10 P0)
- WI-S04-002: 7.6 / 10 (pass-with-fixes; ~8 P0)
- WI-S04-003: 8.6 / 10 (pass-with-fixes-light; ~5 P0)

**Aggregate part1**: **8.05 / 10**.

Calibration vs prior R4 reviews:
- S-01 part1 baseline: 7.6
- S-03 part1: 7.6
- S-03 part2: 7.95 (best-in-class WI-S03-003 = 8.5)
- **S-04 part1: 8.05** (best-in-class WI-S04-003 = 8.6)

Trend is upward, reflecting genuine SOTA elevation across Lotes. WI-S04-003 plausibly raises the bar for the workspace if the canonicalization P0 is resolved cleanly. WI-S04-001 + WI-S04-002 need the Lote 10.4bis pass to clear the SOTA bar; without it, they regress to S-01 baseline.

User's directive "SOTA puro 9-10 é o target" — **not yet met**. Aggregate 8.05 is solid pass-with-fixes; with the 20 P0s landed in Lote 10.4bis, the realistic aggregate ceiling is **8.6-8.8** (pulled up by WI-003 at 9.0-9.2 post-fix, with 001/002 reaching 8.3-8.5 each). The 9-10 target requires:
- WI-001: type-system tenant enforcement landed end-to-end + REAPI conformance suite green at first pass + cost model on outputs_check.
- WI-002: schema corrected SQL + path-key rotation column + cost story re-grounded in actual D1 pricing.
- WI-003: canonicalization decision baked into Annex A test vectors + chave-compromise threat model fully written.

These are tractable in Lote 10.4bis (1-2 day pass). The drift to 9-10 is plausible but not free.

---

**Fim audit S-04 part1.** Próximo: S-04 part2 (WI-004 HKDF + ADR-0021, WI-005 TTL worker + ADR-0019, WI-006 conformance + PRR ship gate).
