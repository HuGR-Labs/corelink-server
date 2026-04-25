---
id: "S-04"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "SECURITY-MODEL"
  - "DATA-MODEL"
  - "INVARIANT-REGISTRY"
  - "KEY-MANAGEMENT"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
  - "RESILIENCE-PATTERNS"
tags: ["sprint", "s04", "action-cache", "reapi", "merkle", "high-risk"]
---

# Sprint S-04 — Action Cache (REAPI GetActionResult/UpdateActionResult + Merkle dual-side + TTL)

> **doc_status:** DRAFT · **lane:** HIGH_RISK · **Versão:** 1.0.0 · **2026-04-25**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (Lote 9.4 SOTA)

> **🚦 Phase boundary:** Fase 1 — Remote Cache.

---

## 1. Objetivo

Implementar Action Cache (AC) — "cereja" do remote cache Bazel/Buck2: dado action_digest determinístico, retorna ActionResult previamente computado. Requer integridade Merkle dual-side (server reject + client verify), tenant-scoped strict (FM-303 catastrophic), TTL infrastructure (defaults per-tier delegated S-07/ADR-0019). Sem AC, CoreLink é só blob store.

## 2. Escopo

### 2.1 In-scope

- **WI-S04-001**: REAPI ActionCache proto handlers (GetActionResult + UpdateActionResult).
- **WI-S04-002**: D1 schema `ac_meta` + R2 bucket `ac-<region>` + UNIQUE constraint.
- **WI-S04-003**: Crate `corelink-ac` (Merkle dual-side codec + verifier) + INV-AC-OUTPUTS-VALID enforcement.
- **WI-S04-004**: CTRL-AC-002 digest signing HKDF + ADR-0021.
- **WI-S04-005**: TTL worker + S-07 boundary alignment ADR-0019.
- **WI-S04-006**: REAPI conformance tests + DASH-AC dashboards + RB-FM-303 dry-run + PRR.

### 2.2 Anti-scope

- ❌ Execute Action (Fase 2 Remote Execution).
- ❌ AC cross-tenant dedup (FM-303 risk; bloqueado at GA).
- ❌ AC CDN layer (S-14 multi-region).
- ❌ AC TTL defaults per-tier (delegated S-07 / ADR-0019).
- ❌ Ed25519 digest signing (rejected; HKDF é suficiente; ADR-0021).

## 3. Customer Impact & Journey

**JTBD:** "Como dev de CI Bazel/Buck2, quero cache de build artifacts (ActionResult) com hit ratio > 70% e Merkle integrity garantida — sem que builds de outros tenants vazem para meu workspace."

**CAPs entregues:** CAP-AC-001..006 (GetActionResult + UpdateActionResult + Merkle dual-side + TTL infra + invalidation + cache hit ratio business métrica).

## 4. Capability Mapping

Ver `_spec_contract.md §4`. INV-AC-TENANT-SCOPED deriva de INV-TENANT-ISOLATION TLA+.

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S04-D1 | REAPI ActionCache handlers | `crates/corelink-worker/src/reapi/ac.rs` | gRPC + REST surface; auth middleware reuse S-03 |
| S04-D2 | D1 ac_meta + R2 ac bucket | `migrations/003_ac.sql` | UNIQUE `(tenant_id, action_digest)`; integration test |
| S04-D3 | Crate corelink-ac (Merkle dual-side) | `crates/corelink-ac/` | Builder/verifier; pre-persist outputs check; criterion benchmark p99 ≤ 10ms |
| S04-D4 | HKDF digest signing + ADR-0021 | `crates/corelink-ac/src/sign.rs` + `specs/03_architecture/adrs/ADR-0021-...md` | info="ac-sig" fixo; property test 10k |
| S04-D5 | TTL worker + S-07 boundary | `crates/corelink-worker/src/ac/ttl.rs` | Refresh-on-hit; defaults per-tier via S-07 ADR-0019 |
| S04-D6 | REAPI conformance + dashboards + RB + PRR | bazelbuild/remote-apis suite | 100% AC ops pass; DASH-AC live; RB-FM-303 dry-run; PRR 11 sign-offs |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Storage

