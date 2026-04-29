---
id: "S-13"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-28"
updated: "2026-04-28"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "KEY-MANAGEMENT"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
tags: ["sprint", "s13", "admin-plane", "config", "dual-approval", "secret-rotation", "progressive-rollout", "terraform-drift", "high-risk"]
---

# Sprint S-13 — Admin Plane (DO Config Singleton + Dual-Approval + Secret Rotation + Progressive Rollout + Terraform Drift)

> **doc_status:** DRAFT · **lane:** HIGH_RISK · **Versão:** 1.0.0 · **2026-04-28**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (cycle 9.4 SOTA elevation, 12.S13.0)

> **Phase boundary:** Fase 3 — Admin plane defense-in-depth pré-GA.
> **FF-HR-005 specific**: bypass de qualquer controle admin = blast radius **todos os customers** (insider threat trivial; secret rotation cripto-load-bearing; auto-rollback ship gate).

---

## 1. Objetivo

Implementar **admin plane defense-in-depth** que torna operações internas auditáveis, dual-approval-gated e reversíveis: (1) DO `config-singleton` per-region com schema versioned (feature flags tipados + rate-limit tunables + retention policies) atualizado via CAS atomic `(version, payload)` com 90d D1 history e rollback API ≤ 5 min; (2) dual-approval workflow PAT-DUAL-APPROVAL-001 enforced em `POST /v1/admin/ops` (header `X-Dual-Approver` + HMAC signature + caller≠approver D1 hard-check + collusion-rotation defense canônica Lote 10.13: new approver MUST NOT have approved any of the last 2 destructive ops in 24h window (equivalente: rolling window de 3 ops consecutivas tem ≥ 3 distinct approvers; oracle prévio LIMIT 3 prior tinha bypass A→B/B→A/A→B), NIST SP 800-53 AC-2(7); missing approver = 403 hard-fail, NÃO advisory); (3) secret rotation worker zero-downtime para 5 asset types per `key_management.md §3.2.1` canonical (TDK 7d / PAT signing 24h / audit chain 24h / admin signing 24h / BYOK 7d) com PAT-ROLL-FORWARD-001 auto-rollback se downstream errors > 1%; (4) terraform drift detection daily 03:00 UTC (`terraform plan` per region + Slack SEV-3 em diff > 0 + RB-FM-206 manual remediation gate, auto-apply forbidden); (5) progressive rollout controller 4-stage (1%→10%→50%→100%) com error-budget burn auto-rollback em ≤ 10 min p99 (error rate > baseline + 3σ OR SLO burn > 14.4 1h OR p99 > baseline + 50% sustained 5 min) e rollback consume ≤ 30% monthly budget máx (excedeu = freeze deploys); (6) audit emission CloudEvent rich per admin op (`actor + mfa_ts + dual_approver + op_payload + prev_state_hash + signature` chain integrity); (7) MFA freshness ≤ 30 min hard-check (CTRL-AUTH-010; expired = 401 force re-MFA, NÃO grace). Mitiga **FM-201** (config rate-limit drop), **FM-204** (secret rotation in-flight), **FM-205** (admin mistake — destructive ops sem 2 sigs), **FM-206** (terraform drift). Implementa CTRL-AUTH-010 + CTRL-AUDIT-003 + CTRL-CRED-003 + ratifica INV-ADMIN-DUAL-APPROVAL + INV-ADMIN-MFA-FRESHNESS (registry §3.12) + reforça INV-KEY-OVERLAP (§3.13) + INV-AUDIT-APPEND-ONLY (§3.6).

**Por que SOTA**: competitors (BuildBuddy, NativeLink) operam admin plane simplificado — single ownership, sem dual-approval, sem progressive rollout, sem rotation overlap canonical. CoreLink S-13 entrega pattern de operations engineering enterprise — cada ato administrativo é cryptographically attested + audit-logged + reversível em < 10 min. Reference: **Google SRE Workbook Ch 16** (canarying releases), **AWS Cell-based architecture** (progressive rollout), **NIST SP 800-53 Rev 5 AC-2(7)** (separation of duties + collusion-rotation), **HashiCorp Vault** (secret rotation reference).

