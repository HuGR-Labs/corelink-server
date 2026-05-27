---
id: "S-14"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-28"
updated: "2026-04-28"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-003", "FF-HR-005", "FF-HR-008", "FF-HR-009"]
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "KEY-MANAGEMENT"
  - "SECURITY-MODEL"
  - "PRIVACY-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "COMPLIANCE-MATRIX"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
  - "INVARIANT-REGISTRY"
  - "AUTH-MODEL"
tags: ["sprint", "s14", "region", "byok", "kms", "fips-140-3", "residency", "schrems-ii", "ed25519", "high-risk"]
---

# Sprint S-14 — Region Expansion (4 Regions WNAM/ENAM/WEUR/SAM) + Cross-Region Failover (PAT-REGION-FAILOVER-001) + BYOK Enterprise (AWS KMS + GCP KMS + Azure Key Vault + HashiCorp Vault) + CMK Kill Switch ≤ 5 min + Erasure Attestation Ed25519 + DPA/Schrems II TIA + TLA+ region_residency + External Pentest

> **doc_status:** DRAFT · **lane:** HIGH_RISK · **Versão:** 1.0.0 · **2026-04-28**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (cycle 9.4 SOTA elevation; 8 lotes + 4 audits adversariais)

> **Phase boundary:** Fase 4 — Multi-region production-grade + crypto sovereignty enterprise tier pré-GA.
> **FF-HR-002+003+005+008+009 specific**: cross-region tenant data leak = catastrophic Schrems II/LGPD legal exposure; BYOK introduz crypto-load-bearing controles (CMK + envelope encryption + kill switch) cuja falha = customer trust loss permanent + breach notification mandatory; multi-cloud KMS adds vendor lock-in surface; DPA amendment é customer-facing legal contract enterprise-revenue-blocking se mal redigido.

---

## 1. Objetivo

Expandir CoreLink para **multi-region production-grade** (4 regiões: **WNAM** us-west / **ENAM** us-east / **WEUR** eu-west / **SAM** sa-east; APAC/AFR deferred pós-GA demand-driven) + entregar **BYOK enterprise tier** com 4 KMS providers (**AWS KMS first-class** + **GCP KMS** + **Azure Key Vault Premium HSM** + **HashiCorp Vault Transit**), region read failover transparent (PAT-REGION-FAILOVER-001), customer kill switch (revoke CMK → cache inacessível p99 ≤ 6 min global (60s detection + 5min DEK cache TTL hard) hard-fail) e erasure attestation signed Ed25519 (NIST SP 800-88 Rev.1 crypto-erase compliant + 7y retention). Critical pra **enterprise audience** (FedRAMP-ready customers + EU data residency + crypto sovereignty + Schrems II compliance posture).

Decomposição cumulativa em 9 WIs: (1) **WI-S14-001** R2+D1+DO provisioning 4 regions via Terraform module reusable + data migration script + RB region; (2) **WI-S14-002** tenant `primary_region` enforcement no write path com insert checks + DO `region_enforcer` + 30k property test cross-region 0 leaks (INV-REGION-NO-CROSS-LEAK CRITICAL); (3) **WI-S14-003** hot blob replica worker (top 1% identified via **OFFLINE batch aggregation** sobre S-09 audit log R2 — **NÃO** via métrica labeled por `tenant_id` proibido por INV-OBS-CARDINALITY-BUDGET S-09; live métrica é `corelink_cas_get_bytes_total{tenant_tier, region}` budget-safe; Lote 9.4 Opus H-02 fix em spec contract R-S14-3) + read failover routing PAT-REGION-FAILOVER-001 + chaos test region outage; (4) **WI-S14-004** BYOK adapter trait `crates/corelink-byok` + AWS KMS adapter first-class + envelope encryption (DEK ephemeral random 32 bytes via CSPRNG [Lote 10.14 codex P1 fix; NÃO BLAKE3-derived deterministic] → body AES-256-GCM → wrap via KMS → wrapped DEK em D1 + body em R2; read flow: fetch wrapped → unwrap via KMS network call ≤30ms p99 region-co-located → decrypt body; DEK cache TTL 5 min hard limit no exception) + 16-combination matrix test framework + FIPS 140-3 doc per provider; (5) **WI-S14-005** BYOK GCP/Azure/Vault adapters batch + 16-combination matrix test (4 providers × write/read/wrap/unwrap) verde em staging weekly; (6) **WI-S14-006** CMK revocation detection (KMS access check background every 60s) + customer kill switch p99 ≤ 6 min global (60s detection + 5min DEK cache TTL hard) (DEK cache TTL 5 min hard expires + KMS access check 60s detect + degrade tenant read-only + audit emit `corelink.byok.cmk_revoked` + alert customer; INV-BYOK-CRYPTO-SOVEREIGNTY CRITICAL hard-fail no operator override) + chaos drill weekly; (7) **WI-S14-007** erasure attestation Ed25519 signing (per-region Ed25519 key 30d overlap canonical key_management.md §3.2.1) + per-DSR-erasure de BYOK tenant produz signed attestation `{tenant_id, request_id, destroyed_ts, kms_provider, evidence_hash, signature_ed25519}` + 7y retention em R2 audit bucket + verify endpoint via public-key + NIST SP 800-88 Rev.1 crypto-erase mode; (8) **WI-S14-008** DPA amendment + Schrems II TIA legal templates (`legal/dpa-residency-amendment.md` + `legal/tia-template.md`) + Legal externo review path + 1 enterprise customer beta signed; (9) **WI-S14-009** TLA+ `region_residency.tla` (cross-region routing actions; verde no CI gate `tla_region_residency_check.sh`) + RB-BYOK-REVOKE dry-run + RB-FM-054 + RB-FM-105 dry-run + external pentest BYOK (Schellman ou A-LIGN; AWS KMS provider primary scope; buffer 10 dias remediation; zero CRITICAL findings = SEAL gate) + PRR HIGH_RISK 11 sign-offs canonical + adversarial summary aggregation. Mitiga **FM-054** (KV global leak), **FM-105** (region replication diverge se existir; create stub se não), **FM-204** (BYOK rotation in-flight quebra serviço — herda S-13 PAT-ROLL-FORWARD-001). Implementa CTRL-CRYPTO-005 (BYOK) + CTRL-KEY-010..015 (envelope encryption + Ed25519 attestation) + CTRL-PRIV-031 (residency) + ratifica INV-BYOK-CRYPTO-SOVEREIGNTY (CRITICAL §3.12) + INV-REGION-NO-CROSS-LEAK (CRITICAL §3.12) + INV-ERASURE-ATTESTATION-SIGNED (HIGH §3.12) + reforça INV-DATA-RESIDENCY (CRITICAL §3.11) + INV-KEY-NO-SKIP + INV-KEY-OVERLAP (§3.13).

