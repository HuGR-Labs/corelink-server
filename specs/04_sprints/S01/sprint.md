---
id: "S-01"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
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
tags: ["sprint", "s01", "cas", "foundation", "high-risk"]
---

# Sprint S-01 — CAS Foundation (Write Path + Tenant Isolation + Integrity)

> **doc_status:** DRAFT · **lane:** HIGH_RISK · **Versão:** 1.0.0 · **2026-04-24**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** até ≥ 2 reviewers nomeados
> **inherits_from:** SECURITY-MODEL + REMOTE-CACHE-PRODUCT-PROFILE + DATA-MODEL + KEY-MANAGEMENT + INVARIANT-REGISTRY + AUTH-MODEL + OBSERVABILITY-MODEL + FAILURE-MODES + RESILIENCE-PATTERNS + SLO-CATALOG

> **🚦 Phase boundary:** este sprint é **Fase 1 — Remote Cache** (não toca execute-action / Remote Execution).

---

## 1. Objetivo

Implementar o **CAS write path mínimo viável** do CoreLink com integridade criptográfica end-to-end e isolamento de tenant, exercitando toda a arquitetura spec-first:

1. Endpoint REAPI v2 `ContentAddressableStorage/BatchUpdateBlobs` + `ByteStream/Write` em Worker-CP (Rust/WASM).
2. Write path enforça `INV-CAS-INTEGRITY` (CTRL-CAS-001): server rejeita se `hash(body) ≠ claimed_digest`.
3. R2 key canônico com HMAC tenant prefix (CTRL-AUTH-004, REG-NAMESPACE-001..005): `cas-<region>/<HMAC(tenant_key, tenant_id)[:16]>/blake3/<hex[0:2]>/<hex[2:4]>/<hex>`.
4. Metadata em D1 `blob_meta` com refcount=1 no primeiro write.
5. Property test + TLA+ check (`cas_integrity.tla`) obrigatórios em CI.

## 2. Escopo do sprint

### 2.1 In-scope

- **WI-S01-001**: Lib `tenant_path` + HMAC prefix derivation.
- **WI-S01-002**: BLAKE3 hasher + verify at write-time (CTRL-CAS-001).
- **WI-S01-003**: R2 write path (single blob ≤ 5 MiB).
- **WI-S01-004**: D1 `blob_meta` DDL + INSERT com refcount.
- **WI-S01-005**: REAPI `BatchUpdateBlobs` Worker-CP handler.
- **WI-S01-006**: Property test Rust `proptest` — 10k iter cobrindo INV-TENANT-ISOLATION + INV-CAS-INTEGRITY + INV-CAS-IDEMPOTENCY.
- **WI-S01-007**: CI workflow com TLC gate (`cas_integrity.tla` + `tenant_isolation.tla`).

### 2.2 Anti-scope

- ❌ Multipart upload (blobs > 5 MiB) — próximo sprint.
- ❌ Read path — WI separado em S02.
- ❌ Action Cache (AC) — S03.
- ❌ Garbage Collection — S04.
- ❌ Remote Execution (Fase 2 pós-GA).
- ❌ Billing events — S05.
- ❌ Frontend / admin UI — fora de escopo.

## 3. Customer Impact & Journey

> Herda de `remote_cache_product_profile.md §2` (Customer Journey).

**JTBD coberto:** "Como dev de CI do tenant X, eu quero fazer upload de blob determinístico e ter garantia criptográfica que meu build output não foi corrompido nem servido a outro tenant."

**CAPs entregues:**
- `CAP-CAS-001` (BLAKE3 content addressing)
- `CAP-CAS-002` (tenant isolation criptografa via HMAC prefix)
- `CAP-CAS-003` (integrity enforce at write-time)

## 4. Capability Mapping (trace)

Ver `01_product/capabilities.md` (a criar) para CAPs finais. Este sprint endereça direct capabilities.

## 5. Deliverables

