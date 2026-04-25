---
id: "SPEC-CONTRACT-S01"
type: "spec_contract"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-24"
updated: "2026-04-24"
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
- **R-S01-4**: D1 schema + migration `001_blob_meta.sql` com refcount transacional.
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
- [ ] PRR (EVT-016 sign-offs 10-12 roles).
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
- PRR approved (10-12 sign-offs).
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

**Fim de spec contract S-01.**
