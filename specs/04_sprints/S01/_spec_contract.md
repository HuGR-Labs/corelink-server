---
id: "SPEC-CONTRACT-S01"
type: "spec_contract"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.7.0"
created: "2026-04-24"
updated: "2026-04-29"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s01", "cas", "foundation", "blake3", "hmac", "tenant-prefix", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-01: CAS Foundation (Write Path + BLAKE3 + HMAC Tenant Prefix + TLA+ Verified)

> **Status:** FROZEN (sprint.md já criado e referenciando este contract). Retroativo no Lote 8.1.
> **Lote 9.4 v1.0.0 → v1.1.0 SOTA elevation**: bump version + EVT addition + 6-col risk register + PERT explicit.

> **SOTA framing:** S-01 é a **foundation** do produto inteiro — bug em tenant isolation aqui = blast radius de 100% dos sprints subsequentes. CoreLink S-01 entrega:
> (a) **TLA+ tenant_isolation.tla 5-layer defense** (camada 5 HMAC implementada aqui) verified em CI;
> (b) **BLAKE3 SIMD-optimized** integrity verify at write (≥ 2 GB/s single core);
> (c) **TenantPrefix newtype** com private field; única construction via `derive_prefix(TDK, tenant_id)`;
> (d) **Property test 10k iter** + **pentest adversarial review** (EVT-025);
> (e) **CI gate TLC** bloqueando merge se invariantes não verdes.

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-01 |
| Nome | CAS Foundation (write path + HMAC + integrity) |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-002 (tenant isolation), FF-HR-005 (controle de segurança) |
| Duração estimada | 3 semanas (2026-04-28 → 2026-05-19) |
| WIs antecipados | 7 |

## 1. Objetivo

Implementar o primeiro incremento de CAS operacional em Worker/R2/D1: endpoint REAPI v2 de write, integridade BLAKE3 ao receber upload (CTRL-CAS-001), R2 key com HMAC tenant prefix (CTRL-AUTH-004), metadata em D1 com refcount, property test 10k iter + TLA+ gate em CI. Essa é a fundação técnica; tudo que vem depois constrói em cima.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK
- **FF-HR-002**: toca INV-TENANT-ISOLATION diretamente (implementação da camada 5 HMAC).
- **FF-HR-005**: implementa CTRL-CAS-001 + CTRL-AUTH-004 + CTRL-ISO-001..005 (controles de segurança canônicos).

## 3. Inherits_from

```yaml
inherits_from:
  - "SECURITY-MODEL"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "DATA-MODEL"
  - "KEY-MANAGEMENT"
  - "INVARIANT-REGISTRY"
  - "AUTH-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "SLO-CATALOG"
```

## 4. CAPs entregues

- **CAP-CAS-001**: BLAKE3 content-addressable storage com integrity verify at write.
- **CAP-CAS-002**: Tenant isolation criptográfico via HMAC prefix.
- **CAP-CAS-003**: Idempotent upload (same body → same digest → same key).

## 5. Requirements específicos

- **R-S01-1**: Crate `corelink-tenant-path` (ver WI-S01-001 full-spec).
- **R-S01-2**: BLAKE3 hasher + verify inline no write path.
- **R-S01-3**: R2 adapter com HMAC key (single blob ≤ 5 MiB; multipart deferred).
- **R-S01-4**: D1 schema + migration `migrations/d1/0001_blob_meta.sql` (path canonical post WI-S01-004 v1.2.0 SEAL Lote — was `migrations/001_blob_meta.sql`, moved to `d1/` subdir to keep CF and Postgres migrations in separate domains) com refcount transacional implementado em `crates/corelink-meta/`.
- **R-S01-5**: REAPI gRPC `BatchUpdateBlobs` + `ByteStream::Write` handlers.
- **R-S01-6**: Property test `prop_cas.rs` cobrindo INV-TENANT-ISOLATION + INV-CAS-INTEGRITY + INV-CAS-IDEMPOTENCY.
- **R-S01-7**: CI workflow rodando TLC nos 4 specs a cada PR.

