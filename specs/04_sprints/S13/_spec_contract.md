---
id: "SPEC-CONTRACT-S13"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s13", "admin-plane", "config", "secret-rotation", "dual-approval", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-13: Admin Plane (Config + Feature Flags + Secret Rotation + Progressive Rollout)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-13 |
| Nome | Admin Plane |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-005 (CTRL-CRED-003 + CTRL-AUTH-010 + CTRL-AUDIT-003 — secret rotation + admin MFA + dual-approval) |
| Duração estimada | 2.5 semanas |
| WIs antecipados | 6 |
| SOTA target | Admin plane com defense-in-depth — DO config singleton + dual-approval enforced + secret rotation zero-downtime + progressive rollout auto-rollback |

## 1. Objetivo

Implementar **admin plane defense-in-depth** que torna operações internas auditáveis, dual-approval-gated e reversíveis: DO `config-singleton` para feature flags + policies + tunables, dual-approval workflow PAT-DUAL-APPROVAL-001 para destructive ops (sem 2 sigs = block), secret rotation automation cross-asset (TDKs, PAT signing keys, audit chain key, BYOK), terraform drift detection daily, progressive rollout 1%→10%→50%→100% com auto-rollback baseado em error budget burn. Bypass desses controles é ataque interno = blast radius global.

**Por que SOTA:** competitors (BuildBuddy, NativeLink) têm admin plane simplificado: single ownership, sem dual-approval, sem progressive rollout. CoreLink S-13 entrega pattern de operations engineering enterprise: cada ato administrativo é cryptographically attested + audit-logged + reversible em < 10 min. Reference: **Google's CHANGE management practices** (SRE Workbook Ch 16), **AWS Cell-based architecture progressive rollout**.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (10–12 sign-offs).
- **FF-HR-005**: implementa CTRL-CRED-003 (secret rotation), CTRL-AUTH-010 (admin MFA), CTRL-AUDIT-003 (admin op audit). Bypass = blast radius global.
- **Justificativa upgrade STANDARD → HIGH_RISK**: codex audit findings 2026-04-24 indica controles security-critical; gap entre lane STANDARD original e content já HIGH_RISK em substância.

## 3. Inherits_from