| ID | Entregável | Onde | Definition of Done |
|---|---|---|---|
| S01-D1 | Lib `tenant_path::derive_prefix(tenant_id) -> [u8; 16]` | `src/tenant_path/mod.rs` | Property test: 1000 pares tenants distintos → prefixos distintos (injective) |
| S01-D2 | BLAKE3 verifier inline no write | `src/cas/write.rs` | Write rejeita se `Hash(body) != claimed_digest` (CTRL-CAS-001) |
| S01-D3 | R2 adapter com HMAC key | `src/storage/r2.rs` | Key format = `cas-<region>/<HMAC16>/blake3/<shards>/<hex>` |
| S01-D4 | D1 schema + migration | `migrations/001_blob_meta.sql` | INSERT transacional com `refcount=1` |
| S01-D5 | REAPI BatchUpdateBlobs | `src/reapi/cas.rs` | gRPC handler completo, proto gen via tonic |
| S01-D6 | Property tests | `tests/prop_cas.rs` | 10k iter verde; cover INV-TENANT-ISOLATION + INV-CAS-INTEGRITY + INV-CAS-IDEMPOTENCY |
| S01-D7 | CI workflow TLC gate | `.github/workflows/tla.yml` | 4 specs TLA+ rodam em cada PR; falhar em 1 invariante quebra CI |

## 6. Escopo técnico por camada (inherits_from)

Todos os elementos abaixo herdam dos canonical sources listados no `inherits_from` do YAML. Deltas locais do sprint:

### 6.1 Storage (herda `data_model.md §5` + `storage_semantics_matrix.md §3`)

- R2 buckets provisionados: `cas-wnam`, `cas-weur`, `cas-sam` (3 regiões para S01).
- D1 instance per região com schema `blob_meta` conforme `data_model.md §4.2`.
- **Delta local:** apenas blobs single-put ≤ 5 MiB neste sprint; multipart deferred.

### 6.2 Auth (herda `auth_model.md §5`)

- PAT scope `cas-w` obrigatório para writes (ver `auth_model.md §3`).
- HMAC tenant prefix via `key_management.md §2` (TDK → Path-HMAC key via HKDF `info="path"`).
- **Delta local:** PAT validation stub em S01 (pre-integration com Clerk); WI-S01-* usa test PATs em CI. Integração Clerk completa em S02.

### 6.3 Invariantes verificadas (herda `invariant_registry.md §3`)

- `INV-TENANT-ISOLATION` (CRITICAL): TLA+ `tenant_isolation.tla` + property test
- `INV-CAS-INTEGRITY` (CRITICAL): TLA+ `cas_integrity.tla` + write-time check
- `INV-CAS-IDEMPOTENCY` (CRITICAL): property test (same body → same digest)
- `INV-CAS-IMMUTABILITY` (CRITICAL): write-once via `INSERT IF NOT EXISTS` em D1

### 6.4 SLOs aplicáveis (herda `slo_catalog.md §4`)

- `SLO-AVAIL-CAS-PUT`: 99.9% availability em team tier (meta S01)
- `SLO-LAT-CAS-PUT`: 99% < 1s para blobs ≤ 16 MiB (meta S01 com single-put ≤ 5 MiB)
- `SLO-CORRECT-CAS`: 100% — todo write rejeitado por INV-CAS-INTEGRITY é logged + métrica

## 7. Definition of Done (lane HIGH_RISK)

Todas abaixo obrigatórias (framework §33.5.4.1 HIGH_RISK matrix):

- [ ] Todos 7 WIs do sprint em status `SEALED`
- [ ] Property tests 10k iter verdes (EVT-002)
- [ ] TLA+ checks verdes em CI (EVT-022): `tenant_isolation`, `gc_correctness`, `cas_integrity`, `audit_immutability`
- [ ] SAST clean (clippy -D warnings + semgrep) (EVT-005)
- [ ] Fuzz targets para parsers: 1h nightly em CI (EVT-008)
- [ ] Integration tests E2E em staging CF (EVT-002)
- [ ] Load test 10k QPS sustained por 10 min em staging (EVT-024)
- [ ] Chaos experiments: inject R2 latency + inject D1 primary failure (EVT-023)
- [ ] Progressive rollout dry-run em staging (EVT-038)
- [ ] Observability plan executado (métricas emitindo, dashboards criados) (EVT-013)
- [ ] PRR completa (EVT-016 sign-offs)
- [ ] Runbook `RB-FM-254` dry-run executado (EVT-017)
- [ ] SBOM gerado + assinado (EVT-010 CycloneDX 1.5)
- [ ] Adversarial review executado (EVT-025 pentest interno)
- [ ] ADR-0015 (se emergir decisão arquitetural não prevista)

## 8. Dependencies