## 2. Escopo

### 2.1 In-scope

- **WI-S13-001**: DO `config-singleton` per-region + schema versioned (feature flags + rate limits + retention policies) + CAS atomic update + propagation pub-sub ≤ 5s edge global + rollback endpoint `POST /v1/admin/config/rollback?to_version=X` + D1 `config_change_log` 90d retention.
- **WI-S13-002**: Admin API endpoint `POST /v1/admin/ops` + dual-approval enforcement (header `X-Dual-Approver` + HMAC signature verify + D1 separation-of-duties check `caller≠approver` + collusion-rotation 3-cycle anti-A↔B defense per NIST AC-2(7)) + audit CloudEvent emission rich + chaos test missing approver → 403 hard-fail.
- **WI-S13-003**: Secret rotation worker (TDK + PAT signing keys + audit chain key + BYOK) zero-downtime + per-asset adapter respeitando `key_management.md §3.2.1` overlap canonical (7d / 24h / 24h / 7d) + PAT-ROLL-FORWARD-001 auto-rollback se downstream errors > 1% + métricas observability.
- **WI-S13-004**: Terraform drift detection GitHub Action daily cron 03:00 UTC + `terraform plan` per region + diff alert Slack SEV-3 + RB-FM-206 runbook dry-run + manual remediation gate (auto-apply forbidden).
- **WI-S13-005**: Progressive rollout controller 4-stage (1% → 10% → 50% → 100%) + auto-rollback gating (error rate > baseline + 3σ OR SLO burn > 14.4 1h OR p99 > baseline + 50% sustained 5 min) + PAT-PROGRESSIVE-ROLLOUT-001 + rollback budget 30% monthly cap + chaos test bad deploy injection → auto-rollback ≤ 10 min.
- **WI-S13-006**: Property tests 10k iter (dual-approval + collusion-rotation + MFA freshness + rotation overlap per asset class) + RB-FM-205 (admin mistake) dry-run + RB-FM-201 (config rate-limit drop) dry-run + adversarial summary aggregation + PRR doc S-13 com 11 sign-offs canonical.

### 2.2 Anti-scope

- UI admin panel (S-16 frontend admin UI; S-13 ships API + CLI only).
- Customer-facing feature flags (out of scope; S-13 = internal flags only).
- A/B testing framework (S-13 lança feature flags binárias; A/B em S-19+).
- Capacity planning automation (terraform handles infrastructure; capacity em S-17 chaos).
- Cost optimization automation (FinOps tier — pós-GA).
- Multi-cloud config federation (Cloudflare-only at GA).
- Self-service customer admin features (S-16 + S-19 onboarding).
- Admin SSO IdP integration (Clerk admin role já cobre S-03 baseline; advanced SSO/SCIM em S-19 enterprise).
- Hardware-backed admin signing (HSM, YubiKey) — overhead operacional desproporcional vs WebAuthn admin step-up (CTRL-AUTH-010).

## 3. Customer Impact & Journey

**JTBD:** "Como SRE on-call em prospect enterprise, preciso evidência verificável de que **nenhum admin (incluindo um único Engineer rogue ou par A↔B colludente)** pode causar dano irreversível: config change tem rollback ≤ 5 min; destructive op exige 2 distinct approvers + collusion-rotation defense; secret rotation é zero-downtime overlap canonical. Como auditor SOC 2 / ISO 27001, preciso attestation que separation of duties (NIST AC-2(7)) é enforced em código (não policy), audit chain admin ops é unbroken, MFA freshness ≤ 30 min é hard-check (não grace)."

**CAPs entregues:** CAP-ADMIN-001 (DO config-singleton) + CAP-ADMIN-002 (dual-approval) + CAP-ADMIN-003 (secret rotation) + CAP-ADMIN-004 (terraform drift) + CAP-ADMIN-005 (progressive rollout) + CAP-ADMIN-006 (admin op audit-rich) + CAP-ADMIN-007 (config rollback API).

