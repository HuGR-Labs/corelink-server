---
id: "SPEC-CONTRACT-S04"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.3.0"
created: "2026-04-24"
updated: "2026-04-25"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s04", "action-cache", "reapi", "merkle", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-04: Action Cache (REAPI GetActionResult/UpdateActionResult + Merkle + TTL Infrastructure)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-04 |
| Nome | Action Cache (AC) |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-002 (cross-tenant AC = catastrophic FM-303), FF-HR-005 (CTRL-AC-001 + CTRL-AC-002) |
| Duração estimada | 3 semanas + buffer 5 dias |
| WIs antecipados | 6 |
| SOTA target | AC tier-1 — Bazel/Buck2 conformance + Merkle verify dual-side + cache hit ratio > 70% staging + tenant-scoped TLA+ verified |

## 1. Objetivo

Implementar **Action Cache (AC) production-grade** — a "cereja" do remote cache Bazel/Buck2: dado hash de action determinística, retorna resultados de execução previamente computada. Requer **integridade Merkle dual-side** (server reject invalid trees + client verify) + **tenant-scoped strict** + **TTL infrastructure** (defaults per-tier delegados a S-07/ADR-0019). Sem AC, CoreLink é só blob store; com AC, é remote cache real que acelera builds. **Bug em tenant isolation = FM-303 catastrophic** (build result vaza entre customers).

**Por que SOTA:** competitors entregam AC com Merkle verify server-only (cache poisoning vector); REAPI v2 conformance varia (BuildBuddy 95%, NativeLink 80%); cache hit ratio raramente exposto como business métrica. CoreLink S-04 entrega: (a) Merkle verify dual-side (server + client); (b) **REAPI v2 conformance suite passing 100%** (bazelbuild/remote-apis test suite); (c) cache hit ratio em DASH-AC + customer-visible em S-16; (d) AC digest signing CTRL-AC-002 (HKDF). Reference: **REAPI v2 spec**, **Bazel ActionCache proto**, **NIST SP 800-185** (KMAC for Merkle).

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (11 sign-offs canonical per framework §33.5.4.3 + ADR-0034; Crypto SME folds into Architect role; DBA folds into Architect; AppSec é o 11º slot).
- **FF-HR-002**: AC cross-tenant = serve build result de outro tenant = catastrófico (FM-303).
- **FF-HR-005**: implementa CTRL-AC-001 (Merkle verify) + CTRL-AC-002 (digest signing HKDF).

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"  # REAPI v2 ActionCache semantics
  - "SECURITY-MODEL"                # CTRL-AC-001..002
  - "DATA-MODEL"                    # ac_meta schema §4.2
  - "INVARIANT-REGISTRY"            # INV-AC-OUTPUTS-VALID, INV-AC-TENANT-SCOPED
  - "KEY-MANAGEMENT"                # TDK reuse + HKDF info="ac-sig"
  - "OBSERVABILITY-MODEL"           # DASH-AC, métricas hit ratio
  - "FAILURE-MODES"                 # FM-303 (AC cross-tenant)
  - "SLO-CATALOG"                   # SLO-AVAIL-AC, SLO-LAT-AC-HIT
  - "RESILIENCE-PATTERNS"           # PAT-RETRY-IDEMPOTENT-001
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-AC-001** | REAPI GetActionResult (cache hit) | Returns `ActionResult` proto se cached; 404 se miss + negative cache (S-02 alignment). |
| **CAP-AC-002** | REAPI UpdateActionResult (cache write) | Writes `ActionResult`; pre-write Merkle verify; idempotent via `(tenant_id, action_digest)` UNIQUE. |
| **CAP-AC-003** | Merkle verification dual-side | Server-side reject invalid trees pre-persist; client-side verify post-download (CAP-CAS-005 reuse). |
| **CAP-AC-004** | AC TTL management infrastructure | TTL worker + refresh-on-hit + expiry detection. **Default value supersedido por S-07 CAP-EVICT-002 per-tier table** (ADR-0019). |
| **CAP-AC-005** | AC entry invalidation | Client-requested via DELETE; admin override via S-13 admin plane. |
| **CAP-AC-006** | Cache hit ratio business métrica | `corelink.cache.hit_ratio{type=ac, tenant_tier, region}` exposed em DASH-AC + customer dashboard S-16. |

