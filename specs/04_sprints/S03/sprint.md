---
id: "S-03"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005", "FF-HR-009"]
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "KEY-MANAGEMENT"
  - "PRIVACY-MODEL"
  - "INVARIANT-REGISTRY"
  - "DATA-MODEL"
  - "COMPLIANCE-MATRIX"
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "SLO-CATALOG"
tags: ["sprint", "s03", "auth", "clerk", "pat", "webauthn", "high-risk"]
---

# Sprint S-03 — Auth Real (Clerk SSO + PAT lifecycle ≤ 60s revocation + WebAuthn MFA)

> **doc_status:** DRAFT · **lane:** HIGH_RISK · **Versão:** 1.0.0 · **2026-04-25**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (Lote 9.4 SOTA elevation)

> **🚦 Phase boundary:** Fase 1 — Remote Cache.

---

## 1. Objetivo

Substituir PAT stub de S-01/S-02 por auth real production-grade: Clerk SSO + PAT lifecycle (Argon2id hash; emit/use/revoke) + revocation propagation ≤ 60s p99 global cross-region + WebAuthn MFA obrigatório admin ops + audit trail EVT-047 CloudEvents. Destrava pagamento real S-10, BYOK enterprise S-14, self-service admin S-13/S-16.

## 2. Escopo

### 2.1 In-scope

- **WI-S03-001**: Clerk adapter (JWKS + JWT validate + clock-skew tolerance).
- **WI-S03-002**: Crate `corelink-pat` com Argon2id (OWASP 2024 params) + timing-safe verify.
- **WI-S03-003**: Tower middleware + TenantCtx injection + 5-layer defense propagation.
- **WI-S03-004**: Revocation DO + KV cache invalidation + cross-region propagation ≤ 60s.
- **WI-S03-005**: Neon schema `account/tenant/user_account/membership/pat` + column encryption.
- **WI-S03-006**: WebAuthn admin flows Level 3 + cross-browser test (Chrome/Firefox/Safari/Edge).
- **WI-S03-007**: Audit events emission EVT-047 + chain integrity S-09 alignment.
- **WI-S03-008**: Property test 10k revocation + pentest engagement + DSR PAT export + RB-FM-160 + PRR.

### 2.2 Anti-scope

- ❌ BYOK enterprise (S-14).
- ❌ SAML / enterprise IdP (Fase 2).
- ❌ UI admin panel (S-16).
- ❌ Billing integration (S-10).
- ❌ TOTP-only MFA (rejected; WebAuthn phishing-resistant).
- ❌ Magic link auth (rejected; phishing-prone).

## 3. Customer Impact & Journey

**JTBD:** "Como dev/admin de tenant, eu preciso de auth phishing-resistant (WebAuthn) + revocation rápida (≤ 60s) para garantir que credentials comprometidas não são abusadas; e como auditor, preciso de audit trail completo de todas as ops de auth."