- **Blocker:** framework v1.0.0-rc1 (atingido).
- **Blocker:** canonical sources REMOTE-CACHE-PRODUCT-PROFILE + DATA-MODEL + SECURITY-MODEL + KEY-MANAGEMENT + INVARIANT-REGISTRY em DRAFT (done).
- **Blocker:** TLA+ specs rodando verde localmente (done, Lote 7.1).
- **Soft dep:** Clerk SSO stub (se Clerk API mudar, WI-S01-005 pode precisar rework — ver Risco R-S01-002).

## 9. Timeline

- **Sprint kick-off:** 2026-04-28 (segunda, após approval)
- **Mid-sprint review:** 2026-05-05
- **Sprint close target:** 2026-05-19 (3 semanas — HIGH_RISK timing)
- **Buffer:** 5 dias úteis (0-buffer é anti-pattern em HIGH_RISK)

## 10. Risk Register

| ID | Risco | Prob | Impacto | Mitigação | Owner |
|---|---|---|---|---|---|
| R-S01-001 | TLA+ small-bound não pega bug que existe em produção large-scale | M | H | Property test 10k iter + pentest adversarial + Apalache futuro | Architect |
| R-S01-002 | Clerk API schema muda mid-sprint | L | M | PAT validation stubado inicialmente; Clerk integration isolada em WI único | Tech Lead |
| R-S01-003 | BLAKE3 `blake3` crate bug descoberto | L | CRITICAL | `cargo-audit` nightly + dupla hash fallback (SHA-256) opcional | Security Lead |
| R-S01-004 | D1 schema migration falha em rollback | M | H | PAT-MIGRATION-IDEM-001; staging rehearsal obrigatório | SRE Lead |
| R-S01-005 | Tenant isolation regression em refactor futuro | M | CRITICAL | TLA+ check em CI bloqueia merge + property test regression | Architect |

## 11. Observability Plan (delta do sprint)

Herda de `observability_model.md §4`. Delta local:

**Métricas novas emitidas:**
- `corelink_cas_put_requests_total{outcome, digest_algo, blob_size_bucket}` (ver §4.2 RED pattern)
- `corelink_cas_put_duration_seconds_bucket{outcome}`
- `corelink_cas_put_hash_mismatch_total{tenant_id}` (CTRL-CAS-001 failure counter)
- `corelink_cas_isolation_assertion_total{outcome}` (PAT-AUTHZ-001 + CTRL-AUTHZ-002)
- `corelink_storage_r2_put_duration_seconds`

**Dashboards novos:**
- `DASH-CAS` (já planejado em `observability_model.md §8`); implementar widgets principais para S01.

**Alertas:**
- `cas_put_hash_mismatch > 0` → SEV-1 imediato (FM-254 cache poisoning).
- `cas_put_isolation_violation > 0` → SEV-1 imediato (FM-253 cross-tenant).
- SLO burn-rate alerts per `slo_catalog.md §9.3` multi-window.

## 12. Security & Privacy (delta)

Herda de `security_model.md` + `privacy_model.md`. Delta local:

- **STRIDE delta:** introduz superfície THR-T-001 (cache poisoning ataque no write path); mitigado por CTRL-CAS-001 write-time check.
- **LINDDUN delta:** introduz processamento de blob body (potencialmente PII-containing do cliente); mitigado por at-rest encryption R2 SSE-S3 + tenant isolation HMAC.
- **Novos trust boundaries:** nenhum (mantém TB-0..TB-5 canônicos).
- **Privacy impact:** nulo direto (blob body é tenant-controlled; HuGR=operador via DPA).

## 13. Post-mortem hooks

- Todo incident P0/P1 durante ou pós-S01 → post_mortem obrigatório referenciando FM correspondente.
- PRR signoff exige zero SEV-1 em 72h de staging rollout.

## 14. Sign-off (HIGH_RISK — 10–12 roles)

Ver tabela canônica em `00_framework.md §33.5.4.3`. Sprint S-01 exige **todos os 11 papéis** + Compliance + Adversarial = **13 total** (incluindo extensões sprint-only).

> Sign-off será preenchido ao final do sprint em `sprint.md §14.1`. Template em `specs/_templates/sprint_contract.md §20.1`.

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | Criação do sprint S01 — CAS Foundation. HIGH_RISK com FF-HR-002 (tenant isolation) + FF-HR-005 (altera controle de segurança CTRL-CAS-001). |

---

**Fim de S-01 sprint contract.** Próximo doc: `work_items/WI-S01-001-tenant-path-hmac.md`.
