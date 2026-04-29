---
id: "SPEC-CONTRACT-S14"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-04-24"
updated: "2026-04-29"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s14", "region", "failover", "byok", "kms", "fips-140-3", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-14: Region Expansion + Cross-Region Failover + BYOK (Enterprise Tier)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-14 |
| Nome | Region Expansion + BYOK |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-002 (cross-region tenant isolation), FF-HR-003 (residency PII), FF-HR-005 (BYOK introduces crypto controls), FF-HR-008 (vendor lock-in via multi-cloud KMS), FF-HR-009 (DPA enterprise customer-facing contract) |
| Duração estimada | 4 semanas |
| WIs antecipados | 9 |
| SOTA target | Multi-region production + BYOK enterprise tier — 4 regiões verde + read failover transparent SLO + 4 KMS providers (AWS/GCP/Azure/Vault) com FIPS 140-3 (AWS KMS L1 + Vault Enterprise L1) / 140-2 (GCP KMS L1, Azure Key Vault Premium L2) documented per provider em compliance/byok-fips-matrix.md + customer kill switch ≤ 6 min p99 (60s detection + 5min DEK cache TTL hard) |

## 1. Objetivo

Expandir CoreLink para **multi-region production-grade** (4 regiões: WNAM/ENAM/WEUR/SAM; APAC/AFR deferred) + **BYOK enterprise tier** com 4 KMS providers (AWS KMS / GCP KMS / Azure Key Vault / HashiCorp Vault), region read failover transparent (PAT-REGION-FAILOVER-001), customer kill switch (revoke CMK → cache inacessível ≤ 5 min), erasure attestation signed Ed25519 (NIST SP 800-88 Rev.1 compliant). Critical pra **enterprise audience** (FedRAMP-ready customers + EU data residency + crypto sovereignty).

**Por que SOTA:** competitors offer either multi-region OR BYOK, raramente ambos com rigor formal. CoreLink S-14 entrega: (a) BYOK FIPS doc per provider em `compliance/byok-fips-matrix.md` (140-3 onde available — AWS KMS L1, Vault Enterprise L1; 140-2 transitional onde 140-3 not yet certified — GCP KMS L1, Azure Key Vault Premium L2); (b) crypto-erase via key destroy NIST compliant (S-11 alignment); (c) DPA amendment template para residency contract; (d) Schrems II TIA template. Reference: **NIST SP 800-57 Pt 1 Rev 5** (key management), **NIST SP 800-130** (cryptographic key management framework).

**Decomposição vs codex finding scope-overstuffed:** S-14 mantém 9 WIs (originalmente 8 + 1 split BYOK adapter trait); região coordinator + failover separado de BYOK; BYOK 4 providers em 2 WIs (AWS isolado high-priority + GCP/Azure/Vault batch).

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (10–12 sign-offs).
- **FF-HR-002**: cross-region keys/queries podem vazar tenant data; 0-tolerance.
- **FF-HR-003**: residency EU é regulatory absoluto (GDPR + Schrems II).
- **FF-HR-005**: BYOK introduz CTRL-KEY-010..012 (customer-controlled crypto).
- **FF-HR-008**: multi-cloud BYOK multi-vendor lock-in risk; matrix testing required.
- **FF-HR-009**: DPA amendment customer-facing contract; legal exposure.

## 3. Inherits_from