## 6. Definition of Done

- [ ] 7 WIs SEALED (EVT-031).
- [ ] Property test 10k iter verde (EVT-002).
- [ ] TLA+ CI gate: 4/4 specs verdes bloqueiam merge se falham (EVT-022).
- [ ] SAST + clippy clean (EVT-005 + EVT-001).
- [ ] Fuzz 1h nightly em CI (EVT-008).
- [ ] Load test 10k QPS × 10 min staging (EVT-024).
- [ ] Chaos: R2 latency inject + D1 failover (EVT-023).
- [ ] Observability: `DASH-CAS` live + alertas armados (EVT-013).
- [ ] PRR (EVT-016 sign-offs 11 roles canonical HIGH_RISK per framework §33.5.4.3).
- [ ] Runbook `RB-FM-254` dry-run (EVT-017).
- [ ] SBOM CycloneDX 1.5 signed (EVT-010 + EVT-011).
- [ ] Adversarial review (EVT-025).

## 7. Completeness Criteria (delta local)

- [ ] **10.s01.1** CAS write path passa E2E em staging com 3 tenants diferentes.
- [ ] **10.s01.2** `corelink_cas_put_hash_mismatch_total == 0` sustained 24h em staging.
- [ ] **10.s01.3** Zero cross-tenant reads em property test + pentest (preparação pra S-02 read path).

## 8. Invariants

- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): implementada via HMAC prefix + dupla assertion.
- **INV-CAS-INTEGRITY** (CRITICAL, TLA+): implementada via write-time hash check.
- **INV-CAS-IDEMPOTENCY** (CRITICAL): deriva de BLAKE3 determinístico.
- **INV-CAS-IMMUTABILITY** (CRITICAL): write-once via `INSERT OR IGNORE` em D1 + `If-None-Match: *` header em R2 PUT (412 retornado em segundo writer; não usa R2 versioning bucket-level — anti-scope WI-S01-003 §7).

## 9. Quality Standards (delta local)

- **14.s01.1** Perf p99 write single blob ≤ 500ms (team tier SLO).
- **14.s01.2** Memory footprint Worker ≤ 128 MB (CF limit).
- **14.s01.3** Zero unsafe; zero unwrap em lib code.

## 10. Anti-scope

- ❌ Read path (S-02).
- ❌ Multipart upload > 5 MiB (S-05).
- ❌ Action Cache (S-04).
- ❌ GC (S-06).
- ❌ Billing events (S-10).
- ❌ Clerk SSO integration (stub PAT; real em S-03).

## 11. Dependencies

- Blocker: S-00 (roadmap + capabilities catalog).

## 12. WIs antecipados

| ID | Título | Estimativa |
|---|---|---|
| WI-S01-001 | Lib tenant_path HMAC | 42h (PERT: O=28h M=42h P=68h) |
| WI-S01-002 | BLAKE3 hasher + verify at write | 24h (PERT: O=16h M=24h P=38h) |
| WI-S01-003 | R2 adapter single-blob | 32h (PERT: O=22h M=32h P=50h) |
| WI-S01-004 | D1 schema + migration + refcount | 20h (PERT: O=14h M=20h P=32h) |
| WI-S01-005 | REAPI BatchUpdateBlobs handler | 48h (PERT: O=32h M=48h P=78h) |
| WI-S01-006 | Property tests 10k iter | 24h (PERT: O=16h M=24h P=38h) |
| WI-S01-007 | CI workflow TLC gate + SBOM | 16h (PERT: O=10h M=16h P=26h) |

Total: ~206h PERT-weighted (~5 weeks 1 dev; 3 weeks 2 devs parcial). Buffer 5 dias confere.

## 13. Duração

3 semanas HIGH_RISK. Buffer 5 dias úteis.

## 14. Critérios de promoção

- DoD §6 completa.
- PRR approved (11 sign-offs canonical HIGH_RISK).
- Staging 72h sem SEV-1.