```yaml
inherits_from:
  - "SECURITY-MODEL"            # CTRL-AUTH-010, CTRL-CRED-003, CTRL-AUDIT-003
  - "RESILIENCE-PATTERNS"       # PAT-DUAL-APPROVAL-001, PAT-ROLL-FORWARD-001, PAT-PROGRESSIVE-ROLLOUT-001, PAT-DRIFT-DETECTION-001, PAT-AUTO-ROLLBACK-001
  - "KEY-MANAGEMENT"            # rotation overlap policy
  - "OBSERVABILITY-MODEL"       # admin ops audit + métricas
  - "FAILURE-MODES"             # FM-205 (admin mistake), FM-201 (config rate-limit drop), FM-204 (rotation in-flight)
  - "INVARIANT-REGISTRY"        # INV-KEY-OVERLAP, INV-AUDIT-APPEND-ONLY
  - "AUTH-MODEL"                # MFA enforcement
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-ADMIN-001** | DO config-singleton | Feature flags tipados + rate limits tunáveis + retention policies; CAS version-aware updates atomic. |
| **CAP-ADMIN-002** | Dual-approval workflow | PAT-DUAL-APPROVAL-001 enforcement; admin op sem 2 signatures = 403; audit append. |
| **CAP-ADMIN-003** | Secret rotation automation | CTRL-CRED-003 + PAT-ROLL-FORWARD-001 zero-downtime rotation TDKs/PAT keys/audit chain key/BYOK. |
| **CAP-ADMIN-004** | Terraform drift detection daily | PAT-DRIFT-DETECTION-001; daily plan + diff alert se IaC vs runtime drift. |
| **CAP-ADMIN-005** | Progressive rollout orchestrator | PAT-PROGRESSIVE-ROLLOUT-001; 1%→10%→50%→100% com error budget burn auto-rollback. |
| **CAP-ADMIN-006** | Admin op audit-rich event | CloudEvent emission per admin op com `actor, MFA_ts, dual_approver, op_payload, prev_state_hash`. |
| **CAP-ADMIN-007** | Config rollback API | Time-travel rollback config para qualquer versão últimos 90d; ≤ 5min recovery. |

## 5. Requirements específicos

### 5.1 Config Singleton (CAP-ADMIN-001 + CAP-ADMIN-007)

- **R-S13-1**: DO `config-singleton` per-region com schema versioned:
  - Feature flags: `Map<feature_id, {enabled: bool, rollout_pct: 0-100, allowlist_tenants: [uuid]}>`.
  - Rate limits tunáveis: `Map<{layer, tier}, {refill_rate, burst}>`.
  - Retention policies: `Map<resource, ttl_days>`.
  - Update via CAS `(version, payload)` atomic; reject se `version != current`.
- **R-S13-2**: Config propagação ≤ 5s edge globalmente; per-Worker subscribe to DO change events.
- **R-S13-3**: Config history retention 90d em D1 `config_change_log`; rollback API `POST /v1/admin/config/rollback?to_version=X` ≤ 5 min.

### 5.2 Dual-Approval (CAP-ADMIN-002)

- **R-S13-4**: Endpoint `POST /v1/admin/ops` exige header `X-Dual-Approver: <user_id>` + `X-Approver-Signature: <hmac>` em adição ao caller MFA.
- **R-S13-5**: Hard-check (não advisory): missing approver → 403 + audit emit `admin.op.dual_approval_missing`.
- **R-S13-6**: Approver não pode ser o caller (separation of duties); enforced via D1 check. **Lote 9.4 Opus H-08 collusion-rotation defense**: além de `caller ≠ approver`, enforcement adicional: nas últimas 3 destructive ops em janela de 24h, deve haver ≥ 3 distinct approvers (anti-collusion-rotation pattern AC-2(7) NIST SP 800-53). Property test cobre cenário de A→approve B / B→approve A loop em sequence.

### 5.3 Secret Rotation (CAP-ADMIN-003)

- **R-S13-7**: Secret rotation worker para 4 asset types; overlap conforme tabela canonical em `key_management.md §3.2.1` (ADR-0018):
  - **TDK** (tenant derivation keys, S-01) — overlap 7d (CTRL-KEY-002).
  - **PAT signing keys** (S-03) — overlap 24h.
  - **Audit chain key** (S-09 R-S09-10) — overlap 24h.
  - **BYOK** (S-14, customer-driven trigger) — overlap 7d.
- **R-S13-8**: Rotation observability: métrica `corelink.admin.rotation_in_progress{asset_type, status}`.
- **R-S13-9**: Rotation rollback: se downstream errors > 1% durante rotation, auto-rollback PAT-ROLL-FORWARD-001.

### 5.4 Terraform Drift (CAP-ADMIN-004)

- **R-S13-10**: GitHub Action daily cron 03:00 UTC: `terraform plan` para todas as regiões; if diff > 0 → SEV-3 alert + Slack message com diff.
- **R-S13-11**: Manual drift remediation runbook RB-FM-206 (terraform-drift); auto-apply forbidden (require human approval).

### 5.5 Progressive Rollout (CAP-ADMIN-005)

- **R-S13-12**: Progressive rollout controller: 4 stages (`1% → 10% → 50% → 100%`) com gating em error budget burn rate.
- **R-S13-13**: Auto-rollback triggers:
  - Error rate > baseline + 3× sigma.
  - SLO burn-rate > 14.4 (1h window).
  - p99 latency > baseline + 50%.
  - Any of these em 5 min sustained → auto-rollback to previous version + SEV-2 alert.
- **R-S13-14**: Bad deploy detection p99 ≤ 10 min; rollback p99 ≤ 5 min.

### 5.6 Audit (CAP-ADMIN-006)

- **R-S13-15**: Cada admin op emite CloudEvent rich:
  - `actor`: user_id + email.
  - `mfa_ts`: timestamp última MFA verification (within last 30 min — CTRL-AUTH-010).
  - `dual_approver`: user_id approver.
  - `op_payload`: full request body.
  - `prev_state_hash`: SHA-256 do state pre-op para rollback.
  - `signature`: HMAC do event (chain integrity).

## 6. Definition of Done

- [ ] **WIs SEALED**: 6/6.
- [ ] **Feature flag** toggle em DO reflete em ≤ 5s edge globalmente (synthetic test) (EVT-018).
- [ ] **Dual-approval**: admin op sem 2 signatures bloqueada + logged (chaos test) (EVT-013).
- [ ] **Secret rotation**: 1 TDK rotated em staging sem downtime sustained 7d overlap (EVT-024).
- [ ] **Terraform drift**: injected drift detected em próximo daily run (EVT-027).
- [ ] **Progressive rollout**: bad deploy auto-rollback em ≤ 10 min (chaos test simula error spike) (EVT-022 + EVT-023).
- [ ] **Config rollback**: rollback to T-7d version ≤ 5 min (drill) (EVT-027).
- [ ] **CTRL-AUDIT-003** (MFA attestation admin ops) enforced + audit chain integrity verified.
- [ ] **CTRL-AUTH-010** (MFA fresh ≤ 30min) enforced; expiry test passes.
- [ ] **INV-KEY-OVERLAP** (key_management §3.2.1 + ADR-0018) — rotation overlap per asset class (TDK 7d / PAT 24h / audit 24h / BYOK 7d) verified em property test (EVT-002).
- [ ] **PRR HIGH_RISK** (10–12 sign-offs): SRE lead + Security lead + Engineer + QA + Compliance officer + Product + 2 peers + Architect + AppSec + Privacy officer + Crypto SME (secret rotation review).
- [ ] **Runbook dry-run**: RB-FM-205 (admin mistake) + RB-FM-201 (config rate-limit drop) + RB-FM-206 (terraform drift) (EVT-017).

## 7. Completeness Criteria (delta local)

- [ ] **10.s13.1** CTRL-AUDIT-003 (MFA attestation admin ops) enforced + 30d clean staging.
- [ ] **10.s13.2** RB-FM-205 (admin mistake) dry-run executed (EVT-017).
- [ ] **10.s13.3** **Dual-approval property test**: 10k attempts variando approver/signature/timing → 0 bypasses.
- [ ] **10.s13.4** **Secret rotation overlap 7d** sustained TDK rotation com 0 read failures from in-flight reads.
- [ ] **10.s13.5** **Progressive rollout** chaos test: bad deploy injection → auto-rollback ≤ 10 min sustained 30d staging.
- [ ] **10.s13.6** **Config rollback drill**: monthly rollback drill executado + report.
- [ ] **10.s13.7** **Audit chain integrity** verified daily 30d clean (admin ops chain unbroken).

## 8. Invariants

### Mantidas

- **INV-KEY-OVERLAP** (HIGH — invariant_registry §3.13 + key_management §3.2.1 + ADR-0018): rotation overlap period respected per asset class.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL — herda S-09): admin op audit events append-only.
- **CTRL-AUTH-010 + CTRL-AUDIT-003 + CTRL-CRED-003**: mantidas via enforcement runtime.

### Novas (introduzidas por S-13 — adicionar a invariant_registry.md §3.12)

- **INV-ADMIN-DUAL-APPROVAL** (HIGH — registry §3.12): toda destructive admin op tem 2 distinct signatures (caller + approver); 0 bypasses em property test 10k attempts. **Lote 9.4 Opus H-08 strengthening**: collusion-rotation defense — últimas 3 destructive ops em 24h DEVE ter ≥ 3 distinct approvers (anti-A↔B rotation). **Why:** single-signature admin = insider threat trivial; 2-engineer collusion via reciprocal approval = bypass com 2 admins; rotation tracking eleva barra. **How to apply:** D1 hard-check + property test 10k including collusion-rotation scenario + audit emission + NIST SP 800-53 AC-2(7) alignment.
- **INV-ADMIN-MFA-FRESHNESS** (HIGH — novo): admin op exige MFA timestamp ≤ 30 min antes da request; expirado = 401 + force re-MFA. **Why:** stale MFA = persistent session hijack vector (CTRL-AUTH-010). **How to apply:** middleware check.

## 9. Quality Standards (delta local)

- **14.s13.1 Config mudanças versionadas em git** (audit trail externo) + D1 (real-time).
- **14.s13.2 Dual-approval é hard-check** (não social norm); enforced em código + property test + chaos test.
- **14.s13.3 Rollback p99 ≤ 5 min** (config) e ≤ 10 min (deploy bad).
- **14.s13.4 Separation of duties**: approver ≠ caller; enforced D1 + audit.
- **14.s13.5 MFA freshness window 30 min**: admin op rejeita se MFA timestamp > 30 min stale.
- **14.s13.6 Audit chain integrity admin ops**: hash chain verifica daily; break = SEV-2.
- **14.s13.7 Auto-rollback budget**: rollback consome 30% do error budget mensal max; > 30% = freeze deploys.

## 10. Anti-scope

- ❌ UI admin panel (S-16 frontend admin UI).
- ❌ Customer-facing feature flags (out of scope; internal only).
- ❌ A/B testing framework (S-13 lança feature flags binárias; A/B é S-19+).
- ❌ Capacity planning automation (terraform handles infrastructure; capacity is S-17 chaos).
- ❌ Cost optimization automation (FinOps tier — pós-GA).
- ❌ Multi-cloud config federation (Cloudflare-only at GA).
- ❌ Self-service customer admin features (S-16 + S-19 onboarding).

## 11. Dependencies

### Hard blockers

- **S-03 SEALED** (auth real para MFA + admin role).
- **S-09 SEALED** (observability para audit + métricas).

### Soft blockers

- **S-12 SEALED** (Cosign deploy verify para progressive rollout reproducibility).

### Outbound

- S-10 (billing usage dashboard consome admin plane config-singleton; S-10 declara S-13 como outbound consumer).
- S-14 (BYOK rotation usa secret rotation framework).
- S-16 (admin UI consome admin plane API).
- S-17 (chaos testing usa progressive rollout para deploy chaos).
- S-20 (GA exige RB-FM-205 + RB-FM-206 dry-run).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S13-001** | DO config-singleton + flag CRUD + propagation + rollback API | DO setup; schema; CAS update; propagation pub-sub; rollback endpoint; D1 history 90d | 14h | 22h | 36h | **23.0h** |
| **WI-S13-002** | Admin API + dual-approval enforcement + audit emission | endpoints; X-Dual-Approver header; HMAC verify; D1 separation check; CloudEvent emit; chaos test | 12h | 18h | 30h | **19.0h** |
| **WI-S13-003** | Secret rotation worker (TDKs + PAT keys + audit chain + BYOK) | rotation framework; per-asset adapter; overlap policy; rollback PAT-ROLL-FORWARD-001; metrics | 16h | 24h | 40h | **25.3h** |
| **WI-S13-004** | Terraform drift detection daily + RB-FM-206 dry-run | GitHub Action cron; terraform plan; diff alert Slack; runbook dry-run; manual remediation flow | 8h | 12h | 20h | **12.7h** |
| **WI-S13-005** | Progressive rollout controller + auto-rollback + error budget integration | controller; 4 stages; gate criteria; auto-rollback PAT-AUTO-ROLLBACK-001; chaos test | 14h | 22h | 36h | **23.0h** |
| **WI-S13-006** | Property tests dual-approval + MFA freshness + rotation overlap + RB-FM-205 dry-run + PRR | property test 10k; MFA expiry test; rotation overlap test; runbook dry-run; PRR doc | 10h | 14h | 22h | **14.7h** |

**Total PERT:** ~118h ≈ 15 dias work × 1 eng. Buffer 3 dias confere com 2.5 semanas.

## 13. Duração + Timeline

- **Duração:** 2.5 semanas (12 dias úteis) + buffer 3 dias.
- **Marcos:**
  - **D+3:** WI-001 SEALED (config singleton + propagation).
  - **D+5:** WI-002 SEALED (dual-approval enforcement).
  - **D+9:** WI-003 SEALED (secret rotation 4 asset types).
  - **D+11:** WI-004 + WI-005 SEALED (terraform drift + progressive rollout).
  - **D+13:** WI-006 SEALED (property tests + runbook + PRR).
  - **D+15:** Sprint review + sign-offs.

## 14. Critérios de promoção

- DoD complete + 30d staging com 3 admin ops reais executados sem incident.
- INV-ADMIN-DUAL-APPROVAL property test verde.
- Secret rotation overlap 7d staging sustained.
- Progressive rollout chaos test passed.
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Config flag race condition** (concurrent updates) | M | M | MEDIUM | M | LOW | CAS atomic + version check + property test 10k concurrent updates 0 lost. |
| **Dual-approval bypassed via bug** | L | M | CRITICAL (insider threat) | M | LOW | Hard-check enforced + property test + chaos test + audit chain detection. |
| **Secret rotation quebra em-flight** (FM-204) | M | M | HIGH | M | LOW | Overlap window 7d for TDK + PAT-ROLL-FORWARD-001 rollback + RB stub. |
| **Auto-rollback falha** → bad deploy permanece | L | M | HIGH | L | LOW | Manual override `wrangler rollback` available + SEV-2 alert immediate + chaos test. |
| **Terraform drift causa unexpected behavior** (FM-206) | M | M | MEDIUM | M | LOW | Daily detection + Slack alert + RB-FM-206 + manual remediation gate. |
| **Progressive rollout false-positive auto-rollback** (deploy ok, auto-rollback wrong) | M | L | MEDIUM (operational) | L | LOW | Auto-rollback budget 30% error budget; manual override available; refine gating thresholds. |
| **MFA bypass via timestamp manipulation** | L | H | CRITICAL | M | LOW | MFA timestamp signed by IdP + clock-skew tolerance ≤ 60s; CTRL-AUTH-010. |
| **Approver collusion** (caller + approver same person via privilege escalation) | L | H | CRITICAL | M | LOW | Separation of duties enforced D1; quarterly access review; audit anomaly detection. |
| **Config rollback corrupts state** (interaction com runtime invariants) | L | M | HIGH | L | LOW | Pre-rollback simulation + rollback drill monthly + manual override. |
| **Audit chain break em admin ops** | L | H | HIGH (compliance) | M | LOW | INV-AUDIT-APPEND-ONLY + INV-OBS-AUDIT-CHAIN-INTEGRITY + daily verify. |

## 16. Benchmarks SOTA externos

| Critério | Google internal | AWS/Stripe | OneTrust admin | **CoreLink target S-13** |
|---|---|---|---|---|
| Dual-approval enforced | Yes | Yes (Stripe Connect) | No | **Yes — hard-check + property test 10k** |
| Secret rotation zero-downtime | Yes | Yes | Manual | **Yes — overlap 7d TDK + PAT-ROLL-FORWARD-001** |
| Progressive rollout 4-stage | Yes | Yes | No | **Yes — 1%/10%/50%/100% + auto-rollback** |
| Terraform drift detection | Yes | CloudFormation drift | No | **Yes — daily plan + Slack alert** |
| Config rollback ≤ 5 min | Internal | Internal | No | **Yes — D1 history 90d + atomic CAS** |
| Admin MFA freshness window | Yes | Yes | Yes | **Yes — 30 min hard-check** |
| Audit chain integrity admin ops | Internal | Yes | Yes | **Yes — INV-OBS-AUDIT-CHAIN-INTEGRITY daily** |

**Veredito SOTA:** S-13 v1.1 atinge feature parity com Google internal + AWS em 7/7 dimensões.

## 17. References (RFCs, papers, standards)

- **Google SRE Workbook Ch 16** — Canarying releases.
- **AWS Cell-based Architecture** — progressive rollout pattern.
- **NIST SP 800-53 Rev 5 AC-2(1)** — separation of duties.
- **NIST SP 800-57 Pt 1 Rev 5** — Recommendation for Key Management (rotation policy).
- **OWASP ASVS V14** — config and deployment.
- **The Twelve-Factor App** — config management principles.
- **HashiCorp Vault** — secret rotation reference architecture.
- **CloudEvents v1.0.2** — audit event spec.

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Dual-approval bypass detected (any bypass attempt) → CRITICAL post-mortem + Security review.
- Secret rotation falhada com downstream errors > 1% → 5-Why obrigatório.
- Auto-rollback false-positive (rollback de deploy bom) > 1× mês → review thresholds + post-mortem.
- Progressive rollout bypass (deploy 100% direto sem stages) → post-mortem + privilege review.
- Audit chain break em admin ops → CRITICAL post-mortem + compliance officer.
- Terraform drift > 7d sem remediation → post-mortem + drift discipline review.

## 19. Waiver policy

S-13 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ INV-ADMIN-DUAL-APPROVAL property test verde — security baseline.
- ❌ INV-ADMIN-MFA-FRESHNESS enforcement — auth baseline.
- ❌ Secret rotation 7d overlap TDK — INV-KEY-OVERLAP requirement.
- ❌ Audit chain integrity admin ops verified daily — compliance baseline.

Itens waivable com Security lead + SRE lead + ADR:

- ⚠️ MFA freshness window 30 min → 60 min para dev environments (não prod).
- ⚠️ Progressive rollout stages 4 → 3 (skip 50%) com explicit risk acceptance.
- ⚠️ Auto-rollback false-positive threshold 1×/mês → 2×/mês com root-cause analysis.

---

**Fim spec contract S-13 v1.1.0 SOTA.**