## 5. Requirements específicos

- **R-S04-1**: REAPI `ActionCache::GetActionResult` + `UpdateActionResult` Worker handlers; gRPC + REST surface.
- **R-S04-2**: D1 schema `ac_meta` (conforme `data_model.md §4.2`) + R2 bucket `ac-<region>`; UNIQUE constraint `(tenant_id, action_digest)`.
- **R-S04-3**: Crate `corelink-ac` com:
  - ActionResult proto codec.
  - Merkle tree builder/verifier.
  - INV-AC-OUTPUTS-VALID enforcement: pre-persist verify outputs digests existem em `blob_meta` alive (não tombstoned).
- **R-S04-4**: AC digest signing via CTRL-AC-002 (HKDF tenant key info="ac-sig"). **Decisão**: HKDF-only (não Ed25519); rationale em ADR-0021 (criar como sub-PR durante S-04 implementation — HKDF é symmetric + faster + suficiente para integrity binding tenant-scoped; Ed25519 over-kill para use case).
- **R-S04-5**: TTL worker (cron DO) que expira entries > `expires_at`; reuses S-07 evicition framework boundary (ADR-0019).
- **R-S04-6**: Metrics + dashboards (DASH-AC conforme `observability_model §8`); cache hit ratio per (tenant_tier, region).
- **R-S04-7**: Negative cache integration: AC miss popula `ac_neg:<action_digest>` KV TTL 60s (mais curto que CAS — actions ficam stale rápido).

## 6. Definition of Done

- [ ] **WIs SEALED**: 6/6 (EVT-031).
- [ ] **E2E**: Bazel build com `--remote_cache=corelink://...` → cache hit reduz tempo em > 50% em rebuilds (EVT-018).
- [ ] **Property test 100k**: AC entry de Tenant A nunca retornada a Tenant B (EVT-002).
- [ ] **TLA+ invariantes verdes**: tenant_isolation.tla covers AC via `INV-AC-TENANT-SCOPED` derivation (EVT-022).
- [ ] **Chaos**: TTL expiry sob load + stale read handling (EVT-023).
- [ ] **SLO-AVAIL-AC e SLO-LAT-AC-HIT** sustained 72h staging (EVT-021).
- [ ] **PRR HIGH_RISK** + adversarial review: SRE + Security + Engineer + QA + Product + Compliance + Privacy + Architect + AppSec + 2 peers + Crypto SME (HKDF review) (EVT-031).
- [ ] **REAPI v2 conformance** (bazelbuild/remote-apis test suite) passa para AC ops (EVT-002 + EVT-018).
- [ ] **Merkle verify**: invalid tree rejected pre-persist + post-download client (EVT-002).
- [ ] **Cache hit ratio** > 70% em workload sintético Bazel (EVT-021).
- [ ] **ADR-0021** "AC digest signing HKDF vs Ed25519" decision documented (EVT-046 if Legal review needed; EVT-027 ADR).
- [ ] **Cost regression gate** (§14.10): AC hit hot path benchmark; PR > 10% regression bloqueia (EVT-002).
- [ ] **RB-FM-303** (AC cross-tenant) dry-run executado em staging (EVT-017).

## 7. Completeness Criteria (delta local)

- [ ] **10.s04.1** REAPI v2 conformance test suite (bazelbuild/remote-apis) passa para AC ops 100% (EVT-002).
- [ ] **10.s04.2** Merkle tree verify: invalid tree rejeitada no read path + write path (EVT-002).
- [ ] **10.s04.3** AC hit ratio > 70% em workload de test Bazel sintético (EVT-021).
- [ ] **10.s04.4** **TTL infrastructure aligned com S-07** (ADR-0019) — defaults per-tier propagated ao deploy time.
- [ ] **10.s04.5** **HKDF digest signing** verified via property test 10k iter (EVT-002).