## 15. Riscos (registry expandido — 6 colunas)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **TLA+ small-bound não pega bug produção** | M | M | HIGH | M | LOW | Property test 10k iter + pentest + Apalache symbolic future + adversarial review trimestral. |
| **Clerk API muda mid-sprint** | L | L | MEDIUM | L | LOW | PAT stubado; integração real em S-03; weekly Clerk changelog review. |
| **BLAKE3 crate CVE** | L | L | CRITICAL | L | LOW | cargo-audit daily + dupla hash fallback (SHA-256 secondary verify); Lote 9.1 supply chain S-12. |
| **D1 migration rollback** | M | L | HIGH | L | LOW | PAT-MIGRATION-IDEM-001; staging rehearsal; transactional migration. |
| **HMAC key leak** (TDK exfil) | L | M | CRITICAL | L | LOW | Cloudflare Secrets Store; never logged; INV-CONF-AT-REST + KMS-backed. |
| **Cross-tenant via path collision** (HMAC truncation) | L | M | CRITICAL | L | LOW | TenantPrefix newtype private field; single construction; TLA+ verifies; property test 100k. |
| **Cost regression** > 10% baseline | M | L | MEDIUM | L | LOW | Lote 9.4 §14.10 cost regression gate; criterion benchmark. |

---