**Persona 1 — SRE on-call em prospect enterprise (RFP evaluation)**:
- Evidence pack inclui: `corelink_admin_dual_approval_total{outcome="ok"}` ratio; rotation overlap reports per asset class (TDK 7d staging sustained); progressive rollout chaos test runs (bad deploy injection → auto-rollback ≤ 10 min p99); config rollback drill report (T-7d version recovery ≤ 5 min).
- Diferenciador competitivo: 95%+ OSS Rust SaaS opera admin plane single-engineer; CoreLink S-13 = pattern Google SRE Workbook Ch 16 + AWS Cell-based rollout.

**Persona 2 — Auditor SOC 2 Type II / ISO 27001**:
- CTRL-AUTH-010 (MFA + session binding) + CTRL-AUDIT-003 (MFA attestation embedded em audit) + CTRL-CRED-003 (token rotation enforcement) attestation dossier; PRR doc S-13 11 sign-offs canonical; rotation overlap evidence per asset class + property test 10k green; admin op audit chain integrity verified daily 30d clean.
- Compliance Matrix mapping: SOC 2 CC6.1 (logical access controls + MFA), CC6.7 (change management), CC6.8 (system monitoring), CC7.1 (anomaly detection), CC8.1 (system change management); ISO 27001 A.5.15 (privileged access), A.5.16 (identity management), A.8.5 (secure authentication); NIST SP 800-53 AC-2(7) (separation of duties + collusion-rotation), AC-6(1) (least privilege), AU-2 (event logging).

**Persona 3 — Engineer onboarding em CoreLink**:
- `docs/internal/admin-plane.md` explica 7 CAPs + audit event schema + dual-approval flow + rotation overlap canonical.
- ADRs ratificadas: ADR-0018 (key overlap per asset) reused; novos forward ADRs documentam dual-approval HMAC signing key + progressive rollout error-budget gating.
- Runbooks RB-FM-201 + RB-FM-205 + RB-FM-206 dry-run reports committed.