## 8. Invariants

### Mantidas

- **INV-AC-OUTPUTS-VALID** (HIGH): outputs referenciados existem em `blob_meta` alive; reconcile diário (S-06 GC interaction).
- **INV-AC-TENANT-SCOPED** (CRITICAL, TLA+): deriva de INV-TENANT-ISOLATION; AC cross-tenant impossible.
- **INV-CAS-IMMUTABILITY** (CRITICAL): AC entry é imutável pós-write (atualização = nova entrada via UPDATE com `(tenant_id, action_digest, version)`).

### Não cria invariants novas — INV-AC-* já em registry §3.3.

## 9. Quality Standards (delta local)

- **14.s04.1 Merkle verify p99 ≤ 10ms** pra trees ≤ 100 nodes (criterion benchmark).
- **14.s04.2 AC GetActionResult p99 ≤ 150ms** (SLO-LAT-AC-HIT) sustained.
- **14.s04.3 Cache hit ratio** emitido como métrica business (`corelink.cache.hit_ratio{type=ac}`); customer-visible S-16.
- **14.s04.4 Tenant isolation defense-in-depth**: 5 layers conforme `auth_model.md §8.1` (AC herda CAS isolation pattern).
- **14.s04.5 Cost regression gate** (§14.10): AC GET hot path benchmark; PR > 10% regression bloqueia.
- **14.s04.6 REAPI v2 conformance** mantido CI (bazelbuild/remote-apis suite quarterly re-run).
- **14.s04.7 HKDF discipline**: info string `ac-sig` fixo; nunca dynamic; tenant_key reused do S-01 TDK.

## 10. Anti-scope

- ❌ Execute Action (Fase 2 — Remote Execution; pós-GA).
- ❌ AC cross-tenant dedup (FM-303 risk; requer ADR explicit + bloqueado at GA).
- ❌ AC Content-Delivery Network (CDN layer em S-14 multi-region).
- ❌ AC TTL defaults per-tier (delegado a S-07 / ADR-0019).
- ❌ Ed25519 digest signing (rejected; HKDF é suficiente; ADR-0021).
- ❌ AC entry version migration tooling — entry imutável; mudança = nova entry.
- ❌ AC outputs proativos pre-fetch (caching reactive only at GA).

## 11. Dependencies

### Hard blockers

- **S-01 SEALED** (CAS write path; outputs blobs precisam existir).
- **S-02 SEALED** (CAS read path para output blobs referenciados).
- **S-03 SEALED** (auth tenant ctx).

### Soft blockers

- **S-09** (observability — DASH-AC; pode ser delayed).

### Outbound

- **S-07** (eviction TTL defaults supersede CAP-AC-004 — ADR-0019).
- **S-13** (admin plane — invalidation override).
- **S-15** (CLI/SDK consume AC API).
- **S-20** (GA exige cache hit ratio sustained + REAPI conformance).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S04-001** | REAPI ActionCache proto handlers + REST surface | gRPC + REST handlers; auth middleware reuse; tenant context propagation | 12h | 18h | 28h | **18.7h** |
| **WI-S04-002** | D1 ac_meta + R2 ac bucket + schema + UNIQUE constraint | D1 migration; R2 bucket per-region; idempotency UNIQUE; integration test | 10h | 14h | 22h | **14.7h** |
| **WI-S04-003** | Crate corelink-ac (Merkle codec + verify dual-side) + INV-AC-OUTPUTS-VALID enforcement | Merkle builder/verifier; ActionResult codec; pre-persist outputs check; criterion benchmark | 14h | 22h | 36h | **23.0h** |
| **WI-S04-004** | CTRL-AC-002 digest signing HKDF + ADR-0021 + property test | HKDF info="ac-sig"; signing/verify; ADR-0021 decision; property test 10k | 10h | 14h | 22h | **14.7h** |
| **WI-S04-005** | TTL worker (cron DO) + S-07 boundary alignment ADR-0019 | TTL worker DO; expiry job; S-07 boundary respect; refresh-on-hit | 10h | 16h | 26h | **16.7h** |
| **WI-S04-006** | REAPI conformance tests + DASH-AC dashboards + RB-FM-303 dry-run + PRR | bazelbuild/remote-apis suite; DASH-AC JSON; cache hit ratio métrica; RB-FM-303 walkthrough; PRR | 12h | 18h | 30h | **19.0h** |