## 16. Changelog

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | Gustavo | Spec contract retroativo (Lote 8.1). |
| 1.1.0 | 2026-04-24 | Gustavo (Lote 9.4 SOTA elevation) | EVT addition + 6-col risk register + PERT explicit. |
| 1.7.0 | 2026-04-29 | Gustavo (S-01 implementation Lote — WI-S01-004 SEAL post codex adversarial review) | **WI-S01-004 SEALED** com codex score ≥ 8.5/10. Workspace agora carrega 4 impl crates (`tenant-path`, `corelink-hash`, `corelink-worker`, `corelink-meta`). New deliverables: (a) `crates/corelink-meta/` — `MetaStore` trait + `InMemoryMetaStore` fake (host-side test backend; real Cloudflare D1 binding shim deferred to WI-S01-005 alongside miniflare integration tier — same trait abstraction split pattern aplicado em WI-S01-003 R2). 7-method canonical SQL templates (INSERT_BLOB_META, UPDATE_REFCOUNT_INC/DEC, UPDATE_SOFT_DELETE, SELECT_BLOB_META, INSERT_AUDIT_OUTBOX, SELECT_AUDIT_OUTBOX_BY_REQUEST) com `?N`-style numeric placeholders + dedicated module preventing string-format-of-user-input. (b) `migrations/d1/0001_blob_meta.sql` — canonical DDL: blob_meta(tenant_id TEXT, digest TEXT, size_bytes INTEGER CHECK > 0, refcount INTEGER DEFAULT 1 CHECK >= 0, created_at INTEGER, last_accessed_at INTEGER, deleted_at INTEGER NULL, PK(tenant_id, digest)) + audit_outbox(id TEXT PK, tenant_id TEXT, digest TEXT NULL, request_id TEXT, event_type TEXT, payload_json TEXT, enqueued_at INTEGER, emitted_at INTEGER NULL, UNIQUE(request_id, event_type)) + 3 partial indexes (alive set, GC candidates, pending drain). (c) `scripts/migrate_d1.sh` — wrangler-d1 migration runner per-env per-region. (d) `.github/workflows/corelink-meta.yml` — PR debug+release+wasm-build+fuzz-smoke + nightly fuzz/mutants/audit. (e) **Atomic batch contract** via plan-then-commit pattern — audit-outbox staging is plan-only (can fail with AuditIdempotencyConflict), blob_meta mutation runs next, audit row commits only if both sides succeed → no orphan blob_meta or orphan audit row. (f) **Schema canonical alignment** in same Lote: `data_model.md §4.2` v0.1.0 → v0.2.0 (audit_outbox DDL added; idx renamed to canonical pair). (g) Test surface: 27 unit/integration + 4 property tests (debug + release); 100% cargo-mutants kill rate (39/39 viable); 60s fuzz smoke clean per target (209k + 535k iter, zero crashes). WI-S01-004 v1.1.0 → v1.2.0 com §13 artifacts table rewrite + §6.1 path corrections + §31 changelog. |
| 1.6.0 | 2026-04-29 | Gustavo (S-01 implementation Lote — WI-S01-002 SEAL post codex round-1 6.1 → target 8.5+) | **WI-S01-002 SEALED** com codex P0 architectural fix: items 5-7 do §6.1 (R2 integration, métricas, audit emit) reframed as "DEFERRED to WI-S01-003/005" porque belongtem ao adapter+handler que *consomem* o crate, não ao crate em si. Concrete in-repo evidence of CTRL-CAS-001 type-driven enforcement adicionada via `BlobStoreWrite` trait em `corelink-hash` cuja signature mandata `&VerifiedBody`; `MemoryBlobStore` fake exercita o contrato. Test surface: 19 unit/property + 2 blob_store_contract + 1 doctest = 22 tests verde (debug); 2 release-only gates (perf 5MiB <50ms, ct-variance <5% delta) + 10 canonical regression vectors covering BLAKE3 chunk boundaries (1023/1024/1025/2048/4096/65536). Fuzz 60s 17.6M runs zero crashes; criterion bench native ~3 GB/s; CI workflow `corelink-hash.yml` PR + nightly lanes; WASM target build verified. P2 fixes: `Digest::as_bytes` properly `#[doc(hidden)] pub` (testing escape, not API surface); `HashMismatch::code()` + `pub const COR_CAS_DIGEST_MISMATCH` for canonical taxonomy alignment; chunk-boundary vectors closing the 1024-byte tree-mode blind spot codex flagged. |
| 1.5.0 | 2026-04-29 | Gustavo (S-01 implementation Lote — codex P1 remediation pre-SEAL of WI-S01-001) | **6 P1 fixed during impl phase** (codex 7.1 → target 8.5+/10): (a) **WI §9.3 entropy math corrected** — was "128 bits, 2^-64 collision" (raw HMAC interpretation); now "96 bits, per-pair 2^-96, birthday-bound 2^48" (b64url truncation interpretation, aligned with §1 + §7.1 canonical). (b) **WI §1 wording tightened** — "16 bytes raw" → "16 bytes ASCII" + ADR-0043 reference; lint enforcement updated to `#![forbid(unsafe_code)]` + Cargo `[lints]`. (c) **WI §9.2 reframed** — HKDF rationale moved from "preferred derivation" to "considered and rejected", with explicit reasoning (HMAC-SHA256 already provides PRF separation; HKDF doubles cost without isolation gain). (d) **ADR-0021 §1.1 L77 tightened** — "raw HMAC; S-01 corelink-tenant-path" → "raw HMAC bytes inside corelink-tenant-path, b64url-truncated to 16 ASCII chars at serialization boundary; see ADR-0043". (e) **ADR-0043 v1.1.0** — entropy math corrected in decision matrix + negative trade-offs + spec follow-ups closed in same Lote. (f) Implementation artifacts: `crates/tenant-path/` complete with HMAC-SHA256 + b64url-trunc-16; 22 tests verde including 4 property tests at 10k iter + 5 cross-language regression vectors + perf regression test (release-only); `from_bytes` takes `Zeroizing<[u8; 32]>` so caller's source buffer is scrubbed; `TenantPrefix::as_bytes` removed (pub-only `as_str`/`Display` per WI §7 anti-scope); `#[allow(clippy::expect_used)]` strategic em 3 sites provably-infallible com rationale por callsite. |
| 1.4.0 | 2026-04-29 | Gustavo (Lote 10.1bis cycle 3 codex SEAL remediation) | **2 P1 + 1 P2 fixed** (score 8.9→target ≥9.0): (a) **WI-005 batch payload contract** — explicit BatchUpdateBlobs aggregate cap ≤ 4 MiB (REAPI MaxBatchTotalSizeBytes); per-blob inline ≤ 4 MiB; ByteStream::Write 4-5 MiB single blob; §9.4 narrative + §14.5.9 memory budget reconciled (peak ≤ 8 MiB defensible per surface); §1 Intent + §1.6.1 body validation explicit. (b) **WI-005 proto type canonical** — BatchUpdateBlobs request format `build.bazel.remote.execution.v2.BatchUpdateBlobsRequest/Response` (não ByteStream WriteRequest); ByteStream::Write proto separado clarified §6.1.2. (c) **WI-006/007 CI artifact unification** — property tests workflow embedded em cas_foundation.yml (PR) + nightly.yml (extended) per WI-007 §6.1.1+6.1.2; fuzz harness path tenant_path_decode.rs unified (L171 + L320 alinhados). |
| 1.3.0 | 2026-04-28 | Gustavo (Lote 10.1bis cycle 2 codex SEAL remediation) | **2 P1 ENGINEERING + 3 P2 EDITORIAL fixed** (score 7.1→8.6→target ≥9.0): (a) **WI-004 schema canonical** — `tenant_id BLOB`→`TEXT` + `digest 'BLAKE3 hex 64-char'`→`'algo:hex'` (per data_model.md §1 L71 + §2.1 L94 + §4.2 L253-254); audit_outbox `id`/`tenant_id` BLOB→TEXT for consistency; LINDDUN UUIDv4→UUIDv7. (b) **WI-005 gRPC status canonical** — 413=OUT_OF_RANGE→RESOURCE_EXHAUSTED (8) (alinha §9.6 L289 + bazelbuild/remote-apis v2.13.0); hash-mismatch AC code 13 (INTERNAL)→code 10 (ABORTED) canonical; full mapping table now includes gRPC numeric codes. (c) **WI-007 fuzz harness** — base32 → HMAC16 base64url canonical. (d) **WI-003 sign-off placeholder** — count ambiguity removed (Owner+FA já incluídos nos 11). |
| 1.2.0 | 2026-04-28 | Gustavo (Lote 10.1bis cycle 1 codex SEAL remediation) | **4 codex 7.1 blockers fixed**: (a) **P0 blob_meta canonical** — WI-004 refcount DEFAULT 0→1 (S-06 GC mark requires `refcount > 0` reachability per data_model.md §4.2 L257); timestamps seconds → milliseconds (canonical); 3 AC scenarios updated (first INSERT refcount=1; refcount race initial=1 final=101); §9.4 design decision aligned. (b) **P1 TenantPrefix encoding** — WI-001+003 base32 → HMAC16 canonical (`b64(HMAC_SHA256)[0:16]` per remote_cache_product_profile.md §7.1 + storage_semantics_matrix.md §3); AC-4 renamed; helper renamed `to_hmac16`; key format strings unified; ST-004 description updated. (c) **P1 audit semantics WI-005** — split INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (handler→outbox via D1 batch, this WI scope) from INV-AUDIT-APPEND-ONLY (S-09 chain immutability, downstream); §12 invariants table corrected; §10.5.5 completeness clarified. (d) **P1 WI-006 anti-scope alignment** — property test reframed for storage-layer (R2Reader direct, not REAPI read endpoint S-02); UUIDv4 → UUIDv7 canonical (data_model.md §3); arbitrary_cas_op enum clarified (Put + GetStorageLayer; Delete is S-06); refcount race scenario explicit S-01 scope. (e) **P2 ADR-0015 collision** — renamed to ADR-0043 (HMAC tenant prefix algorithm; ADR-0015 já alocado para reproducible-build-best-effort); 4 references updated + sprint.md DoD. (f) **P2 sign-off normalization** — 10-12/13 → 11 canonical (framework §33.5.4.3 HIGH_RISK matrix + ADR-0034 solo-tier waiver); 14 locations updated across spec_contract + sprint + 5 WIs. |

---

**Fim de spec contract S-01.**