**CAPs entregues:** CAP-AUTH-001..007 (Clerk SSO + PAT scopes + revocation + MFA WebAuthn + tenant provisioning + audit events + Argon2id).

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4`. Auth_model §8.1 5-layer defense — este sprint implementa camadas 1-2 reused S-01 + camada 3 (PAT scope check via middleware).

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S03-D1 | Clerk adapter | `crates/corelink-clerk/` | JWKS cache 24h KV; JWT validate clock-skew 60s |
| S03-D2 | Crate corelink-pat (Argon2id + scope + timing-safe) | `crates/corelink-pat/` | OWASP 2024 params; `subtle::ConstantTimeEq`; benchmark calibration ~250ms |
| S03-D3 | Tower middleware + TenantCtx | `crates/corelink-worker/src/auth/` | 5-layer defense propagation; integration test |
| S03-D4 | Revocation DO + KV invalidation | `crates/corelink-worker/src/auth/revocation.rs` | Cross-region ≤ 60s p99; chaos test |
| S03-D5 | Neon schema | `migrations/002_auth_tables.sql` | Account + Tenant + Membership + PAT + WebAuthn credential com pgcrypto column encryption |
| S03-D6 | WebAuthn flows | `crates/corelink-worker/src/auth/webauthn.rs` | Level 3 spec; passkey + YubiKey + platform; cross-browser CI |
| S03-D7 | Audit events EVT-047 | `crates/corelink-worker/src/audit/auth_events.rs` | 4 event types (issued/revoked/login/mfa_verified); chain hash S-09 |
| S03-D8 | Property + pentest + RB + PRR | `tests/prop_revocation.rs` + `PRR-S03.md` | Property tests: 10k iter PR (≤ 1 min) + 100k iter nightly (≤ 30 min orçamento; Argon2 sampled 1% para fit budget); pentest clean; RB-FM-160 dry-run; PRR 13 sign-offs (12 mandatory + 1 advisory Crypto SME) |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Auth (herda `auth_model.md §3 + §5 + §8.1`)

- PAT format: `corelink_<env>_<base64url(32 bytes)>`.
- Argon2id params OWASP 2024: m=65536, t=3, p=4.
- 5-layer defense propagation: PAT scope → tenant prefix derivation → AuthZ check → R2 key prefix → audit cross-check.
- **Delta local**: PAT signing key 24h overlap (per ADR-0018 + key_management §3.2.1).

### 6.2 Crypto (herda `key_management.md §3.13`)

- TDK reused S-01 para tenant prefix derivation.
- PAT signing key: distinct asset class; 24h rotation overlap.
- WebAuthn credential storage: pgcrypto column-level encrypted em Neon.

### 6.3 Privacy (herda `privacy_model.md`)

- PAT é PII; DSR export integration S-11.
- PAT plaintext NUNCA logged; redact macro S-09 R-S09-6.

### 6.4 SLOs (herda `slo_catalog.md`)

- SLO-FRESH-PAT-REVOKE ≤ 60s p99 (novo SLO; adicionar slo_catalog).
- SLO-AVAIL-AUTH ≥ 99.9% sustained 72h staging.

## 7. Definition of Done (lane HIGH_RISK)

- [ ] Todos 8 WIs SEALED (EVT-031).
- [ ] Chaos test revoke PAT mid-flight → falha ≤ 60s p99 cross-region (EVT-023).
- [ ] Pentest adversarial: PAT forge + replay attack tested (EVT-040).
- [ ] SSO flow E2E: signup → tenant created → PAT emitted → use em CAS write → revoke → falha (EVT-018).
- [ ] MFA WebAuthn 3 devices (YubiKey + platform + iCloud Keychain passkey) (EVT-018).
- [ ] LGPD DSR export PAT list + revoke em erasure pipeline (S-11 integration) (EVT-042).
- [ ] Property test 10k iter PR + 100k iter nightly (≤ 30 min com Argon2 sampled 1% iter budget; vide WI-S03-008 §6.1 P0 fix Lote 10.3bis) (EVT-002).
- [ ] PRR HIGH_RISK 13 sign-offs (12 mandatory + 1 advisory Crypto SME) (EVT-031): Owner + Final Approver + SRE + Security + Engineer (×2) + QA + Product + Compliance + Privacy + Architect + AppSec + Crypto SME advisory.
- [ ] TLA+ tenant_isolation.tla green sustained pós-integração (EVT-022).
- [ ] OWASP ASVS V2/V3 100% checklist pass (EVT-002).
- [ ] Cost regression gate: auth middleware p99 ≤ 5ms sustained (Lote 9.4 §14.10).
- [ ] RB-FM-160 (auth invalid storm) dry-run (EVT-017).
- [ ] SBOM CycloneDX 1.5+ signed (EVT-010).

## 8. Dependencies

### Hard blockers

- **S-01 SEALED** (tenant_path lib + blob_meta).
- **S-00** roadmap.

### Soft blockers

- S-09 audit emission stack (audit chain integrity preferred but not blocker).

### Outbound

- S-02, S-04, S-08, S-10, S-11, S-13, S-14, S-19, S-20.

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-01+S-02 SEALED).
- **Mid-check**: D+10.
- **Sprint close**: D+27 (4 semanas HIGH_RISK + 7 dias buffer).

## 10. Risk Register

Ver `_spec_contract.md §15` (10-row 6-col).

## 11. Observability Plan

Métricas auth (sprint contract §observability); dashboards DASH-AUTH novo.

## 12. Security & Privacy

STRIDE: spoofing/tampering/info-disclosure todos cobertos via 5-layer defense + Argon2id + timing-safe verify + WebAuthn phishing-resistant. LINDDUN: PAT é PII (DSR-supported S-11).

## 13. Post-mortem hooks

PAT scope escalation / revocation > 60s / WebAuthn replay / Argon2id timing attack / PAT log leak / JWKS cache compromise — todos triggers post-mortem CRITICAL.

## 14. Sign-off (HIGH_RISK 13)

Owner + Final Approver + SRE Lead + Security Lead + Engineer + QA + Product + Compliance + Privacy + Architect + AppSec + Crypto SME + 2 peers (12 mandatory + 1 optional Crypto SME = 13).

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-03 (Lote 9.5b). |

---

**Fim de S-03 sprint contract.**
