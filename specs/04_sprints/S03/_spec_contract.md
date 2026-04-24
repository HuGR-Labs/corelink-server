---
id: "SPEC-CONTRACT-S03"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s03", "auth", "clerk", "high-risk"]
---

# Spec Contract — S-03: Auth Real (Clerk SSO + PAT lifecycle)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-03 |
| Nome | Auth Real (Clerk SSO + PAT lifecycle + revocation 60s) |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-002 (tenant isolation), FF-HR-005 (CTRL-AUTH-*), FF-HR-009 (contrato — Terms) |
| Duração estimada | 3–4 semanas |
| WIs antecipados | 8 |

## 1. Objetivo

Substituir PAT stub de S-01/S-02 por auth real: Clerk SSO para signup de devs, PAT lifecycle completo (emit → use → revoke) com scopes tipados, revocation propagation em ≤ 60s global, MFA para admin ops, audit trail de toda op auth. Destrava pagamento real em S-10 e acesso enterprise em S-14.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK
- **FF-HR-002**: tenant_id derivado do auth context — bug quebra isolation.
- **FF-HR-005**: implementa CTRL-AUTH-001 (PAT HMAC), CTRL-AUTH-007 (nonce replay), CTRL-AUTH-010 (MFA), CTRL-AUTHZ-001..002, CTRL-CRED-001..004.
- **FF-HR-009**: introduz contrato (Terms of Service + PAT scope agreement) com customer.

## 3. Inherits_from

```yaml
inherits_from:
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "KEY-MANAGEMENT"
  - "PRIVACY-MODEL"
  - "INVARIANT-REGISTRY"
  - "DATA-MODEL"
  - "COMPLIANCE-MATRIX"
  - "OBSERVABILITY-MODEL"
```

## 4. CAPs entregues

- **CAP-AUTH-001**: Clerk SSO signup + login (email + WebAuthn optional).
- **CAP-AUTH-002**: PAT emission com scopes tipados (cache-r, cache-w, admin-*).
- **CAP-AUTH-003**: PAT revocation propagation ≤ 60s global (CTRL-CRED-004).
- **CAP-AUTH-004**: MFA obrigatório para admin ops (CTRL-AUTH-010).
- **CAP-AUTH-005**: Tenant provisioning (Account → Tenant → Membership).

## 5. Requirements específicos

- **R-S03-1**: Clerk adapter (`crates/clerk-auth`) + JWKS fetch + JWT validate.
- **R-S03-2**: Middleware `tower` interceptando gRPC/HTTP + injetando TenantCtx.
- **R-S03-3**: Crate `corelink-pat` com emit (Argon2id hash), verify, scope check.
- **R-S03-4**: Revocation broadcast via DO `pat-invalidator-<region>` + KV cache invalidation.
- **R-S03-5**: Tabela Neon `account`, `tenant`, `user_account`, `membership`, `pat` (DDL conforme `data_model.md §4.1`).
- **R-S03-6**: WebAuthn registration/authentication endpoints para admin flows.
- **R-S03-7**: Audit events `pat.issued.v1`, `pat.revoked.v1`, `auth.login.v1` (EVT-047).
- **R-S03-8**: Property test: revoked PAT falha em < 60s em qualquer região.

## 6. Definition of Done

- [ ] 8 WIs SEALED.
- [ ] Chaos test: revoke PAT mid-flight → falha ≤ 60s (EVT-023).
- [ ] Pentest adversarial: tentativa de PAT forge + replay attack com timestamp oldnesses (EVT-025).
- [ ] SSO flow E2E: signup → tenant created → PAT emitted → use em CAS write → revoke → falha.
- [ ] MFA WebAuthn testado com 3 devices diferentes (YubiKey, platform authenticator).
- [ ] LGPD DSR support: export PAT list + revoke em erasure pipeline.

## 7. Completeness Criteria (delta local)

- [ ] **10.s03.1** CTRL-CRED-004: revocation p99 ≤ 60s em 3 regiões (load test).
- [ ] **10.s03.2** INV-TENANT-ISOLATION preservada pós-integração (TLA+ verde + property test).
- [ ] **10.s03.3** Zero secret em logs (SAST gitleaks + custom rules).

## 8. Invariants

- INV-TENANT-ISOLATION (CRITICAL, TLA+)
- INV-CONF-IN-FLIGHT (HIGH): TLS 1.3 obrigatório Clerk↔Worker.
- INV-CONF-AT-REST (HIGH): PAT hashes em Neon com SSE.

## 9. Quality Standards (delta local)

- **14.s03.1** Zero PAT plaintext persistido (só hash).
- **14.s03.2** WebAuthn credentials in Neon com column encryption.
- **14.s03.3** Auth middleware adds ≤ 5ms p99 overhead.

## 10. Anti-scope

- ❌ BYOK enterprise (S-14).
- ❌ SAML / enterprise IdP (Fase 2).
- ❌ UI admin panel (S-16).
- ❌ Billing integration (S-10).

## 11. Dependencies

- **Blocker:** S-01 SEALED (tenant_path lib; auth middleware consumes it).
- **Blocker:** S-00 (capabilities catalog pra scope naming).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S03-001 | Clerk adapter (JWKS + JWT validate) |
| WI-S03-002 | Crate corelink-pat (emit + verify + scope) |
| WI-S03-003 | Tower middleware + TenantCtx injection |
| WI-S03-004 | Revocation DO + KV cache invalidation |
| WI-S03-005 | Neon schema + migrations (account/tenant/user/pat) |
| WI-S03-006 | WebAuthn admin flows |
| WI-S03-007 | Audit events emission (EVT-047) |
| WI-S03-008 | Property test revocation < 60s |

## 13. Duração

3–4 semanas; buffer 7 dias úteis (Clerk API changes são risco).

## 14. Critérios de promoção

- DoD complete.
- Pentest report aprovado.
- PRR.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Clerk schema breaking change | M | HIGH |
| WebAuthn cross-browser compat | M | MEDIUM |
| Revocation latência > 60s sustained | M | HIGH (SLO miss) |
| PAT scope escalation bug | L | CRITICAL |

---

**Fim spec contract S-03.**