- D1 schema `ac_meta` per `data_model.md §4.2`; UNIQUE `(tenant_id, action_digest)`.
- R2 bucket `ac-<region>` per region.

### 6.2 Auth + Crypto

- HKDF tenant key `info="ac-sig"` (CTRL-AC-002).
- TDK reused S-01.
- ADR-0021 documenta HKDF vs Ed25519 decision (HKDF preferred — symmetric + faster + sufficient for integrity binding tenant-scoped).

### 6.3 Invariants

- INV-AC-OUTPUTS-VALID (HIGH): outputs referenciados existem em blob_meta alive; reconcile diário S-06 GC.
- INV-AC-TENANT-SCOPED (CRITICAL, TLA+): deriva de INV-TENANT-ISOLATION.
- INV-CAS-IMMUTABILITY (CRITICAL): AC entry imutável pós-write.

### 6.4 SLOs

- SLO-AVAIL-AC ≥ 99.9%.
- SLO-LAT-AC-HIT p99 ≤ 150ms.

## 7. Definition of Done (HIGH_RISK)

- [ ] 6 WIs SEALED (EVT-031).
- [ ] E2E Bazel build com `--remote_cache=corelink://...` → cache hit > 50% time saving (EVT-018).
- [ ] Property test 100k AC tenant isolation (EVT-002).
- [ ] TLA+ tenant_isolation.tla green pós-integração (EVT-022).
- [ ] Chaos: TTL expiry sob load + stale read handling (EVT-023).
- [ ] SLO-AVAIL-AC + SLO-LAT-AC-HIT sustained 72h staging (EVT-021).
- [ ] PRR HIGH_RISK 11 sign-offs (EVT-031).
- [ ] REAPI v2 conformance (bazelbuild/remote-apis) AC ops 100% pass (EVT-002 + EVT-018).
- [ ] Merkle invalid tree rejected pre-persist + post-download client (EVT-002).
- [ ] Cache hit ratio > 70% workload sintético Bazel (EVT-021).
- [ ] ADR-0021 ratificado (EVT-027).
- [ ] Cost regression gate § 14.10 universal (EVT-002).
- [ ] RB-FM-303 (AC cross-tenant) dry-run (EVT-017).

## 8. Dependencies

### Hard blockers

- **S-01 SEALED** (CAS write).
- **S-02 SEALED** (CAS read; output blobs referenciados).
- **S-03 SEALED** (auth tenant ctx).

### Outbound

- S-07 (eviction TTL defaults supersede CAP-AC-004 — ADR-0019).
- S-13 (admin invalidation override).
- S-15 (CLI/SDK consume AC API).
- S-20 (GA exige cache hit ratio sustained).

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-03 SEALED).
- **Mid-check**: D+8.
- **Sprint close**: D+15 (3 semanas + 5 dias buffer).

## 10. Risk Register

Ver `_spec_contract.md §15` (8 risks 6-col).

## 11. Observability Plan

DASH-AC com cache hit ratio per (tenant_tier, region); customer-visible em S-16.

## 12. Security & Privacy

STRIDE: AC tenant isolation (INV-AC-TENANT-SCOPED) é THE primary threat — mitigated via TLA+ + property test + 5-layer defense.

## 13. Post-mortem hooks

Cross-tenant AC leak / Merkle invalid persisted / INV-AC-OUTPUTS-VALID violation > 5 records / REAPI conformance regression / cache hit ratio < 50% sustained.

## 14. Sign-off (HIGH_RISK 10–12)

11 roles incl. Crypto SME (HKDF review) + AppSec.

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-04 (Lote 9.5b). |

---

**Fim de S-04 sprint contract.**