```yaml
inherits_from:
  - "KEY-MANAGEMENT"            # rotation + overlap + CTRL-KEY-010..015
  - "SECURITY-MODEL"            # CTRL-CRYPTO-005 (BYOK), CTRL-KEY-015 (Ed25519 attestation)
  - "PRIVACY-MODEL"             # CTRL-PRIV-031 (residency)
  - "STORAGE-SEMANTICS-MATRIX"  # multi-region storage model
  - "COMPLIANCE-MATRIX"         # SOC 2 CC6.1 + GDPR + Schrems II
  - "RESILIENCE-PATTERNS"       # PAT-REGION-FAILOVER-001
  - "OBSERVABILITY-MODEL"       # SLO-LAT-CAS-GET cross-region
  - "FAILURE-MODES"             # FM-054 (KV global leak), FM-105 (replication diverge)
  - "SLO-CATALOG"               # cross-region SLO targets
  - "INVARIANT-REGISTRY"        # INV-DATA-RESIDENCY, INV-KEY-NO-SKIP
  - "AUTH-MODEL"                # tenant region context propagação
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-REGION-001** | 4 regiões operantes | WNAM (us-west), ENAM (us-east), WEUR (eu-west), SAM (sa-east). |
| **CAP-REGION-002** | Tenant pinning por região + cross-region restrict | Insert checks; 20k property test cross-region 0 leaks. |
| **CAP-REGION-003** | Read failover hot blobs | PAT-REGION-FAILOVER-001; top 1% hot blobs replicated; SLO preserved durante region outage chaos. |
| **CAP-BYOK-001** | BYOK AWS KMS integration | First-class; FIPS 140-3 verified; envelope encryption (DEK wrapped por CMK). |
| **CAP-BYOK-002** | BYOK GCP KMS integration | KMS API; FIPS 140-2 verified (140-3 quando GCP suportar). |
| **CAP-BYOK-003** | BYOK Azure Key Vault integration | HSM-backed (Premium tier); FIPS 140-2 Level 2. |
| **CAP-BYOK-004** | BYOK HashiCorp Vault integration | Customer-hosted; mTLS auth; transit secrets engine. |
| **CAP-BYOK-005** | Customer kill switch | Revoke CMK access → cache 503 ≤ 5 min; INV-KEY-NO-SKIP enforcement. |
| **CAP-BYOK-006** | Erasure attestation Ed25519-signed | NIST SP 800-88 Rev.1 compliant; signed proof of erasure delivered to customer + retained 7y. |
| **CAP-REGION-004** | DPA amendment + Schrems II TIA | Legal templates for enterprise contract closure. |

## 5. Requirements específicos

### 5.1 Region (CAP-REGION-001..003 + CAP-REGION-004)

- **R-S14-1**: Infrastructure: R2 buckets + D1 instances + DO storage em 4 regiões; provisioning via Terraform module `corelink-region` reusable.
- **R-S14-2**: Tenant `primary_region` enforcement no write path:
  - Insert check: `tenant.region == request.region`; mismatch = 403 + audit emission.
  - DO `region_enforcer` validates per-request.
- **R-S14-3**: Hot blob replica detector (top 1% identified via offline aggregation, **NÃO** via métrica labeled por `tenant_id` — proibido por INV-OBS-CARDINALITY-BUDGET S-09). Strategy: (a) métrica agregada `corelink.cas.get.bytes_total{tenant_tier, region}` para budget-safe live monitoring; (b) offline daily job em audit log R2 (S-09 audit bucket) computa top-1% per tenant via batch query — escapa cardinality budget porque é offline. Replication worker copia para sibling region async; lag p99 ≤ 60s. **Lote 9.4 Opus H-02 fix.**
- **R-S14-4**: PAT-REGION-FAILOVER-001 read failover: se primary region 503/504 → read from secondary; SLO preserved transparently.
- **R-S14-5**: DPA amendment template em `legal/dpa-residency-amendment.md` covering residency commitment per region; Schrems II TIA template em `legal/tia-template.md`.

### 5.2 BYOK (CAP-BYOK-001..006)

- **R-S14-6**: BYOK adapter trait `crates/corelink-byok` (Rust):
  - `trait KmsProvider { fn wrap_dek(&self, dek: &[u8]) -> Result<WrappedDek>; fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek>; fn check_access(&self) -> Result<KmsAccessStatus>; }`.
  - 4 implementations: `AwsKmsProvider`, `GcpKmsProvider`, `AzureKeyVaultProvider`, `VaultProvider`.
- **R-S14-7**: Envelope encryption flow:
  - Write: gen ephemeral **DEK random 32 bytes via CSPRNG** (`getrandom::getrandom`; NOT BLAKE3-derived per Lote 10.14 codex P1 fix — deterministic DEK = compromise propagation across blobs same hash) → encrypt body AES-256-GCM (FIPS 197 + FIPS 140-3 approved) com DEK + nonce 96-bit random → wrap DEK via KMS → store wrapped DEK em D1 + body em R2.
  - Read: fetch wrapped DEK → unwrap via KMS → decrypt body.
- **R-S14-8**: CMK revocation detection: KMS access check em background (every 60s); revoked → degrade tenant read-only + emit `corelink.byok.cmk_revoked` audit + alert customer.
- **R-S14-9**: Customer kill switch: revoke CMK → cache 503 ≤ 5 min globally; chaos test verified.
- **R-S14-10**: Erasure attestation Ed25519:
  - Per-DSR-erasure (S-11) of BYOK tenant: destroy CMK access → emit signed attestation `{tenant_id, request_id, destroyed_ts, kms_provider, evidence_hash, signature_ed25519}`.
  - Retained 7y em R2 audit bucket; deliverable to customer + auditor.

### 5.3 Compliance (CAP-REGION-004)

- **R-S14-11**: FIPS 140-3 verification per KMS provider:
  - AWS KMS: FIPS 140-3 Level 1 default; document compliance.
  - GCP KMS: FIPS 140-2 (3 quando suportado); document.
  - Azure: FIPS 140-2 Level 2 (Premium HSM); document.
  - Vault: FIPS 140-3 Level 1 (Vault Enterprise); document.
- **R-S14-12**: Multi-cloud BYOK matrix test (`tests/byok_matrix_test.rs`): 16 combinations (4 providers × write/read/wrap/unwrap) tested em staging.

## 6. Definition of Done

> **Two-phase SEAL** (per timeline): items verificáveis instantaneamente fecham em **Implementation SEAL D+30** (libera downstream S-15/S-16/S-17/S-19 development); items requerendo "sustained 30d staging" (4 regions stable, BYOK matrix test weekly green sustained, kill switch chaos drill weekly, replication lag p99 sustained 7d, DPA signed lighthouse customer) fecham em **GA Evidence Gate SEAL D+60** (libera GA promotion S-20). Ambos SEALs canonicos; sprint considerado concluído apenas após GA Evidence Gate D+60. Lote 10.14 codex P1 alignment.

- [ ] **WIs SEALED**: 9/9 *(Implementation SEAL D+30)*.
- [ ] **4 regiões live** + chaos test region outage cada região (EVT-023).
- [ ] **Tenant EU** property test: blob lands em WEUR, nunca ENAM (20k test) (EVT-002).
- [ ] **BYOK AWS KMS E2E**: customer CMK → wrap DEK → CAS write → read decrypt sucesso (EVT-024).
- [ ] **BYOK 4 providers matrix test** verde em staging (16 combinations) (EVT-002).
- [ ] **Kill switch test (Lote 10.14 codex P0 canonical disambiguation)**: revoke CMK → detection p99 ≤ 60s (KMS access check interval) + DEK cache TTL hard ≤ 5 min → **customer-perceived global cache 503 p99 ≤ 6 min**; chaos drill weekly D+30..D+60 (EVT-023). Componentes individuais hard-fail (não waiverable per §19); SLA total realistic é 6 min p99 (não 5 min).
- [ ] **SLO sustained** during region failover chaos: SLO-LAT-CAS-GET p99 < 300ms preserved (EVT-021).
- [ ] **Replication lag** p99 ≤ 60s para hot blobs sustained 7d staging (EVT-021).
- [ ] **Residency DPA amendment** drafted + reviewed por Legal externo (EVT-044).
- [ ] **Schrems II TIA template** drafted (EVT-046).
- [ ] **Erasure attestation Ed25519** test: BYOK tenant DSR erasure → signed attestation generated + verifiable (EVT-024 + EVT-042).
- [ ] **FIPS 140-3 / 140-2 compliance documented** per provider em `compliance/byok-fips-matrix.md`.
- [ ] **PRR HIGH_RISK**: Security lead + Crypto SME + SRE + Privacy officer + Legal + Compliance officer + Engineer + QA + Product + 2 peers + AppSec.
- [ ] **External pentest BYOK flow** (1 provider AWS) — clean (EVT-040).
- [ ] **Runbook dry-run**: RB-BYOK-REVOKE + RB-FM-054 (KV stale cross-region) + RB-FM-105 (region replication diverge if exists; create stub if not) (EVT-017).

## 7. Completeness Criteria (delta local)

- [ ] **10.s14.1** Replication lag < 60s p99 para hot blobs sustained 7d staging.
- [ ] **10.s14.2** Residency contract: DPA amendment + Schrems II TIA template Legal-reviewed (EVT-046 + EVT-044).
- [ ] **10.s14.3** **Cross-region tenant isolation 0 leaks** em 30k property test.
- [ ] **10.s14.4** **BYOK 4 providers FIPS-verified** + matrix test verde.
- [ ] **10.s14.5** **Kill switch ≤ 5 min** sustained 30d staging chaos drill weekly.
- [ ] **10.s14.6** **Erasure attestation** Ed25519 verifiable post-facto via public key.
- [ ] **10.s14.7** **External pentest BYOK** clean — zero CRITICAL findings.
- [ ] **10.s14.8** **DPA amendment signed** com 1 enterprise customer beta (EVT-044).

## 8. Invariants

### Mantidas

- **INV-DATA-RESIDENCY** (HIGH — herda S-11): tenant region pinned + enforced cross-region.
- **INV-TENANT-ISOLATION** (CRITICAL): mantém cross-region; TLA+ verifica.
- **INV-KEY-NO-SKIP** (HIGH — invariant_registry §3.13): BYOK revoke → writes fail corretamente.
- **INV-KEY-OVERLAP** (HIGH — invariant_registry §3.13 + key_management §3.2.1 + ADR-0018): rotation overlap respected per asset class — Ed25519 attestation key 30d, BYOK CMK 7d.

### Novas (introduzidas por S-14 — adicionar a invariant_registry.md §3.12)

- **INV-BYOK-CRYPTO-SOVEREIGNTY** (CRITICAL — novo): customer revoga CMK → cache inacessível em ≤ 5 min global; nenhum bypass via cached unwrapped DEK > 5 min. **Why:** sem isso BYOK = teatro; customer não tem real control. **How to apply:** DEK cache TTL 5 min hard + KMS access check 60s.
- **INV-REGION-NO-CROSS-LEAK** (CRITICAL — novo): blob/AC/billing tagged com region; cross-region read = 403 + audit. **Why:** Schrems II + LGPD Art. 33; gap = catastrophic legal. **How to apply:** insert checks + property test 30k.
- **INV-ERASURE-ATTESTATION-SIGNED** (HIGH — novo): erasure de BYOK tenant produz attestation Ed25519-signed verifiable. **Why:** customer + auditor exigem proof. **How to apply:** per-erasure attestation generation + 7y retention.

## 9. Quality Standards (delta local)

- **14.s14.1 Region failover transparent ao cliente** (SLO preserved); fallback latency overhead < 50ms p99.
- **14.s14.2 BYOK latency overhead < 30ms p99** (AWS KMS region-co-located baseline); cache DEK 5 min hard limit.
- **14.s14.3 Multi-cloud BYOK matrix tested** em staging weekly; auto-fail PR se matrix break.
- **14.s14.4 FIPS compliance documented** per provider; quarterly review com Crypto SME.
- **14.s14.5 Kill switch SLA ≤ 5 min** com chaos drill weekly.
- **14.s14.6 DEK cache eviction** atomic; revocation propagates ≤ 5 min via subscribe-pub.
- **14.s14.7 Erasure attestation** assinada com per-region Ed25519 key; key rotation overlap 30d (canonical: `key_management.md §3.2.1` + ADR-0018).
- **14.s14.8 Cost regression gate**: BYOK adds < 15% overhead em CAS path; matched em benchmark CI.

## 10. Anti-scope

- ❌ APAC/AFR regions (pós-GA demand-driven; 6+ months pós-GA).
- ❌ Active-active multi-region writes (primary-only em S-14; active-active = Fase 2).
- ❌ BYOE (Bring Your Own Encryption) — Fase 2.
- ❌ Customer-managed HSM on-prem (only cloud KMS at GA).
- ❌ Quantum-resistant crypto (post-quantum migration is Fase 3).
- ❌ Per-tenant region migration (lock at signup; migration = manual ticket).
- ❌ Federated KMS (multi-cloud failover) — single CMK per tenant at GA.
- ❌ Cross-region replication for non-hot blobs — top 1% only.

## 11. Dependencies

### Hard blockers

- **S-01..S-10 SEALED** (core product working).
- **S-11 SEALED** (residency tooling + DSR for crypto-erase integration).
- **S-13 SEALED** (admin plane para config flags per-region + secret rotation framework BYOK).
- **S-12 SEALED** (signed deploy critical para production multi-region).

### Soft blockers

- **S-09 SEALED** (observability cross-region + métricas).

### Outbound

- S-15 (CLI/SDK exposes BYOK config).
- S-16 (admin UI BYOK setup wizard).
- S-19 (enterprise onboarding com BYOK + DPA flow).
- S-20 (GA exige 4 regions stable + BYOK 4 providers + DPA signed lighthouse customers).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S14-001** | R2+D1+DO provisioning 4 regions + Terraform module + migration | terraform module; 4 regions provisioning; data migration script; runbook | 18h | 28h | 44h | **28.7h** |
| **WI-S14-002** | Tenant region pinning enforcement + 30k property test | insert checks; DO region_enforcer; property test; runbook RB-region-leak | 12h | 18h | 28h | **18.7h** |
| **WI-S14-003** | Hot blob replica worker (PAT-REGION-FAILOVER-001) + read failover | hot blob detector **OFFLINE batch aggregation** sobre S-09 audit log R2 (per Lote 9.4 Opus H-02 canonical fix; **NÃO** via live métrica labeled by `tenant_id` proibido por INV-OBS-CARDINALITY-BUDGET); replica worker async; failover routing; chaos test | 16h | 24h | 36h | **24.7h** |
| **WI-S14-004** | BYOK adapter trait + AWS KMS adapter (first-class) + matrix test framework | trait design; AWS adapter; envelope encryption; matrix test framework; FIPS doc | 18h | 28h | 44h | **28.7h** |
| **WI-S14-005** | BYOK GCP/Azure/Vault adapters + 16-combination matrix test | 3 adapters; matrix test 16 combinations; staging deploy; FIPS doc per provider | 20h | 32h | 50h | **32.7h** |
| **WI-S14-006** | CMK revocation detection + customer kill switch ≤ 5 min + chaos drill | KMS access check 60s; cache TTL 5 min; revoke flow; chaos test; runbook RB-BYOK-REVOKE | 12h | 18h | 28h | **18.7h** |
| **WI-S14-007** | Erasure attestation Ed25519 signing + 7y retention + verify endpoint | per-region Ed25519 key; signing; storage R2 7y; verify endpoint; NIST SP 800-88 evidence | 10h | 16h | 26h | **16.7h** |
| **WI-S14-008** | DPA amendment + Schrems II TIA template + Legal review + 1 customer signed | DPA template; TIA template; Legal externo review; lighthouse customer test | 12h | 20h | 32h | **20.7h** |
| **WI-S14-009** | TLA+ region_residency.tla + RB-BYOK-REVOKE dry-run + External pentest BYOK | TLA+ spec residency; CI integration; pentest engagement; remediation | 16h | 24h | 38h | **25.0h** |

**Total PERT:** ~234h ≈ 29 dias work × 1 eng. Buffer 10 dias confere com 4 semanas (multi-cloud unpredictability).

## 13. Duração + Timeline

- **Duração:** 4 semanas (20 dias úteis) + buffer 10 dias.
- **Marcos:**
  - **D+5:** WI-001 SEALED (4 regions live).
  - **D+8:** WI-002 + WI-003 SEALED (region pinning + failover).
  - **D+13:** WI-004 SEALED (AWS KMS + matrix framework).
  - **D+18:** WI-005 SEALED (GCP/Azure/Vault).
  - **D+21:** WI-006 + WI-007 SEALED (kill switch + erasure attestation).
  - **D+25:** WI-008 + WI-009 SEALED (DPA + TLA+ + pentest start).
  - **D+30:** Sprint review + sign-offs + pentest results integrated.

## 14. Critérios de promoção

- DoD complete + 30d staging com 4 regions + BYOK 4 providers stable.
- External pentest BYOK clean.
- DPA signed com 1 enterprise customer beta.
- TLA+ region_residency verde.
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **BYOK API drift entre clouds** (provider API change) | H | M | MEDIUM | H | LOW | Adapter trait isolation + matrix test weekly + version-pin SDKs + ADR per provider. |
| **Residency leak via KV global** (FM-054) | M | M | HIGH | M | LOW | INV-REGION-NO-CROSS-LEAK + KV namespace per-region + property test 30k + runbook. |
| **Region replication diverge** (FM-105) | M | M | MEDIUM | M | LOW | INV-CAS-INTEGRITY (hash check post-replica) + reconciliation + alerts. |
| **Customer CMK-off causa false SEV-1** | M | L | LOW (expected behavior) | L | LOW | Métrica `byok_revoked_total` distinguishes from real outage; runbook clarifies. |
| **DEK cache TTL bypass** (revocation não propaga) | L | M | CRITICAL | M | LOW | Cache TTL 5 min hard + subscribe-pub revocation event + property test. |
| **KMS provider outage** (AWS KMS down) | L | M | HIGH | L | LOW | DEK cache 5 min absorvve outage; degrade-mode read-only se sustained > 5 min. |
| **FIPS compliance drift** (provider muda level) | L | M | HIGH (compliance) | L | LOW | Quarterly review + alert se NIST CMVP module status changes. |
| **Schrems II legal landscape change** (TIA template invalida) | M | M | HIGH | M | LOW | Quarterly Legal review + EDPB guidelines monitoring. |
| **Erasure attestation forge attempt** | L | H | CRITICAL | M | LOW | Ed25519 signing per-region + 7y retention + audit chain + verify endpoint. |
| **DPA template legal challenge** (customer disputes) | M | M | HIGH (revenue) | M | LOW | Legal externo review + 1 lighthouse customer signed before broad rollout. |
| **External pentest finds CRITICAL em BYOK** | M | M | HIGH (delay GA) | M | MEDIUM | Hire reputable firm (Schellman/A-LIGN); buffer 10 dias para remediation. |
| **Multi-region replication storm** (top 1% becomes 30%) | L | M | MEDIUM (cost) | L | LOW | Cardinality detector + budget alert; manual override possible. |
| **Region failover false-positive** (slowness misdetected as outage) | M | L | LOW | L | LOW | Multi-signal detection (5xx + latency + reachability); runbook documents. |

## 16. Benchmarks SOTA externos

| Critério | AWS S3 + KMS | GCS + Cloud KMS | Azure Blob + Key Vault | NativeLink | **CoreLink target S-14** |
|---|---|---|---|---|---|
| Multi-region (4+ regions) | Yes | Yes | Yes | No | **Yes — 4 regions WNAM/ENAM/WEUR/SAM** |
| BYOK customer-controlled CMK | Yes | Yes | Yes | No | **Yes — 4 providers AWS/GCP/Azure/Vault** |
| FIPS 140-3 compliance | Yes (140-3) | 140-2 | 140-2 L2 | No | **Yes — documented per provider** |
| Customer kill switch ≤ 5 min | Yes (KMS revoke) | Yes | Yes | No | **Yes — INV-BYOK-CRYPTO-SOVEREIGNTY** |
| Cross-region read failover transparent | Yes (CRR) | Yes | Yes | No | **Yes — PAT-REGION-FAILOVER-001 hot 1%** |
| Erasure attestation signed | No | No | No | No | **Yes — Ed25519 + NIST SP 800-88 Rev.1** |
| TLA+ region residency | No | No | No | No | **Yes — region_residency.tla** |
| DPA amendment template | Standard | Standard | Standard | None | **Yes — Schrems II TIA included** |

**Veredito SOTA:** S-14 v1.1 atinge feature parity AWS/GCP/Azure em multi-region + BYOK; vantagem em erasure attestation Ed25519 + TLA+ verified residency.

## 17. References (RFCs, papers, standards)

- **NIST SP 800-57 Pt 1 Rev 5** — Recommendation for Key Management.
- **NIST SP 800-130** — Cryptographic Key Management Framework.
- **NIST SP 800-88 Rev.1** — Guidelines for Media Sanitization (crypto-erase).
- **FIPS 140-3** — Security Requirements for Cryptographic Modules.
- **FIPS 140-2** (legacy, sunset) — para providers not yet 140-3 certified.
- **NIST FIPS 197** — AES specification.
- **NIST FIPS 186-5** — Digital Signature Standard (Ed25519).
- **CSA Cloud Controls Matrix v4** — multi-cloud security baseline.
- **EDPB Recommendations 01/2020** — supplementary measures for international transfers.
- **AWS KMS Best Practices** <https://docs.aws.amazon.com/kms/latest/developerguide/best-practices.html>.
- **GCP Cloud KMS Documentation** <https://cloud.google.com/kms/docs>.
- **Azure Key Vault Security** <https://learn.microsoft.com/en-us/azure/key-vault/general/security-features>.
- **HashiCorp Vault Transit Engine** <https://developer.hashicorp.com/vault/docs/secrets/transit>.

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Cross-region tenant data leak → CRITICAL post-mortem + breach notification consideration (Schrems II).
- BYOK kill switch SLA miss (> 5 min) → CRITICAL post-mortem + Crypto SME + Compliance.
- DEK cache TTL bypass detected → CRITICAL post-mortem + INV-BYOK-CRYPTO-SOVEREIGNTY review.
- KMS provider outage > 30 min → post-mortem + provider escalation + PAT review.
- Region replication diverge unrecovered > 1h → post-mortem + INV-CAS-INTEGRITY review.
- Erasure attestation forge detected → CRITICAL post-mortem + Security incident.
- DPA legal challenge → post-mortem com Legal + customer trust review.

## 19. Waiver policy

S-14 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ INV-BYOK-CRYPTO-SOVEREIGNTY kill switch SLA non-waivable (Lote 10.14 codex P0 canonical disambiguation): **detection ≤ 60s** (KMS access check interval; SLO-BYOK-CMK-DETECT) + **DEK cache TTL ≤ 5 min hard** (no extension; SLO-BYOK-DEK-EVICT) → **customer-perceived global kill switch p99 ≤ 6 min** (60s detection p99 + 5 min cache TTL p99). Componentes individuais hard-fail; total realistic SLA é 6 min p99 (não 5 min).
- ❌ DEK cache TTL ≤ 5 min hard non-waivable (componente do kill switch; cannot be waived).
- ❌ INV-REGION-NO-CROSS-LEAK property test verde — Schrems II baseline.
- ❌ External pentest BYOK clean — security baseline.
- ❌ FIPS doc per provider (140-3 onde available — AWS KMS + Vault Enterprise; 140-2 onde 140-3 ainda not certified — GCP KMS L1, Azure Key Vault Premium L2; matrix em `compliance/byok-fips-matrix.md` documented per provider) — compliance baseline.
- ❌ Erasure attestation Ed25519 signed verifiable — NIST SP 800-88 requirement.

Itens waivable com Security lead + Compliance officer + Legal + ADR:

- ⚠️ BYOK 4 providers → 3 providers GA (defer 1 to post-GA com customer demand).
- ⚠️ Replication lag p99 ≤ 60s → ≤ 120s com customer SLA addendum.
- ⚠️ KMS check interval 60s → 30s (tightening; reduces detection p99 a ~30s; cost overhead 2× KMS API calls).

---

**Fim spec contract S-14 v1.1.0 SOTA.**