**Por que SOTA**: competitors (AWS S3+KMS, GCS+Cloud KMS, Azure Blob+Key Vault, NativeLink) offer either multi-region OR BYOK, raramente ambos com rigor formal. CoreLink S-14 entrega: (a) BYOK FIPS doc per provider em `compliance/byok-fips-matrix.md`; (b) crypto-erase via key destroy NIST SP 800-88 Rev.1 compliant integrado com S-11 DSR; (c) DPA amendment template Schrems II TIA covered (vs competitors' standard); (d) erasure attestation Ed25519-signed verifiable post-facto (zero competitors); (e) TLA+ formal verification region residency (zero competitors). Reference: **NIST SP 800-57 Pt 1 Rev 5** (key management), **NIST SP 800-130** (cryptographic key management framework), **NIST SP 800-88 Rev.1** (crypto-erase), **FIPS 140-3** (cryptographic module security), **FIPS 186-5** (Ed25519 digital signature), **EDPB Recommendations 01/2020** (Schrems II TIA supplementary measures), **CSA Cloud Controls Matrix v4** (multi-cloud baseline).

## 2. Escopo

### 2.1 In-scope

- **WI-S14-001**: R2 buckets + D1 instances + DO storage provisioning em 4 regiões via Terraform module `corelink-region` reusable (per-region inputs: `region_name + r2_location_hint + d1_location + do_jurisdiction + cf_zone`); data migration script (S-01..S-13 single-region → S-14 multi-region) + dry-run report; RB-region (provisioning + rollback + migration runbook).
- **WI-S14-002**: Tenant `primary_region` enforcement schema (D1 column `tenants.primary_region NOT NULL CHECK IN ('wnam','enam','weur','sam')`); insert checks no write path (`tenant.region == request.region` mismatch = 403 + audit emit `corelink.region.cross_region_read_blocked`); DO `region_enforcer` validates per-request via tenant lookup; property test 30k iter cross-region scenarios → 0 leaks; runbook RB-region-leak; INV-REGION-NO-CROSS-LEAK CRITICAL ratificada.
- **WI-S14-003**: Hot blob replica worker (PAT-REGION-FAILOVER-001) — **offline batch aggregation** sobre S-09 audit log R2 daily computa top-1% per tenant (escapes cardinality budget porque é offline; live métrica é `corelink_cas_get_bytes_total{tenant_tier, region}` tier-labeled NÃO tenant-labeled per Lote 9.4 Opus H-02); replication worker async copia para sibling region; lag p99 ≤ 60s SLO sustained 7d; failover routing transparent durante region outage (chaos test verde); SLO-LAT-CAS-GET p99 < 300ms preserved during failover.
- **WI-S14-004**: BYOK adapter trait `crates/corelink-byok` (Rust): `trait KmsProvider { fn wrap_dek + fn unwrap_dek + fn check_access }`; AWS KMS adapter first-class FIPS 140-3 Level 1 verified; envelope encryption flow (write: gen ephemeral **DEK random 32 bytes via CSPRNG** [Lote 10.14 codex P1 fix; NÃO BLAKE3-derived deterministic — deterministic = compromise propagation] → encrypt body AES-256-GCM nonce per-write 96-bit random → wrap DEK via AWS KMS → store wrapped DEK em D1 + body em R2; read: fetch wrapped DEK → unwrap via KMS network call ≤30ms p99 region-co-located → decrypt body); DEK cache TTL 5 min hard limit (no exception, no advisory mode; INV-BYOK-CRYPTO-SOVEREIGNTY enforces); 16-combination matrix test framework + FIPS doc.
- **WI-S14-005**: BYOK GCP KMS adapter (FIPS 140-2 / 140-3 quando suportado) + Azure Key Vault Premium HSM adapter (FIPS 140-2 Level 2) + HashiCorp Vault Transit adapter (FIPS 140-3 Level 1 Vault Enterprise; mTLS auth customer-hosted) batch implementation; 16-combination matrix test `tests/byok_matrix_test.rs` (4 providers × 4 ops {write, read, wrap, unwrap}) verde em staging weekly + auto-fail PR se matrix break; FIPS doc per provider em `compliance/byok-fips-matrix.md`.
- **WI-S14-006**: CMK revocation detection background — KMS access check every 60s per active BYOK tenant; revoked detected → degrade tenant read-only + emit audit `corelink.byok.cmk_revoked` + alert customer; customer kill switch p99 ≤ 6 min global hard-fail (60s detection + 5min DEK cache TTL hard; Lote 10.14 codex P0 disambiguation) (DEK cache TTL 5 min hard expires all in-flight reads; total p99 ≤ 5 min; INV-BYOK-CRYPTO-SOVEREIGNTY no operator override); chaos drill weekly em staging; runbook RB-BYOK-REVOKE dry-run.
- **WI-S14-007**: Erasure attestation Ed25519 — per-region Ed25519 signing key (30d overlap canonical per `key_management.md §3.2.1` + ADR-0018; rotation worker S-13 owns); per-DSR-erasure de BYOK tenant (S-11 integration) destrói CMK access + emite signed attestation `{tenant_id, request_id, destroyed_ts, kms_provider, evidence_hash, signature_ed25519}`; retained 7y em R2 audit bucket; verify endpoint `GET /v1/public/attestation/{request_id}` via public-key endpoint `GET /v1/public/keys/erasure/{region}.pub`; NIST SP 800-88 Rev.1 crypto-erase mode; INV-ERASURE-ATTESTATION-SIGNED HIGH ratificada.
- **WI-S14-008**: DPA amendment template `legal/dpa-residency-amendment.md` covering residency commitment per region (4 regions enumerated com data localization promises) + Schrems II TIA template `legal/tia-template.md` (EDPB Recommendations 01/2020 supplementary measures; technical + organizational + contractual controls per Schrems II framework); Legal externo review path (no in-house counsel; engagement plan ~$15-30k 6-week lead); 1 enterprise customer beta DPA signed (lighthouse).
- **WI-S14-009**: TLA+ `specs/tla/region_residency.tla` (cross-region routing actions; full TLC-checked spec (Lote 10.14 codex P1: stub-only NOT acceptable); CI gate `scripts/tla_region_residency_check.sh` verde no PR); RB-BYOK-REVOKE dry-run + RB-FM-054 (KV stale cross-region) + RB-FM-105 (region replication diverge if exists; create stub if not) dry-runs em staging; external pentest BYOK engagement (Schellman ou A-LIGN per spec contract risk row 11; AWS KMS provider primary scope; buffer 10 dias remediation; zero CRITICAL findings = SEAL gate); PRR HIGH_RISK 11 sign-offs canonical (per framework §33.5.4.3 + ADR-0034); adversarial test summary aggregation (cumulative 9 WIs + chaos scenarios).

### 2.2 Anti-scope

- APAC/AFR regions (pós-GA demand-driven; 6+ months pós-GA).
- Active-active multi-region writes (primary-only em S-14; active-active = Fase 2).
- BYOE (Bring Your Own Encryption) — Fase 2.
- Customer-managed HSM on-prem (only cloud KMS at GA).
- Quantum-resistant crypto / post-quantum migration — Fase 3.
- Per-tenant region migration (lock at signup; migration = manual ticket).
- Federated KMS (multi-cloud failover) — single CMK per tenant at GA.
- Cross-region replication for non-hot blobs — top 1% only.
- Customer-managed audit chain attestation key (CoreLink-managed em S-14; customer-attested em Fase 2).
- DPA template auto-signing flow (S-19 enterprise onboarding cobre wizard).
- Multi-DPO escalation workflow (single DPO em S-14).
- Customer-facing BYOK dashboard UI (S-16 admin UI cobre wizard).

## 3. Customer Impact & Journey

**JTBD:** "Como CISO em prospect enterprise (FedRAMP-ready / EU customer / financial services), preciso evidência verificável de que: (a) **dado de tenant EU jamais sai de WEUR** — nem em failover, nem em chunk dedup cross-region, nem em backup; insertchecks + 30k property test 0 leaks + TLA+ region_residency verifica formalmente; (b) **revogo o CMK no meu KMS provider e CoreLink fica inacessível em ≤ 5 min global** sem operator override possível — kill switch hard-fail enforced via INV-BYOK-CRYPTO-SOVEREIGNTY + chaos drill weekly evidence; (c) **erasure de tenant produz signed attestation Ed25519** que posso verificar offline via public key endpoint, retida 7y por compliance auditor; (d) **DPA amendment + Schrems II TIA** Legal-reviewed externamente cobrindo todas 4 regions + supplementary measures EDPB Recommendations 01/2020. Como auditor SOC 2 Type II + ISO 27001 + GDPR DPO, preciso attestation que: residency é enforced em código (não policy), BYOK FIPS 140-3 / 140-2 documented per provider, erasure attestation crypto-erase mode NIST SP 800-88 Rev.1 compliant, external pentest BYOK clean."

**CAPs entregues:** CAP-REGION-001 (4 regiões operantes) + CAP-REGION-002 (tenant pinning + cross-region restrict) + CAP-REGION-003 (read failover hot blobs) + CAP-REGION-004 (DPA + Schrems II TIA) + CAP-BYOK-001..004 (4 KMS providers) + CAP-BYOK-005 (kill switch) + CAP-BYOK-006 (erasure attestation Ed25519).

**Persona 1 — CISO em prospect enterprise (FedRAMP / EU / financial services)**:
- Evidence pack inclui: `corelink_byok_wrap_dek_duration_seconds_bucket{provider, plan}` p99 ≤ 30ms; `corelink_byok_cmk_revoked_total{provider, plan}` chaos drill records weekly; `corelink_region_failover_total{from_region, to_region, plan}` chaos test sustained; FIPS 140-3 / 140-2 documented per provider em `compliance/byok-fips-matrix.md`; TLA+ region_residency CI gate green; external pentest BYOK clean report (zero CRITICAL).
- Diferenciador competitivo: AWS S3+KMS / GCS / Azure Blob oferecem multi-region OR BYOK; CoreLink S-14 oferece **ambos com TLA+ formal verification + erasure attestation Ed25519** (zero competitors).

**Persona 2 — Auditor SOC 2 Type II + ISO 27001 + GDPR DPO**:
- CTRL-CRYPTO-005 (BYOK) + CTRL-KEY-010..015 (envelope encryption + Ed25519 attestation) + CTRL-PRIV-031 (residency) attestation dossier.
- PRR doc S-14 11 sign-offs canonical (Owner + Final Approver + Architect com Crypto SME folded mandatory + Security Lead + SRE Lead + Engineer + QA Lead + Product + Compliance + Privacy + AppSec); WI-S14-008 adds Legal Counsel as 12th sign-off (legal-touching exception).
- Compliance Matrix mapping: SOC 2 CC6.1 (logical access — encryption + region) + CC6.7 (change management — Terraform) + CC8.1 (system change); ISO 27001 A.5.10 (information security policy multi-region) + A.5.34 (privacy + residency) + A.10.1 (cryptography); NIST SP 800-57 Pt 1 Rev 5 §5 (key management lifecycle) + NIST SP 800-88 Rev.1 §2.4 (crypto-erase) + NIST SP 800-130 (cryptographic key management framework); FIPS 140-3 (cryptographic modules) + FIPS 186-5 (Ed25519); EDPB Recommendations 01/2020 (Schrems II); LGPD Art. 33 §1º (residência) + GDPR Art. 17 (erasure) + Art. 32 (security of processing) + Art. 46 (international transfers).

**Persona 3 — Engineer onboarding em CoreLink S-14**:
- `docs/internal/multi-region-byok.md` explica 9 CAPs + envelope encryption flow + 4 KMS providers + erasure attestation chain + TLA+ region_residency primer.
- ADRs ratificadas: ADR-0018 (key overlap per asset) reused para Ed25519 30d; novos forward ADRs documentam BYOK adapter trait (per-provider isolation) + envelope encryption flow + cross-region replication strategy hot 1%.
- Runbooks RB-BYOK-REVOKE + RB-FM-054 + RB-FM-105 dry-run reports committed.

**SLA addendum**:
- Cross-region tenant isolation: 0 leaks em 30k property test (INV-REGION-NO-CROSS-LEAK).
- Region read failover: SLO-LAT-CAS-GET p99 < 300ms preserved during failover (PAT-REGION-FAILOVER-001).
- Hot blob replication lag: p99 ≤ 60s sustained 7d staging.
- BYOK latency overhead: p99 < 30ms (AWS KMS region-co-located baseline).
- DEK cache TTL: 5 min hard limit (no exception).
- CMK revocation kill switch: ≤ 5 min global p99 (chaos drill weekly).
- Erasure attestation: Ed25519-signed verifiable post-facto via public key; 7y retention.
- DPA amendment: Legal externo review + 1 enterprise customer beta signed.
- TLA+ region_residency: CI gate green no PR.
- External pentest BYOK: zero CRITICAL findings.

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4`. Foundation: `security_model.md §6` (CTRL-CRYPTO-005 BYOK + CTRL-KEY-010..015 envelope encryption + CTRL-KEY-015 Ed25519 attestation) + `privacy_model.md` (CTRL-PRIV-031 residency) + `key_management.md §3.2.1` (overlap canonical: BYOK CMK 7d + Ed25519 attestation 30d) + `resilience_patterns.md` (PAT-REGION-FAILOVER-001) + `observability_model.md §3.1` (cardinality budget + `plan` label canonical) + `failure_modes.md` (FM-054 + FM-105) + `invariant_registry.md §3.12` (INV-DATA-RESIDENCY + INV-BYOK-CRYPTO-SOVEREIGNTY + INV-REGION-NO-CROSS-LEAK + INV-ERASURE-ATTESTATION-SIGNED) + `invariant_registry.md §3.13` (INV-KEY-NO-SKIP + INV-KEY-OVERLAP).

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S14-D1 | R2+D1+DO 4 regions Terraform module + migration script + RB-region | `infra/terraform/modules/corelink-region/` + `scripts/migrate_single_to_multi_region.rs` + `specs/05_runbooks/RB-region.md` | Module reusable per-region inputs; 4 regions provisioned (WNAM/ENAM/WEUR/SAM); migration dry-run report; RB-region committed; chaos test region outage cada região verde |
| S14-D2 | Tenant region pinning enforcement + 30k property test | `crates/corelink-region-enforcer/` + `crates/corelink-worker/middleware/region_check.rs` + `tests/region_pinning_proptest.rs` | D1 schema migration `tenants.primary_region`; insert checks; DO `region_enforcer`; 30k property test cross-region 0 leaks; INV-REGION-NO-CROSS-LEAK ratificada; runbook RB-region-leak |
| S14-D3 | Hot blob replica worker + read failover (PAT-REGION-FAILOVER-001) | `crates/corelink-replica-worker/` + `crates/corelink-failover-router/` + `scripts/hot_blob_offline_aggregator.rs` | Offline batch daily job sobre S-09 audit log R2 (top 1% per tenant); replication worker async; lag p99 ≤ 60s 7d sustained; failover routing transparent; SLO-LAT-CAS-GET p99 < 300ms preserved chaos test |
| S14-D4 | BYOK trait + AWS KMS adapter + envelope encryption + matrix framework | `crates/corelink-byok/` + `crates/corelink-byok-aws/` + `tests/byok_matrix_framework.rs` + `compliance/byok-fips-matrix.md` | Trait `KmsProvider`; AwsKmsProvider impl; envelope encryption flow (DEK CSPRNG random 32 bytes → AES-256-GCM → wrap → D1 + R2; Lote 10.14 codex P1 fix); DEK cache TTL 5 min hard; matrix framework; FIPS 140-3 doc AWS KMS Level 1 verified |
| S14-D5 | BYOK GCP/Azure/Vault adapters + 16-combination matrix test | `crates/corelink-byok-gcp/` + `crates/corelink-byok-azure/` + `crates/corelink-byok-vault/` + `tests/byok_matrix_test.rs` | 3 adapters impl; 16 combinations (4 providers × 4 ops) green em staging weekly; FIPS doc per provider; auto-fail PR se matrix break; staging deploy |
| S14-D6 | CMK revocation kill switch ≤ 5 min + chaos drill | `crates/corelink-byok-revocation/` + `specs/05_quality/runbooks/RB-BYOK-REVOKE.md` | KMS access check 60s background; cache TTL 5 min hard; revoke flow degrade read-only + audit emit + alert; chaos test verified ≤ 5 min global; INV-BYOK-CRYPTO-SOVEREIGNTY ratificada; RB-BYOK-REVOKE dry-run |
| S14-D7 | Erasure attestation Ed25519 + 7y retention + verify endpoint | `crates/corelink-erasure-attestation/` + `crates/corelink-public-keys-api/` + `migrations/0XX_erasure_attestation.sql` | Per-region Ed25519 key 30d overlap canonical; per-DSR-erasure signed attestation; storage R2 7y; verify endpoint via public-key; NIST SP 800-88 Rev.1 evidence; INV-ERASURE-ATTESTATION-SIGNED ratificada |
| S14-D8 | DPA amendment + Schrems II TIA + Legal review + 1 customer signed | `legal/dpa-residency-amendment.md` + `legal/tia-template.md` + `specs/_audits/2026-XX-XX-legal-review-s14.md` | DPA template covering 4 regions; TIA template EDPB 01/2020; Legal externo review; 1 enterprise customer beta signed (lighthouse); ADR if waiver needed |
| S14-D9 | TLA+ region_residency + RB-BYOK-REVOKE + External pentest BYOK + PRR | `specs/tla/region_residency.tla` + `scripts/tla_region_residency_check.sh` + `specs/_audits/2026-XX-XX-pentest-byok-s14.md` + `specs/04_sprints/S14/PRR-S14.md` | TLA+ spec verde no CI gate; pentest engagement clean (zero CRITICAL); buffer 10 dias remediation; PRR doc 11 sign-offs canonical (12 com Legal Counsel para WI-008 trace); adversarial summary 30+ scenarios |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Auth Model (herda `auth_model.md`)

- TenantCtx propagation cross-region: tenant `primary_region` field stored em D1 + cached em DO; middleware lookup ensures handler tem region context antes de R2/D1 access.
- BYOK customer initiates rotation via admin API com Clerk JWT RS256 + WebAuthn UV=1 step-up (CTRL-AUTH-010 herdada S-03; admin role required for BYOK config).
- DSR erasure trigger requires WebAuthn UV=1 + 7-day cooling-off period (S-11 herdada); BYOK erasure attestation generated atomic with key destroy.
- INV-AUTH-PII-ENCRYPTED herdada S-03 reforced — email + WebAuthn keys cipher BYTEA via pgcrypto regardless of tenant region.

### 6.2 Security Model (herda `security_model.md §6`)

- **CTRL-CRYPTO-005** (BYOK) — IMPLEMENTA primary; envelope encryption flow documented em §6.4 detailed design; per-provider FIPS attestation; DEK cache TTL 5 min hard limit.
- **CTRL-KEY-010** (CMK customer-controlled) — IMPLEMENTA via 4 KMS providers; CoreLink stores wrapped DEK only, never plaintext CMK material.
- **CTRL-KEY-011** (KMS access verification) — IMPLEMENTA background 60s check + revocation detection.
- **CTRL-KEY-012** (Kill switch hard-fail) — IMPLEMENTA via INV-BYOK-CRYPTO-SOVEREIGNTY enforcement.
- **CTRL-KEY-013** (BYOK audit emission) — IMPLEMENTA via `corelink.byok.{wrap,unwrap,revoke,kill_switch}` events.
- **CTRL-KEY-014** (BYOK rotation customer-trigger) — herda S-13 rotation framework; BYOK adapter integrates.
- **CTRL-KEY-015** (Ed25519 attestation) — IMPLEMENTA primary em WI-S14-007; per-region signing key 30d overlap.
- **CTRL-PRIV-031** (residency) — IMPLEMENTA primary via insert checks + DO enforcer + 30k property test.
- **CTRL-AUDIT-001 + CTRL-AUDIT-002 + CTRL-AUDIT-005** herdadas (audit hash chain + rich events + 7y retention; erasure attestation contributes 7y events).

### 6.3 Privacy Model (herda `privacy_model.md`)

- **CTRL-PRIV-031** residency — covered §6.2.
- LGPD Art. 33 §1º — residency enforcement; cross-region read = 403 + audit; supplementary technical measure = encryption-at-rest (BYOK FIPS-verified).
- GDPR Art. 17 — erasure right; BYOK erasure = crypto-erase via key destroy NIST SP 800-88 Rev.1 mode; signed attestation delivered.
- GDPR Art. 32 — security of processing; BYOK + multi-region failover + chaos drill weekly = appropriate technical measures.
- GDPR Art. 46 + Schrems II — international transfers; DPA amendment + TIA template (EDPB Recommendations 01/2020 supplementary measures: technical {encryption} + organizational {DPA + 4 regions enumerated} + contractual {DPA + sub-processor agreement}).
- LINDDUN delta vide §12.

### 6.4 Key Management (herda `key_management.md §3.2.1`)

Overlap canonical per asset (tabela 3.2.1):
- **BYOK customer CMK** (CoreLink-side cache): **7d** (customer notification window aligned).
- **Ed25519 attestation key** (per-region): **30d** (long overlap pra retain verifiability post-rotation; verify endpoint accepts both keys during overlap).
- **DEK cache TTL**: **5 min hard** (NÃO um overlap key — é cache invalidation TTL; INV-BYOK-CRYPTO-SOVEREIGNTY governs).

Hard upper bound: 30d sem ADR + Security lead sign-off.
INV-KEY-OVERLAP enforced via property test per asset class (10k iter; rotation worker S-13 owns BYOK + Ed25519 attestation rotation).

Envelope encryption flow detailed:
```
Write path:
  1. Generate ephemeral DEK random 32 bytes via CSPRNG (`getrandom::getrandom`; NOT KDF-derived deterministic — Lote 10.14 codex P1 canonical fix; rewrap durante CMK rotation = unwrap+rewrap atomic preserving DEK identity, sem precisar deterministic derivation)
  2. Encrypt body via AES-256-GCM with DEK; nonce per-write 96-bit random
  3. Wrap DEK via KmsProvider::wrap_dek(dek) → WrappedDek bytes
  4. Store: D1 row (tenant_id, blob_hash, wrapped_dek, kms_provider, kms_key_id, created_at) + R2 body ciphertext

Read path:
  1. Fetch D1 row (wrapped_dek, kms_provider, kms_key_id)
  2. Check DEK cache (TTL 5 min hard limit); if hit → step 4
  3. If miss → KmsProvider::unwrap_dek(wrapped_dek) network call ≤30ms p99 region-co-located → cache DEK 5 min TTL
  4. Decrypt body via AES-256-GCM with DEK; verify nonce + tag
  5. Return plaintext

Kill switch path (INV-BYOK-CRYPTO-SOVEREIGNTY):
  1. KMS access check background 60s detects revoked
  2. Emit audit corelink.byok.cmk_revoked
  3. Mark tenant degraded read-only
  4. Alert customer
  5. DEK cache TTL 5 min expires all in-flight reads
  6. Total p99 ≤ 5 min global
  7. Hard-fail (no operator override; no advisory mode)
```

### 6.5 Resilience Patterns (herda `resilience_patterns.md`)

- **PAT-REGION-FAILOVER-001**: WI-S14-003 implementa primary; FM-054 + FM-105 mitigated.
- **PAT-ROLL-FORWARD-001**: herda S-13; BYOK rotation reuses se downstream errors > 1%.
- **PAT-FORMAL-VERIFICATION-001**: WI-S14-009 implementa via TLA+ region_residency.
- **PAT-DUAL-APPROVAL-001**: herda S-13; admin BYOK config changes use dual-approval.

### 6.6 Failure Modes (herda `failure_modes.md`)

- **FM-054** (KV global leak — KV cache cross-region) — RB-FM-054 dry-run em WI-S14-009; INV-REGION-NO-CROSS-LEAK + KV namespace per-region + 30k property test mitigation.
- **FM-105** (region replication diverge if exists; create stub if not) — RB-FM-105 dry-run em WI-S14-009; INV-CAS-INTEGRITY (hash check post-replica) + reconciliation + alerts mitigation.
- **FM-204** (BYOK rotation in-flight breakage) — herda S-13 PAT-ROLL-FORWARD-001 mitigation.

### 6.7 Observability Model (herda `observability_model.md §3.1`)

Métricas underscored snake_case com label `plan` aplicável (per `observability_model.md §3.1` cardinality budget + §4.1 naming convention; NUNCA per-tenant labels — INV-OBS-CARDINALITY-BUDGET):

- `corelink_byok_wrap_dek_duration_seconds_bucket{provider, plan}` (histogram p50/p95/p99 wrap latency).
- `corelink_byok_unwrap_dek_duration_seconds_bucket{provider, plan}` (histogram unwrap latency ≤ 30ms p99 target).
- `corelink_byok_dek_cache_hit_total{provider, plan}` (counter).
- `corelink_byok_dek_cache_miss_total{provider, plan}` (counter).
- `corelink_byok_dek_cache_evict_total{provider, reason, plan}` (reason ∈ ttl_expired|cmk_revoked|capacity).
- `corelink_byok_cmk_revoked_total{provider, plan}` (counter; chaos drill increments).
- `corelink_byok_cmk_access_check_total{provider, outcome, plan}` (outcome ∈ ok|revoked|api_error).
- `corelink_byok_kill_switch_duration_seconds_bucket{provider, plan}` (histogram revoke → cache empty ≤ 5 min p99).
- `corelink_byok_matrix_test_total{provider, op, outcome}` (op ∈ write|read|wrap|unwrap; outcome ∈ ok|fail).
- `corelink_region_failover_total{from_region, to_region, plan}` (counter).
- `corelink_region_replication_lag_seconds{primary_region, replica_region, plan}` (histogram p99 ≤ 60s SLO).
- `corelink_region_cross_region_read_blocked_total{primary_region, request_region, plan}` (counter; alert > 0).
- `corelink_region_hot_blob_replicated_total{primary_region, replica_region, plan}` (counter).
- `corelink_cas_get_bytes_total{tenant_tier, region}` (counter; **tier label NOT tenant_id**; cardinality budget safe).
- `corelink_erasure_attestation_signed_total{region, kms_provider, plan}` (counter).
- `corelink_erasure_attestation_verify_total{outcome}` (outcome ∈ ok|sig_invalid|key_not_found).

### 6.8 SLOs

- **SLO-REGION-FAILOVER-LATENCY** (novo): SLO-LAT-CAS-GET p99 < 300ms preserved during region failover chaos test.
- **SLO-REGION-REPLICATION-LAG** (novo): replication lag p99 ≤ 60s sustained 7d staging.
- **SLO-BYOK-WRAP-LATENCY** (novo): wrap_dek p99 ≤ 30ms region-co-located baseline.
- **SLO-BYOK-UNWRAP-LATENCY** (novo): unwrap_dek p99 ≤ 30ms region-co-located baseline.
- **SLO-BYOK-KILL-SWITCH** (novo): revoke → cache empty p99 ≤ 5 min global.
- **SLO-BYOK-MATRIX-AVAILABILITY** (novo): 16-combination matrix green em staging weekly.

## 7. Definition of Done (lane HIGH_RISK)

> **Two-phase SEAL** (per Timeline §9): items verificáveis instantaneamente fecham em **Implementation SEAL D+30**; items requerendo "sustained 30d staging" janela (4 regions stable + replication lag 7d + matrix weekly + kill switch chaos drill weekly + DPA customer signed) fecham em **GA Evidence Gate SEAL D+60**. Ambos SEALs canonicos; sprint considerado concluído apenas após GA Evidence Gate D+60.

- [ ] **WIs SEALED**: 9/9 (EVT-031).
- [ ] **4 regiões live** + chaos test region outage cada região (EVT-023).
- [ ] **Tenant EU property test**: blob lands em WEUR, nunca ENAM (30k test) — INV-REGION-NO-CROSS-LEAK ratificada (EVT-002).
- [ ] **BYOK AWS KMS E2E**: customer CMK → wrap DEK → CAS write → read decrypt sucesso (EVT-024).
- [ ] **BYOK 4 providers matrix test** verde em staging (16 combinations) (EVT-002).
- [ ] **Kill switch test**: revoke CMK → cache 503 ≤ 5 min global; chaos drill (EVT-023) — INV-BYOK-CRYPTO-SOVEREIGNTY ratificada.
- [ ] **SLO sustained** during region failover chaos: SLO-LAT-CAS-GET p99 < 300ms preserved (EVT-021).
- [ ] **Replication lag** p99 ≤ 60s para hot blobs sustained 7d staging *(GA Evidence Gate D+60)* (EVT-021).
- [ ] **Residency DPA amendment** drafted + reviewed por Legal externo (EVT-044).
- [ ] **Schrems II TIA template** drafted (EVT-046).
- [ ] **Erasure attestation Ed25519** test: BYOK tenant DSR erasure → signed attestation generated + verifiable (EVT-024 + EVT-042) — INV-ERASURE-ATTESTATION-SIGNED ratificada.
- [ ] **FIPS 140-3 / 140-2 compliance documented** per provider em `compliance/byok-fips-matrix.md`.
- [ ] **CTRL-CRYPTO-005** (BYOK) + **CTRL-KEY-010..015** (envelope encryption + Ed25519) + **CTRL-PRIV-031** (residency) enforced.
- [ ] **INV-DATA-RESIDENCY** (registry §3.11 herdada) reforced via 30k property test + TLA+ region_residency.
- [ ] **INV-BYOK-CRYPTO-SOVEREIGNTY** (CRITICAL — registry §3.12 nova) ratificada — DEK cache TTL 5 min hard + KMS access check 60s; chaos drill weekly green.
- [ ] **INV-REGION-NO-CROSS-LEAK** (CRITICAL — registry §3.12 nova) ratificada — insert checks + 30k property test 0 leaks; CI gate ativo.
- [ ] **INV-ERASURE-ATTESTATION-SIGNED** (HIGH — registry §3.12 nova) ratificada — Ed25519 signing per-region + 7y retention + verify endpoint.
- [ ] **INV-KEY-OVERLAP** (registry §3.13 herdada) — Ed25519 attestation 30d + BYOK CMK 7d overlap canonical respeitado.
- [ ] **TLA+ region_residency.tla** verde no CI gate `tla_region_residency_check.sh` (EVT-002 + PAT-FORMAL-VERIFICATION-001).
- [ ] **External pentest BYOK flow** (1 provider AWS) — clean zero CRITICAL findings (EVT-040).
- [ ] **Runbook dry-run**: RB-BYOK-REVOKE + RB-FM-054 (KV stale cross-region) + RB-FM-105 (region replication diverge if exists; create stub if not) (EVT-017).
- [ ] **PRR HIGH_RISK** 11 sign-offs canonical (per framework §33.5.4.3 + ADR-0034): Owner + Final Approver + Architect (Crypto SME specialization mandatory para BYOK + KMS + envelope encryption + Ed25519) + Security Lead + SRE Lead + Engineer (S-14 lead) + QA Lead + Product + Compliance Officer + Privacy Officer + AppSec advisor (EVT-031). WI-S14-008 adds Legal Counsel as 12th sign-off (legal-touching exception).
- [ ] **10.s14.1** Replication lag < 60s p99 sustained 7d staging *(GA Evidence Gate D+60)*.
- [ ] **10.s14.2** Residency contract: DPA amendment + Schrems II TIA template Legal-reviewed (EVT-046 + EVT-044) *(GA Evidence Gate D+60)*.
- [ ] **10.s14.3** Cross-region tenant isolation 0 leaks em 30k property test (EVT-002).
- [ ] **10.s14.4** BYOK 4 providers FIPS-verified + matrix test verde sustained weekly *(GA Evidence Gate D+60)*.
- [ ] **10.s14.5** Kill switch ≤ 5 min sustained 30d staging chaos drill weekly *(GA Evidence Gate D+60)*.
- [ ] **10.s14.6** Erasure attestation Ed25519 verifiable post-facto via public key (EVT-024).
- [ ] **10.s14.7** External pentest BYOK clean — zero CRITICAL findings (EVT-040).
- [ ] **10.s14.8** DPA amendment signed com 1 enterprise customer beta (EVT-044) *(GA Evidence Gate D+60)*.
- [ ] **Cost regression gate**: BYOK adds < 15% overhead em CAS path; matched em benchmark CI; multi-region infra ≤ $800/mês (4× R2 + 4× D1 + 4× DO).
- [ ] **Métricas underscored Prometheus**: 16+ região + BYOK metrics emitting em staging com label `plan` aplicável (NUNCA per-tenant labels per INV-OBS-CARDINALITY-BUDGET) (EVT-013).

## 8. Dependencies

### Hard blockers

- **S-01..S-10 SEALED** (core product working: CAS + AC + auth + observability + billing).
- **S-11 SEALED** (residency tooling baseline + DSR for crypto-erase integration).
- **S-12 SEALED** (signed deploy critical para production multi-region).
- **S-13 SEALED** (admin plane para config flags per-region + secret rotation framework BYOK + Ed25519 attestation key rotation).

### Soft blockers

- **S-09 SEALED** (observability cross-region + métricas underscored + audit chain per-region).

### Outbound

- **S-15** (CLI/SDK exposes BYOK config + region selection at signup).
- **S-16** (admin UI BYOK setup wizard + region picker + kill switch UI).
- **S-19** (enterprise onboarding com BYOK + DPA flow + 1 lighthouse customer).
- **S-20** (GA exige 4 regions stable + BYOK 4 providers + DPA signed lighthouse customers + external pentest clean).

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-11 + S-12 + S-13 SEALED).
- **D+5**: WI-S14-001 SEALED (4 regions live).
- **D+8**: WI-S14-002 + WI-S14-003 SEALED (region pinning + failover).
- **D+13**: WI-S14-004 SEALED (AWS KMS + matrix framework).
- **D+18**: WI-S14-005 SEALED (GCP/Azure/Vault).
- **D+21**: WI-S14-006 + WI-S14-007 SEALED (kill switch + erasure attestation).
- **D+25**: WI-S14-008 + WI-S14-009 SEALED (DPA + TLA+ + pentest start).
- **D+30**: Sprint review + 11 sign-offs canonical PRR coletados + **Implementation SEAL ceremony** (todos WIs entregues + tooling em produção + zero P0/P1 abertos + pentest results integrated).
- **D+30..D+60**: **Observation window (30d sustained evidence)** — 4 regions stable + replication lag 7d sustained + matrix weekly verde + kill switch chaos drill weekly + DPA amendment Legal review concluded + 1 enterprise customer beta DPA signed. Métricas coletadas continuously; nenhum WI re-aberto exceto fix-critical.
- **D+60**: **GA Evidence Gate SEAL** (sprint sign-off final) — DoD ship-gate criteria validados com janela 30d real (não-simulada); replication sustained, matrix weekly green streak, kill switch chaos sustained, DPA signed lighthouse customer todos comprovados via DASH-REGION + DASH-BYOK. Implementation já SEALED em D+30; este gate libera S-15 + S-16 + S-19 + S-20 GA dependencies.
- **Total**: 4 semanas implementação (20 dias úteis) + 30d observation window + GA Evidence Gate D+60 + buffer 10 dias multi-cloud unpredictability.

## 10. Risk Register

Ver `_spec_contract.md §15` (13 riscos 6-col com Owner per item: BYOK API drift entre clouds, Residency leak via KV global FM-054, Region replication diverge FM-105, Customer CMK-off causa false SEV-1, DEK cache TTL bypass, KMS provider outage, FIPS compliance drift, Schrems II legal landscape change, Erasure attestation forge attempt, DPA template legal challenge, External pentest finds CRITICAL em BYOK, Multi-region replication storm, Region failover false-positive).

## 11. Observability Plan

DASH-REGION + DASH-BYOK (novos dashboards):

DASH-REGION:
- Region health per-region (4-panel: WNAM/ENAM/WEUR/SAM availability + p50/p99 latency + 5xx rate + queue depth).
- Cross-region read blocked counter + alert > 0 (residency leak indicator).
- Region failover events timeline (chaos test + production incidents).
- Replication lag p99 per replica pair (heatmap).
- Hot blob replication rate per region.

DASH-BYOK:
- BYOK wrap/unwrap latency p99 per provider (4 providers stacked).
- DEK cache hit/miss ratio per provider.
- DEK cache evictions reason breakdown (TTL vs revoke vs capacity).
- CMK revocation events counter + alert > 0.
- Kill switch duration p99 (revoke → cache empty ≤ 5 min SLO).
- Matrix test 16-combination green status (weekly).
- Erasure attestation signed counter per region.
- Erasure attestation verify success rate.

Métricas listadas em §6.7 (16+); todas com label `plan` aplicável; cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (≤ 20k séries únicas per métrica; ≤ 100k total; **NUNCA per-tenant labels** — hot blob detection via offline aggregation NOT live cardinality-violating métrica per Lote 9.4 Opus H-02 fix).

## 12. Security & Privacy

**STRIDE delta** (vs S-13 baseline):
- **Spoofing**: BYOK customer CMK identity verified via KMS provider OAuth/IAM; wrapped DEK contains kms_key_id binding; impersonation requires KMS provider compromise.
- **Tampering**: envelope encryption (AES-256-GCM) detects body tampering via auth tag; wrapped DEK tampering detected via KMS unwrap fail; D1 row tampering detected via signing chain (S-09 audit chain integrity); region replication diverge detected via INV-CAS-INTEGRITY hash check post-replica.
- **Repudiation**: erasure attestation Ed25519-signed verifiable post-facto via public key; 7y retention; per-DSR-erasure CloudEvent rich audit chain (S-09 + S-11 inheritance).
- **Information disclosure**: residency enforced via insert checks + DO region_enforcer + 30k property test; cross-region read = 403 + audit emit (NUNCA passthrough silencioso); KV namespace per-region prevents global leak FM-054; BYOK CMK never plaintext em CoreLink (only wrapped DEK + cached unwrapped DEK 5 min hard TTL); DEK cache eviction atomic on revoke (no race window).
- **DoS**: region failover transparent ≤ 50ms overhead (PAT-REGION-FAILOVER-001); KMS provider outage absorbed by DEK cache 5 min (degrade-mode read-only se sustained > 5 min); customer kill switch is intentional DoS por design (INV-BYOK-CRYPTO-SOVEREIGNTY).
- **Elevation of privilege**: BYOK config requires admin role + Clerk JWT RS256 + WebAuthn UV=1 step-up + dual-approval (herda S-13); CMK access never accessible by CoreLink staff (KMS provider IAM scoping enforces); erasure attestation signing key per-region accessible only by erasure worker service account.

**LINDDUN delta** (vs S-11 baseline):
- **Linkability**: tenant_id em audit é necessário (compliance accountability); region em métrica é tier-labeled NÃO tenant-labeled (cardinality budget); cross-region linkability blocked by insert checks.
- **Identifiability**: BYOK customer email + KMS provider org_id em audit (intentional CTRL-AUDIT-002); LGPD Art. 7º legitimate interest (customer's own data control).
- **Non-repudiation**: erasure attestation Ed25519 = forensic-grade cryptographic evidence verifiable offline; public-key endpoint enables auditor verification; chain integrity preserved across rotation boundary (Ed25519 30d overlap canonical).
- **Detectability**: cross-region read blocked emits audit (intentional alert source); CMK revocation emits audit + alert customer; matrix test failures alert ≤ 5 min.
- **Disclosure of information**: DPA amendment + Schrems II TIA template covers data flow disclosure to customers (EDPB Recommendations 01/2020 transparency); FIPS 140-3 / 140-2 documented per provider em `compliance/byok-fips-matrix.md`.
- **Unawareness**: customer notified per CMK revocation via alert + dashboard; erasure attestation delivered to customer + retained 7y.
- **Non-compliance**: SOC 2 CC6.1/CC6.7/CC8.1 + ISO 27001 A.5.10/A.5.34/A.10.1 + NIST SP 800-57/130/88 + FIPS 140-3/186-5 + EDPB 01/2020 + LGPD Art. 33 + GDPR Art. 17/32/46 satisfied.

## 13. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc** (per `_spec_contract.md §18`):

- Cross-region tenant data leak detected → CRITICAL post-mortem + breach notification consideration (Schrems II + LGPD Art. 33).
- BYOK kill switch SLA miss (> 5 min) → CRITICAL post-mortem + Crypto SME + Compliance.
- DEK cache TTL bypass detected → CRITICAL post-mortem + INV-BYOK-CRYPTO-SOVEREIGNTY review.
- KMS provider outage > 30 min → post-mortem + provider escalation + PAT review.
- Region replication diverge unrecovered > 1h → post-mortem + INV-CAS-INTEGRITY review.
- Erasure attestation forge detected → CRITICAL post-mortem + Security incident.
- DPA legal challenge from customer → post-mortem com Legal + customer trust review.
- Region failover false-positive (slowness misdetected as outage) > 1× mês → review thresholds + post-mortem.
- FIPS compliance drift (provider muda level) → post-mortem + Compliance officer.
- External pentest BYOK CRITICAL finding → CRITICAL + remediation cycle + 5-Why.

## 14. Sign-off (HIGH_RISK 11 canonical)

11 roles per framework §33.5.4.3 + ADR-0034: Owner + Final Approver + Architect (com **Crypto SME specialization MANDATORY** para S-14 — BYOK + envelope encryption + KMS adapter trait + Ed25519 attestation + FIPS attestation + AES-256-GCM body encryption + DEK cache 5 min hard limit) + Security Lead + SRE Lead + Engineer (S-14 lead) + QA Lead + Product + Compliance Officer + Privacy Officer + AppSec advisor.

**WI-S14-008 exception**: legal-touching DPA amendment + Schrems II TIA adds **Legal Counsel as 12th sign-off** (Legal externo review path; ~$15-30k 6-week lead).

Crypto SME folds into Architect role specialization (precedent: S-13 secret rotation + S-12 SLSA L3 + Cosign keyless OIDC). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect per framework §33.5.4.3 + ADR-0034).

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-14 (cycle 12.S14.0; spec contract v1.1.0 8 lotes + 4 audits adversariais base). |

---

**Fim de S-14 sprint contract.**