**Total PERT:** ~107h ≈ 13 dias work × 1 eng. Buffer 5 dias confere com 3 semanas.

## 13. Duração + Timeline

- **Duração:** 3 semanas (15 dias úteis) + buffer 5 dias.
- **Marcos:**
  - **D+3:** WI-001 + WI-002 SEALED (REAPI handlers + schema).
  - **D+8:** WI-003 SEALED (Merkle dual-side).
  - **D+10:** WI-004 SEALED (HKDF signing + ADR-0021).
  - **D+12:** WI-005 SEALED (TTL worker + boundary).
  - **D+15:** WI-006 SEALED (conformance + dashboards + PRR).
  - **D+17:** Sprint review.

## 14. Critérios de promoção

- DoD complete + 72h staging sustained.
- Bazel REAPI conformance passa 100%.
- Cache hit ratio measurable.
- HKDF property test 10k green.
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido — 6 colunas)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Merkle tree invalid detected late** (após persist) | M | M | HIGH (stale AC entries) | M | LOW | Dual-side verify (server pre-persist + client post-download); pre-persist reject invalid; INV-AC-OUTPUTS-VALID daily reconcile. |
| **AC TTL too aggressive** → hit ratio baixo | M | L | MEDIUM (customer perception) | L | LOW | Defaults per-tier ADR-0019; refresh-on-hit; customer-visible métrica; tunable via admin S-13. |
| **AC tenant leak via bug de digest computation** | L | M | CRITICAL (FM-303) | M | LOW | INV-AC-TENANT-SCOPED TLA+ + property test 100k + tenant prefix derivation pattern; RB-FM-303. |
| **REAPI v2 spec interpretation divergente de Bazel** | M | M | MEDIUM (rework) | M | LOW | Conformance suite bazelbuild/remote-apis CI; quarterly re-run; community engagement. |
| **HKDF info string drift** (typo "ac-sig" → "acsig") | L | L | HIGH (signature mismatch global) | L | LOW | Constant em código; CI test verifica; ADR-0021 documented. |
| **Cache hit ratio < 70% em workload real** | M | L | MEDIUM (DX miss) | L | LOW | Workload tuning + DX research; customer dashboard surface; iterate post-launch. |
| **Outputs referenciados deletados via GC** (race S-06) | M | M | HIGH (INV-AC-OUTPUTS-VALID violated) | M | LOW | INV-GC-004 mark-phase-aware re-ref protection (S-06); reconcile diário detect; auto-fix. |
| **REAPI gRPC vs REST surface drift** | M | L | LOW | L | LOW | Single source proto; codec generated; conformance suite cobre. |

## 16. Benchmarks SOTA externos

| Critério | BuildBuddy | NativeLink | bazel-remote | **CoreLink target S-04** |
|---|---|---|---|---|
| REAPI v2 conformance | 95% | 80% | 90% | **100% suite green** |
| Merkle verify dual-side | Server only | Server only | Server only | **Yes — server + client** |
| Cache hit ratio business métrica | Yes (UI) | No | No | **Yes — DASH-AC + customer dashboard** |
| TLA+ tenant isolation AC | No | No | No | **Yes — derivada INV-AC-TENANT-SCOPED** |
| AC GetActionResult p99 | 200-400ms | 150-300ms | 100-200ms | **≤ 150ms warm** |
| TTL infrastructure tunable | Manual | Manual | Manual | **Yes — config-singleton S-13** |
| HKDF tenant signing | No | No | No | **Yes — CTRL-AC-002** |