**SLA addendum**:
- Config flag toggle propagação edge global ≤ 5s p99 (synthetic test).
- Dual-approval verify latency ≤ 50ms p99.
- Secret rotation overlap respeitado per asset class (TDK 7d / PAT 24h / audit 24h / admin signing 24h / BYOK 7d).
- Bad deploy detection p99 ≤ 10 min; rollback p99 ≤ 5 min.
- Config rollback to T-7d version ≤ 5 min p99 (drill).
- Auto-rollback consume ≤ 30% monthly error budget (excedeu = freeze deploys).
- MFA freshness window 30 min hard-check; expired = 401 force re-MFA.
- Audit chain integrity admin ops daily verify; break = SEV-2.

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4`. Foundation: `security_model.md §6.1` (CTRL-AUTH-010) + `security_model.md §6.8` (CTRL-AUDIT-003) + `security_model.md §6.11` (CTRL-CRED-003) + `key_management.md §3.2.1` (overlap canonical per asset) + `resilience_patterns.md §3.7` (PAT-DUAL-APPROVAL-001 + PAT-ROLL-FORWARD-001 + PAT-PROGRESSIVE-ROLLOUT-001 + PAT-DRIFT-DETECTION-001) + `invariant_registry.md §3.12` (INV-ADMIN-DUAL-APPROVAL + INV-ADMIN-MFA-FRESHNESS) + `invariant_registry.md §3.13` (INV-KEY-OVERLAP).

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S13-D1 | DO config-singleton + flag CRUD + propagation + rollback | `crates/corelink-config-do/` + `crates/corelink-config-api/` | DO per-region; schema versioned + CAS atomic; propagação edge ≤ 5s; D1 `config_change_log` 90d; rollback `POST /v1/admin/config/rollback?to_version=X` ≤ 5 min |
| S13-D2 | Admin API + dual-approval enforcement + audit emission | `crates/corelink-admin-api/` + `crates/corelink-dual-approval/` | `POST /v1/admin/ops` enforce header `X-Dual-Approver` + HMAC; D1 separation check `caller≠approver`; collusion-rotation 3-cycle defense; missing approver = 403 hard-fail; audit CloudEvent emit rich; chaos test green |
| S13-D3 | Secret rotation worker (4 asset types) | `crates/corelink-rotation-worker/` + `crates/corelink-rotation-adapters/` | Per-asset adapter respeitando overlap canonical 5 asset types (TDK 7d / PAT 24h / audit 24h / admin signing 24h / BYOK 7d); PAT-ROLL-FORWARD-001 auto-rollback se downstream errors > 1%; métrica `corelink_admin_rotation_in_progress{asset_type, status}` emitting |
| S13-D4 | Terraform drift detection daily | `.github/workflows/terraform-drift.yml` + `specs/05_runbooks/RB-FM-206.md` | Daily cron 03:00 UTC; `terraform plan` per region; diff alert Slack SEV-3; RB-FM-206 runbook dry-run committed; auto-apply forbidden (manual gate) |
| S13-D5 | Progressive rollout controller + auto-rollback | `crates/corelink-rollout-controller/` + `infra/cf-gradual-deploy/` | 4-stage 1%→10%→50%→100%; gating error rate / SLO burn / p99 latency; auto-rollback ≤ 10 min p99; budget 30% monthly cap; chaos test bad deploy injection green |
| S13-D6 | Property tests + RB dry-runs + PRR | `crates/corelink-admin-proptest/` + `specs/04_sprints/S13/PRR-S13.md` + `specs/_audits/2026-XX-XX-rb-fm-205-dry-run.md` | 10k property test (dual-approval + collusion-rotation + MFA freshness + rotation overlap); RB-FM-205 + RB-FM-201 + RB-FM-206 dry-runs; PRR doc 11 sign-offs canonical |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Auth Model (herda `auth_model.md`)

- MFA freshness window 30 min hard-check (`auth_model.md §5` admin step-up); admin op middleware enforces `mfa_ts ≤ now - 30min` antes handler dispatch; expired = 401 + force re-MFA (no grace).
- WebAuthn UV=1 mandatory para admin step-up (INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN herdada S-03).
- Clerk session binding UA+IP+PKCE (CTRL-AUTH-010 herdada S-03).

### 6.2 Security Model (herda `security_model.md §6`)

- **CTRL-AUTH-010** (MFA + session binding para admin) — enforce em middleware admin-api; 30 min freshness window hard-check.
- **CTRL-AUDIT-003** (MFA attestation para admin ops) — WebAuthn signature embedded em CloudEvent audit record per admin op.
- **CTRL-CRED-003** (token rotation enforcement) — TDK 7d / PAT signing 24h / audit chain 24h / admin signing 24h / BYOK 7d enforced via rotation worker; ADR-0018 governs.
- **CTRL-AUDIT-001 + CTRL-AUDIT-002 + CTRL-AUDIT-005** herdadas (audit hash chain + write events ricos + 7y retention).

### 6.3 Key Management (herda `key_management.md §3.2.1`)

- Overlap canonical per asset (tabela 3.2.1):
  - PAT signing key: 24h (HMAC-SHA256; hybrid HMAC + Argon2id S-03 cycle 9 SEAL decision (a)).
  - Audit chain key (per-region): 24h.
  - TDK (tenant derivation): 7d.
  - BYOK customer CMK (CoreLink-side cache): 7d.
- Hard upper bound: 30d sem ADR + Security lead sign-off.
- INV-KEY-OVERLAP enforced via property test per asset class (10k iter, simula rotation start → completion verifica old + new ambos válidos para reads, writes apenas para new = INV-KEY-NO-SKIP).

### 6.4 Resilience Patterns (herda `resilience_patterns.md §3.7`)

- **PAT-DUAL-APPROVAL-001**: WI-S13-002 implementa primary; FM-201 + FM-205 mitigated.
- **PAT-ROLL-FORWARD-001**: WI-S13-003 implementa rotation overlap; FM-204 mitigated.
- **PAT-PROGRESSIVE-ROLLOUT-001**: WI-S13-005 implementa 4-stage com error-budget auto-rollback; FM-200 mitigated.
- **PAT-DRIFT-DETECTION-001**: WI-S13-004 implementa daily terraform plan; FM-206 mitigated.

### 6.5 Failure Modes (herda `failure_modes.md`)

- **FM-201** (config change causa rate-limit drop) — RB-FM-201 dry-run em WI-S13-006.
- **FM-204** (secret rotation quebra serviço) — PAT-ROLL-FORWARD-001 mitigation em WI-S13-003.
- **FM-205** (manual intervention apaga dado / admin mistake) — PAT-DUAL-APPROVAL-001 mitigation em WI-S13-002 + RB-FM-205 dry-run em WI-S13-006.
- **FM-206** (terraform drift) — PAT-DRIFT-DETECTION-001 em WI-S13-004 + RB-FM-206 dry-run.

### 6.6 Observability Model (herda `observability_model.md §3.1 + §4.1`)

Métricas underscored snake_case com label `plan` aplicável (per `observability_model.md §3.1` cardinality budget + §4.1 naming convention):

- `corelink_admin_config_propagation_seconds_bucket` (histogram p50/p95/p99 propagation latency edge global).
- `corelink_admin_config_cas_conflict_total{layer,plan}` (CAS version mismatch retries).
- `corelink_admin_config_rollback_total{outcome,plan}` (outcome ∈ ok|version_unknown|state_corrupt).
- `corelink_admin_dual_approval_total{outcome,plan}` (outcome ∈ ok|missing_approver|sig_invalid|caller_eq_approver|collusion_rotation_violation).
- `corelink_admin_mfa_freshness_total{outcome,plan}` (outcome ∈ ok|stale|missing).
- `corelink_admin_rotation_in_progress{asset_type,status}` (gauge; status ∈ pending|active|overlap|retired|rolled_back).
- `corelink_admin_rotation_total{asset_type,outcome}` (outcome ∈ ok|rolled_back|aborted|failed).
- `corelink_admin_rotation_overlap_seconds{asset_type}` (histogram; baseline against canonical table 3.2.1).
- `corelink_admin_terraform_drift_findings_total{region,severity}` (severity ∈ none|low|medium|high).
- `corelink_admin_rollout_stage_gauge{stage,outcome}` (stage ∈ s1pct|s10pct|s50pct|s100pct; outcome ∈ active|complete|rolled_back).
- `corelink_admin_rollout_auto_rollback_total{trigger,plan}` (trigger ∈ error_rate|slo_burn|p99_latency).
- `corelink_admin_rollout_budget_consumed_ratio{plan}` (gauge; alert > 0.30).
- `corelink_admin_audit_chain_break_total{region}` (counter; alert > 0).

### 6.7 SLOs

- **SLO-ADMIN-CONFIG-PROPAGATION** (novo; adicionar slo_catalog em S-13): config flag toggle propagação edge global ≤ 5s p99 sustained 30d staging.
- **SLO-ADMIN-DUAL-APPROVAL-LATENCY** (novo): dual-approval verify latency ≤ 50ms p99.
- **SLO-ADMIN-ROTATION-OVERLAP** (novo): rotation overlap respeitado per asset class (canonical table 3.2.1) verified em property test verde.
- **SLO-ADMIN-ROLLBACK-RECOVERY** (novo): config rollback to T-7d ≤ 5 min p99; bad deploy auto-rollback ≤ 10 min p99.

## 7. Definition of Done (lane HIGH_RISK)

> **Two-phase SEAL** (per Timeline §9): items verificáveis instantaneamente fecham em **Implementation SEAL D+15**; items requerendo "sustained 30d staging" janela (CTRL-AUDIT-003 30d clean MFA attestation, audit chain integrity 30d clean, progressive rollout 30d sustained chaos test, config rollback monthly drill) fecham em **GA Evidence Gate SEAL D+45**. Ambos SEALs canonicos; sprint considerado concluído apenas após GA Evidence Gate D+45.

- [ ] **WIs SEALED**: 6/6 (EVT-031).
- [ ] **Feature flag** toggle em DO reflete em ≤ 5s edge globalmente (synthetic test) (EVT-018).
- [ ] **Dual-approval**: admin op sem 2 signatures bloqueada + logged (chaos test) (EVT-013).
- [ ] **Secret rotation**: 1 TDK rotated em staging sem downtime sustained 7d overlap (EVT-024).
- [ ] **Terraform drift**: injected drift detected em próximo daily run (EVT-027).
- [ ] **Progressive rollout**: bad deploy auto-rollback em ≤ 10 min (chaos test simula error spike) (EVT-022 + EVT-023).
- [ ] **Config rollback**: rollback to T-7d version ≤ 5 min (drill) (EVT-027).
- [ ] **CTRL-AUDIT-003** (MFA attestation admin ops) enforced + audit chain integrity verified.
- [ ] **CTRL-AUTH-010** (MFA fresh ≤ 30min) enforced; expiry test passes.
- [ ] **INV-KEY-OVERLAP** (key_management §3.2.1 + ADR-0018) — rotation overlap per asset class (TDK 7d / PAT 24h / audit 24h / admin signing 24h / BYOK 7d; 5 asset classes) verified em property test (EVT-002).
- [ ] **INV-ADMIN-DUAL-APPROVAL** (registry §3.12) ratificada — property test 10k green incluindo collusion-rotation A→B/B→A scenario; CI gate ativo (EVT-022).
- [ ] **INV-ADMIN-MFA-FRESHNESS** (registry §3.12) ratificada — middleware enforced; expiry test green (EVT-022).
- [ ] **PRR HIGH_RISK** 11 sign-offs canonical (per framework §33.5.4.3 + ADR-0034): Owner + Final Approver + Architect (Crypto SME specialization mandatory para secret rotation cripto-load-bearing + dual-approval HMAC signing key) + Security Lead + SRE Lead + Engineer (S-13 lead) + QA Lead + Product + Compliance Officer + Privacy Officer + AppSec advisor (EVT-031).
- [ ] **Runbook dry-run**: RB-FM-205 (admin mistake) + RB-FM-201 (config rate-limit drop) + RB-FM-206 (terraform drift) (EVT-017).
- [ ] **10.s13.1** CTRL-AUDIT-003 (MFA attestation admin ops) enforced + 30d clean staging *(GA Evidence Gate D+45)*.
- [ ] **10.s13.2** RB-FM-205 (admin mistake) dry-run executed (EVT-017).
- [ ] **10.s13.3** Dual-approval property test: 10k attempts variando approver/signature/timing → 0 bypasses (EVT-002).
- [ ] **10.s13.4** Secret rotation overlap 7d sustained TDK rotation com 0 read failures from in-flight reads *(GA Evidence Gate D+45)*.
- [ ] **10.s13.5** Progressive rollout chaos test: bad deploy injection → auto-rollback ≤ 10 min sustained 30d staging *(GA Evidence Gate D+45)*.
- [ ] **10.s13.6** Config rollback drill: monthly rollback drill executado + report *(GA Evidence Gate D+45)*.
- [ ] **10.s13.7** Audit chain integrity verified daily 30d clean (admin ops chain unbroken) *(GA Evidence Gate D+45)*.
- [ ] **Cost regression gate**: admin plane infra ≤ $200/mês (DO + D1 history + Slack notifications + GitHub Actions terraform plan) (EVT-002).
- [ ] **Métricas underscored Prometheus**: 13+ admin plane metrics emitting em staging (`corelink_admin_*`) com label `plan` aplicável (EVT-013).

## 8. Dependencies

### Hard blockers

- **S-03 SEALED** (auth real para MFA + admin role + WebAuthn step-up; audit pre/post emission Tower middleware).
- **S-09 SEALED** (observability para audit chain + métricas underscored Prometheus + DASH-SUPPLY pattern).

### Soft blockers

- **S-12 SEALED recomendado** (Cosign deploy verify para progressive rollout reproducibility — release sem signature blocked em deploy webhook).

### Outbound

- **S-10** (billing usage dashboard consume admin plane config-singleton para retention policies tunables; S-10 declara S-13 outbound consumer).
- **S-14** (BYOK rotation usa secret rotation framework + customer-trigger overlap 7d).
- **S-16** (admin UI consume admin plane API + 7 CAPs).
- **S-17** (chaos testing usa progressive rollout para deploy chaos).
- **S-20** (GA exige RB-FM-205 + RB-FM-206 dry-run + audit chain integrity 30d clean).

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-03 + S-09 SEALED; preferentemente após S-12 SEALED para deploy verify foundation).
- **D+3**: WI-S13-001 SEALED (DO config-singleton + propagation + rollback API).
- **D+5**: WI-S13-002 SEALED (admin API + dual-approval enforcement + audit emission + collusion-rotation defense).
- **D+9**: WI-S13-003 SEALED (secret rotation 5 asset types + PAT-ROLL-FORWARD-001).
- **D+11**: WI-S13-004 + WI-S13-005 SEALED (terraform drift + progressive rollout + auto-rollback).
- **D+13**: WI-S13-006 SEALED (property tests 10k + RB dry-runs + PRR).
- **D+15**: Sprint review + sign-offs + **Implementation SEAL ceremony** (todos WIs entregues + tooling em produção + zero P0/P1 abertos + 11 sign-offs canonical PRR coletados).
- **D+15..D+45**: **Observation window (30d sustained evidence)** — CTRL-AUDIT-003 30d clean MFA attestation; audit chain integrity 30d clean; rotation overlap 7d sustained TDK staging; progressive rollout chaos test 30d staging; config rollback monthly drill executado. Métricas coletadas continuously; nenhum WI re-aberto exceto fix-critical.
- **D+45**: **GA Evidence Gate SEAL** (sprint sign-off final) — DoD ship-gate criteria validados com janela 30d real (não-simulada); audit chain green streak, MFA freshness sustained, rotation overlap sustained, progressive rollout chaos sustained todos comprovados via DASH-ADMIN. Implementation já SEALED em D+15; este gate libera S-14 + S-16 dependencies + S-20 GA dependency.
- **Total**: 2.5 semanas implementação (12 dias úteis) + 30d observation window + GA Evidence Gate D+45.

## 10. Risk Register

Ver `_spec_contract.md §15` (10 risks 6-col com Owner per item: config flag race condition, dual-approval bypass, secret rotation in-flight breakage, auto-rollback failure, terraform drift unexpected behavior, progressive rollout false-positive, MFA bypass via timestamp manipulation, approver collusion, config rollback corrupts state, audit chain break em admin ops).

## 11. Observability Plan

DASH-ADMIN (novo dashboard):
- Config flag propagation latency p99 (per region) — gauge over time.
- CAS conflict rate (concurrent updates retries).
- Config rollback success ratio + recovery time.
- Dual-approval pass/block ratio + reason breakdown.
- MFA freshness pass/stale ratio.
- Secret rotation in-progress per asset type (active/overlap/retired states).
- Rotation overlap window per asset class (vs canonical table 3.2.1).
- Terraform drift findings per region (daily trend 30d).
- Progressive rollout stage gauge + auto-rollback events.
- Rollout budget consumed ratio (alert > 30%).
- Audit chain integrity (daily verifier success + break alert).

Métricas listadas em §6.6 (13+); todas com label `plan` aplicável + cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (≤ 20k séries únicas per métrica; ≤ 100k total).

## 12. Security & Privacy

**STRIDE delta** (vs S-03 + S-09 baseline):
- **Spoofing**: dual-approval HMAC signature + caller≠approver D1 hard-check + collusion-rotation 3-cycle defense; admin role verified via Clerk JWT RS256 + WebAuthn UV=1 step-up; MFA freshness ≤ 30 min hard-check.
- **Tampering**: DO config CAS atomic version-aware (concurrent updates rejected); audit chain hash integrity unbroken (INV-AUDIT-APPEND-ONLY); rotation worker idempotent.
- **Repudiation**: audit CloudEvent rich (`actor + mfa_ts + dual_approver + op_payload + prev_state_hash + signature` chain); WebAuthn attestation embedded (CTRL-AUDIT-003); 7y retention (CTRL-AUDIT-005).
- **Information disclosure**: config payload pseudo-public (feature flags + tunables não contêm secrets; secrets em separate KMS); rotation worker logs redacted (no key bytes); terraform plan diff Slack inclui resource names mas não secrets.
- **DoS**: progressive rollout 4-stage limita blast radius; auto-rollback ≤ 10 min mitiga bad deploy; rotation overlap canonical previne in-flight breakage.
- **Elevation of privilege**: admin role least-privilege (CTRL-AUTHZ-001); dual-approval enforce em destructive ops; collusion-rotation eleva barra contra 2-engineer reciprocal approval; auto-apply forbidden em terraform (manual gate).

**LINDDUN delta**:
- **Linkability**: admin actor + mfa_ts em audit é necessário (compliance); pseudonymization não aplicável (admin é internal employee + accountable).
- **Identifiability**: admin user_id + email em audit (intencional; CTRL-AUDIT-002); LGPD Art. 7º legitimate interest (employer's right to monitor).
- **Non-repudiation**: WebAuthn attestation = forensic-grade evidence; audit chain Merkle = tampering detectable.
- **Detectability**: dependent destructive ops publicly tracked em audit (internal); collusion-rotation anomaly detection via D1 query last 24h ops.
- **Disclosure of information**: config payload public para internal team via D1 history; secrets nunca em config-singleton (separate KMS path).
- **Unawareness**: admin operations procedure documented em runbook + onboarding training.
- **Non-compliance**: SOC 2 CC6.1/CC6.7/CC6.8/CC7.1/CC8.1 + ISO 27001 A.5.15/A.5.16/A.8.5 + NIST SP 800-53 AC-2(1)/AC-2(7)/AC-6(1)/AU-2 + LGPD Art. 38 + GDPR Art. 32 satisfied.

## 13. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Dual-approval bypass detected (any bypass attempt) → CRITICAL post-mortem + Security review.
- Secret rotation falhada com downstream errors > 1% → 5-Why obrigatório.
- Auto-rollback false-positive (rollback de deploy bom) > 1× mês → review thresholds + post-mortem.
- Progressive rollout bypass (deploy 100% direto sem stages) → post-mortem + privilege review.
- Audit chain break em admin ops → CRITICAL post-mortem + compliance officer.
- Terraform drift > 7d sem remediation → post-mortem + drift discipline review.
- MFA bypass via timestamp manipulation detected → CRITICAL post-mortem + auth model review.
- Approver collusion detected (anti-rotation 3-cycle violation) → CRITICAL post-mortem + access review + Security incident response.
- Config rollback corrompe state (interaction com runtime invariants) → SEV-2 + post-mortem.

## 14. Sign-off (HIGH_RISK 11 canonical)

11 roles per framework §33.5.4.3 + ADR-0034: Owner + Final Approver + Architect (com Crypto SME specialization mandatory para secret rotation cripto-load-bearing — TDK 7d / PAT 24h / audit 24h / admin signing 24h / BYOK 7d overlap (5 asset classes); dual-approval HMAC signing key derivation; audit chain hash integrity admin ops) + Security Lead + SRE Lead + Engineer (S-13 lead) + QA Lead + Product + Compliance Officer + Privacy Officer + AppSec advisor. Crypto SME folds into Architect role specialization (precedent: S-12 SLSA L3 + Cosign keyless OIDC + Rekor inclusion review folded into Architect). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect).

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-13 (cycle 12.S13.0). |

---

**Fim de S-13 sprint contract.**