## 17. References (RFCs, papers, standards)

- **REAPI v2 Specification** <https://github.com/bazelbuild/remote-apis>.
- **bazelbuild/remote-apis conformance test suite**.
- **NIST SP 800-185** — KMAC and SHA-3 (Merkle alignment).
- **NIST SP 800-108** — Key Derivation (HKDF).
- **Bazel ActionCache proto** <https://github.com/bazelbuild/remote-apis/blob/main/build/bazel/remote/execution/v2/remote_execution.proto>.
- `specs/03_architecture/error_taxonomy.md` — `COR_AC_*` error codes.
- ADR-0019 (TTL ownership S-04↔S-07).

## 18. Post-mortem hooks

- Cross-tenant AC leak detected → CRITICAL post-mortem + Privacy + breach notification consideration.
- Merkle invalid persisted (escape from dual-side verify) → CRITICAL post-mortem + verifier code review.
- INV-AC-OUTPUTS-VALID violation > 5 records → 5-Why obrigatório + GC interaction review.
- REAPI conformance regression → post-mortem + Bazel community engagement.
- Cache hit ratio < 50% sustained 7d → post-mortem (DX) + workload analysis.

## 19. Waiver policy

S-04 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ INV-AC-TENANT-SCOPED property test 100k green — security baseline.
- ❌ Merkle verify dual-side — INV-CAS-INTEGRITY alignment.
- ❌ REAPI v2 conformance suite passa **100%** — DX baseline (Lote 10.4bis tightening; HIGH_RISK GA não deve shippar com 5% conformance gap; emergency bumps via ADR forward).
- ❌ HKDF digest signing CTRL-AC-002 — security baseline.

Itens waivable com Architect + Crypto SME + ADR:

- ⚠️ Cache hit ratio target 70% → 50% com plan to improve next sprint.
- ⚠️ AC GetActionResult p99 150ms → 200ms com customer SLA addendum.

**Lote 10.4bis amendment**: 95%-com-waiver path para conformance REMOVIDO (era ⚠️ no v1.1.0). HIGH_RISK GA exige 100% conformance non-negotiable. Emergency Bazel security release (CVE-driven) bypass cadence quarterly via Architect approval + ADR within 1 sprint (vide WI-S04-006 §6.1.10).

## 20. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | Gustavo | Initial sprint contract S-04 (Action Cache + HKDF signing; HIGH_RISK 11 sign-offs canonical). |
| 1.1.0 | 2026-04-25 | Gustavo | Lote 10.4bis P0 fixes (Agent R4 review remediation): HKDF info bytes domain separation; 95%-with-waiver REMOVED; INV §3.15 promotion; ADR canonical path adrs/; TenantCtx-only; audit fail-closed; 4-tier P0/P1/P2/P3. |
| 1.2.0 | 2026-04-25 | Gustavo | **Lote 10.4-tris fixes** (Sonnet R5 independent review; 6 NEW P0s + 11 P1s + ADR creation): (a) **P0-R5-001** key_id=0 reserved sentinel + `SigError::KeyIdReserved`; (b) **P0-R5-002** HKDF-Extract composition with path-HMAC TDK documented in ADR-0021 §Risks (Option B accepted under HMAC security; attack cost 2^128); (c) **P0-R5-003** canonical_bytes 121 bytes retained; result_hash = BLAKE3(merkle_root).hex() D1 INDEX column ONLY (NOT cripto binding); ADR-0037; (d) **P0-R5-004** Gherkin BatchUpdateActionResult struck; (e) **P0-R5-005** Crypto SME mandatory (WI-004 SEAL non-waivable) vs advisory (WI-006 PRR ceremony) split explicit em BOTH WIs + ADR-0034; (f) **P0-R5-006** §30.1 key rotation procedures `sig_key_id` + `path_key_id` independent lifecycles; (g) **P1-R5-016** ADRs 0021/0034/0035/0036/0037 stubs created. |

---

**Fim spec contract S-04 v1.2.0 SOTA.**
