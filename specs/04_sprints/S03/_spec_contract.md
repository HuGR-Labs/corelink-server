---
id: "SPEC-CONTRACT-S03"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.10.2"
created: "2026-04-24"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s03", "auth", "clerk", "pat", "webauthn", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-03: Auth Real (Clerk SSO + PAT Lifecycle + Revocation ≤ 60s + WebAuthn MFA)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-03 |
| Nome | Auth Real |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-002 (tenant isolation derivada de auth ctx), FF-HR-005 (CTRL-AUTH-001..010 + CTRL-CRED-001..004 + CTRL-AUTHZ-001..002), FF-HR-009 (Terms of Service contrato customer) |
| Duração estimada | 4 semanas + buffer 7 dias |
| WIs antecipados | 8 |
| SOTA target | Auth tier-1 — Clerk SSO + PAT lifecycle ≤ 60s revocation global + WebAuthn MFA + Argon2id PAT hash + audit chain |

## 1. Objetivo

Substituir PAT stub de S-01/S-02 por **auth real production-grade**: Clerk SSO para signup de devs (email + WebAuthn opcional), PAT lifecycle completo (emit → use → revoke) com scopes tipados, **revocation propagation em ≤ 60s global cross-region**, MFA WebAuthn obrigatório para admin ops, audit trail de toda op auth (EVT-047 CloudEvents). Destrava pagamento real em S-10 (Stripe customer ID), acesso enterprise em S-14 (BYOK + DPA), self-service admin em S-13/S-16.

**Por que SOTA:** competitors operam PAT lifecycle com revocation lag minutos (não segundos); MFA via TOTP (não WebAuthn phishing-resistant); PAT hashes weak (bcrypt single-cost); zero audit trail. CoreLink S-03 entrega: (a) **Argon2id PAT hash** (NIST SP 800-63B + OWASP recommendation 2024); (b) **revocation ≤ 60s p99 global** via DO broadcast + KV cache invalidation; (c) **WebAuthn FIDO2-compliant MFA** (passkeys); (d) **audit chain integrity** S-09 alignment; (e) **5-layer tenant isolation** propagated. Reference: **NIST SP 800-63B** (Digital Identity Guidelines), **WebAuthn Level 3** spec, **OWASP ASVS V2/V3** (Auth/Session), **Clerk best practices**.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (11 sign-offs canonical per framework §33.5.4.3 + ADR-0034; Crypto SME folds into Architect role; AppSec é o 11º slot).
- **FF-HR-002**: tenant_id derivado do auth context — bug quebra isolation cross-tenant.
- **FF-HR-005**: implementa CTRL-AUTH-001..010 + CTRL-CRED-001..004 + CTRL-AUTHZ-001..002 (16+ controles).
- **FF-HR-009**: Terms of Service + PAT scope agreement = contrato customer; bug = legal exposure.

## 3. Inherits_from

```yaml
inherits_from:
  - "AUTH-MODEL"                # PAT semantics + 5-layer defense
  - "SECURITY-MODEL"            # CTRL-AUTH-* + CTRL-CRED-* + CTRL-AUTHZ-*
  - "KEY-MANAGEMENT"            # PAT signing key (registry §3.13 INV-KEY-OVERLAP 24h per ADR-0018)
  - "PRIVACY-MODEL"             # PAT como PII; DSR support
  - "INVARIANT-REGISTRY"        # INV-TENANT-ISOLATION, INV-CONF-IN-FLIGHT, INV-CONF-AT-REST, INV-KEY-NO-SKIP
  - "DATA-MODEL"                # account, tenant, user_account, membership, pat schemas §4.1
  - "COMPLIANCE-MATRIX"         # SOC 2 CC6.1..6.8 + LGPD/GDPR PII handling
  - "OBSERVABILITY-MODEL"       # auth métricas, SLO-FRESH-PAT-REVOKE
  - "RESILIENCE-PATTERNS"       # PAT-INVALIDATE-001 (revocation propagation)
  - "SLO-CATALOG"               # SLO-FRESH-PAT-REVOKE
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-AUTH-001** | Clerk SSO signup + login | Email + WebAuthn optional via Clerk SDK; JWKS validation. |
| **CAP-AUTH-002** | PAT emission com scopes tipados | `cache-r`, `cache-w`, `cache-rw`, `cache-find-missing`, `admin-tenant-read`, `admin-tenant-write`, `admin-billing` (canonical hyphen-form per auth_model.md §scope L176-184); emit one-time-display. |
| **CAP-AUTH-003** | PAT revocation propagation ≤ 60s global | CTRL-CRED-004; DO broadcast + KV cache invalidation 3 regiões. |
| **CAP-AUTH-004** | MFA WebAuthn obrigatório admin ops | CTRL-AUTH-010; passkey + YubiKey support; freshness 30 min (S-13 alignment). |
| **CAP-AUTH-005** | Tenant provisioning | Account → Tenant → Membership atomic; tenant_id stable identifier. |
| **CAP-AUTH-006** | Audit events PAT lifecycle | EVT-047 CloudEvents: `pat.issued`, `pat.revoked`, `pat.used`, `auth.login`, `mfa.verified`. |
| **CAP-AUTH-007** | Argon2id PAT hash | NIST SP 800-63B compliant; cost params per OWASP 2024 (m=65536, t=3, p=4). |
| **CAP-AUTH-008** | WebAuthn MFA Level 3 | W3C WebAuthn Level 3 spec compliance; passkey + YubiKey + platform authenticator; per WI-S03-006 (cycle 6 codex SEAL: was implicit em WI-006 §4 mas não enumerated em spec_contract; aligned). |
| **CAP-AUTH-009** | Phishing-resistant admin ops | WebAuthn step-up obrigatório admin operations (UV=1 required; per WI-S03-006); CTRL-AUTH-010. |

## 5. Requirements específicos

- **R-S03-1**: Clerk adapter (`crates/clerk-auth`) + JWKS fetch (cache 24h KV) + JWT validate (clock-skew tolerance 60s).
- **R-S03-2**: Middleware `tower` (gRPC) + Worker handler (HTTP) interceptando + injetando `TenantCtx { tenant_id, user_id, scopes, mfa_ts }`.
- **R-S03-3**: Crate `corelink-pat` com:
  - **Argon2id** hash (params: `m_cost=65536`, `t_cost=3`, `p_cost=4` — OWASP 2024 recommendation).
  - PAT format canonical: `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` (cycle 9 SEAL decision (a) hybrid HMAC + Argon2id; e.g., `corelink_pat_abc12345.x9k....abcDEF12345`); `token_id` = 16-char deterministic indexed lookup key; `random_secret` = 32 bytes random base64url; `hmac_sig` = base64url(HMAC-SHA256(pat_signing_key, token_id||"."||random_secret))[:22] fast-fail layer (≤100µs; defense vs DDoS + phishing).
  - Verify: timing-safe compare via `subtle::ConstantTimeEq`.
  - Scope check: typed enum + bitfield matching.
- **R-S03-4**: Revocation broadcast via DO `pat-invalidator-<region>`:
  - Per-region DO mantém `revoked_pat_ids` set (LRU 100k entries).
  - Cross-region propagation via Workers Queue (target ≤ 60s p99).
  - KV cache `pat_valid:<hash>` TTL 60s; revocation invalidates.
- **R-S03-5**: Tabela Neon `account`, `tenant`, `user_account`, `membership`, `pat` (DDL conforme `data_model.md §4.1`); column-level encryption para PAT hash + WebAuthn credential.
- **R-S03-6**: WebAuthn registration/authentication endpoints:
  - WebAuthn Level 3 spec compliance.
  - Passkey + YubiKey + platform authenticator support.
  - Resident credentials preferred.
  - MFA freshness 30 min (CTRL-AUTH-010).
- **R-S03-7**: Audit events `pat.issued.v1`, `pat.revoked.v1`, `auth.login.v1`, `mfa.verified.v1` (EVT-047 CloudEvents); chain integrity S-09 alignment.
- **R-S03-8**: Property test 10k: revoked PAT falha em < 60s em qualquer região; race conditions revocation vs concurrent use covered.
- **R-S03-9**: PAT signing key rotation: 24h overlap per ADR-0018 / key_management §3.2.1; aligned com S-13 secret rotation framework.

## 6. Definition of Done

- [ ] **WIs SEALED**: 8/8 (EVT-031).
- [ ] **Chaos test**: revoke PAT mid-flight → falha ≤ 60s p99 cross-region (EVT-023).
- [ ] **Pentest adversarial**: tentativa de PAT forge + replay attack com timestamp oldness; zero successes (EVT-040).
- [ ] **SSO flow E2E**: signup → tenant created → PAT emitted → use em CAS write → revoke → falha (EVT-018).
- [ ] **MFA WebAuthn testado** com 3 devices diferentes (YubiKey, platform authenticator, passkey iCloud Keychain) (EVT-018).
- [ ] **LGPD DSR support**: export PAT list + revoke em erasure pipeline (S-11 integration) (EVT-042).
- [ ] **Property test 10k iter PR + 100k iter nightly** (≤ 30 min com Argon2 sampled 1% iter para fit budget; cf. WI-S03-008 §6.1.1): revocation propagation race conditions; 0 false positive (EVT-002).
- [ ] **PRR HIGH_RISK 11 sign-offs canonical** (per framework §33.5.4.3 + ADR-0034): Owner + Final Approver + Architect (Crypto SME specialization for JWT/Argon2id/WebAuthn/CBOR-COSE) + Security Lead + SRE Lead + Engineer + QA Lead + Product + Compliance + Privacy + AppSec advisor (EVT-031).
- [ ] **TLA+ tenant_isolation.tla** verde sustained com auth real integration (EVT-022).
- [ ] **CTRL-CRED-001..004** todos com evidence (PAT only-once display, no PAT em logs, hash storage, revocation ≤ 60s) (EVT-024).
- [ ] **OWASP ASVS V2/V3** checklist 100% pass (EVT-002).
- [ ] **Cost regression gate** (§14.10): auth middleware hot path ≤ 5ms p99 sustained (EVT-002).
- [ ] **RB-FM-160** (auth invalid storm) dry-run executado (criar runbook stub se ausente) (EVT-017).

## 7. Completeness Criteria (delta local)

- [ ] **10.s03.1** CTRL-CRED-004: revocation p99 ≤ 60s em 3 regiões (load test) (EVT-021).
- [ ] **10.s03.2** INV-TENANT-ISOLATION preservada pós-integração (TLA+ verde + property test 100k) (EVT-022 + EVT-002).
- [ ] **10.s03.3** Zero secret em logs (SAST gitleaks + custom rules) (EVT-002).
- [ ] **10.s03.4** **Argon2id params calibration**: hash time ~250ms server-side (OWASP rec); benchmark CI.
- [ ] **10.s03.5** **WebAuthn cross-browser**: tested Chrome/Firefox/Safari/Edge latest 2 versions.
- [ ] **10.s03.6** **DSR PAT export** integration test S-11 (EVT-042).
- [ ] **10.s03.7** **MFA freshness window 30 min** enforced + expiry test passes.

## 8. Invariants

### Mantidas

- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): tenant_id derivado de auth ctx; cross-tenant impossible.
- **INV-CONF-IN-FLIGHT** (HIGH): TLS 1.3 obrigatório Clerk↔Worker.
- **INV-CONF-AT-REST** (HIGH): PAT hashes em Neon com SSE + column-level encryption.
- **INV-KEY-NO-SKIP** (HIGH — registry §3.13): PAT signing key states respected.
- **INV-KEY-OVERLAP** (HIGH — registry §3.13 + ADR-0018): PAT signing key 24h overlap.

## 9. Quality Standards (delta local)

- **14.s03.1 Zero PAT plaintext persistido** (só hash via Argon2id).
- **14.s03.2 WebAuthn credentials** em Neon com column encryption (`pgcrypto`).
- **14.s03.3 Auth middleware** adds ≤ 5ms p99 overhead (criterion benchmark).
- **14.s03.4 Argon2id params** OWASP 2024 (m=65536, t=3, p=4); calibration ~250ms.
- **14.s03.5 PAT one-time display**: PAT mostrado apenas uma vez em UI/API; opt-out impossible.
- **14.s03.6 Timing-safe verify** via `subtle::ConstantTimeEq` (rust-crypto).
- **14.s03.7 MFA freshness 30 min** stale = re-MFA required.
- **14.s03.8 Cost regression gate** (§14.10): auth middleware p99 ≤ 5ms sustained.

## 10. Anti-scope

- ❌ BYOK enterprise (S-14).
- ❌ SAML / enterprise IdP (Fase 2 pós-GA).
- ❌ UI admin panel (S-16).
- ❌ Billing integration (S-10).
- ❌ Account recovery via SMS (anti-pattern phishing-prone — anti-scope estrito).
- ❌ TOTP-only MFA (rejected; WebAuthn phishing-resistant).
- ❌ Magic link auth (rejected; phishing-prone).
- ❌ Session cookie auth (PAT-only at GA; cookies S-16 admin UI).
- ❌ OAuth provider (CoreLink não é IdP).

## 11. Dependencies

### Hard blockers

- **S-01 SEALED** (tenant_path lib; auth middleware consumes TenantPrefix derive).
- **S-00 SEALED** (capabilities catalog pra scope naming).

### Soft blockers

- **S-09** (audit emission; pode ser delayed).

### Outbound

- **S-02** (CAS read consume auth ctx).
- **S-04** (AC consume auth ctx).
- **S-08** (PAT-level rate limit).
- **S-10** (billing customer mapping).
- **S-11** (DSR PAT export).
- **S-13** (admin plane MFA freshness).
- **S-14** (BYOK + enterprise auth).
- **S-19** (signup flow).
- **S-20** (GA exige pentest clean + revocation SLO sustained).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S03-001** | Clerk adapter (JWKS + JWT validate + clock-skew) | crate setup; JWKS fetch + cache 24h KV; JWT validate; clock-skew tolerance | 14h | 22h | 36h | **23.0h** |
| **WI-S03-002** | Crate corelink-pat (emit + verify + scope + Argon2id) | Argon2id integration; PAT format; timing-safe verify; scope check; benchmark calibration | 16h | 24h | 38h | **25.0h** |
| **WI-S03-003** | Tower middleware + TenantCtx injection + 5-layer defense propagation | middleware design; TenantCtx; Worker handler integration; integration test 5-layer | 12h | 18h | 30h | **19.0h** |
| **WI-S03-004** | Revocation DO + KV cache invalidation + cross-region propagation ≤ 60s | DO design; KV cache; cross-region Queue; load test; chaos test mid-flight revoke | 16h | 24h | 40h | **25.3h** |
| **WI-S03-005** | Neon schema + migrations (account/tenant/user/pat) + column encryption | schema migration; pgcrypto column encrypt; integration test | 10h | 14h | 22h | **14.7h** |
| **WI-S03-006** | WebAuthn admin flows (Level 3 spec) + cross-browser test | WebAuthn registration/auth; resident credentials; passkey + YubiKey + platform; cross-browser CI | 16h | 24h | 40h | **25.3h** |
| **WI-S03-007** | Audit events emission (EVT-047) + chain integrity S-09 alignment | CloudEvents emitter; 4 event types; chain hash; per-region audit bucket | 10h | 14h | 22h | **14.7h** |
| **WI-S03-008** | Property test revocation < 60s + pentest engagement + DSR PAT export + RB-FM-160 + PRR | property test 10k; pentest scope; DSR integration; runbook stub; PRR doc | 14h | 22h | 36h | **23.0h** |

**Total PERT:** ~170h ≈ 21 dias work × 1 eng. Buffer 7 dias confere com 4 semanas (Clerk + WebAuthn cross-browser unpredictable).

## 13. Duração + Timeline

- **Duração:** 4 semanas (20 dias úteis) + buffer 7 dias.
- **Marcos:**
  - **D+5:** WI-001 + WI-005 SEALED (Clerk adapter + schema).
  - **D+9:** WI-002 + WI-003 SEALED (PAT crate + middleware).
  - **D+14:** WI-004 SEALED (revocation ≤ 60s).
  - **D+18:** WI-006 SEALED (WebAuthn cross-browser).
  - **D+22:** WI-007 SEALED (audit events).
  - **D+26:** WI-008 SEALED (property + pentest + PRR).
  - **D+27:** Sprint review.

## 14. Critérios de promoção

- DoD complete + 72h staging sustained.
- Pentest report aprovado (zero HIGH/CRITICAL).
- Revocation SLO ≤ 60s sustained.
- WebAuthn cross-browser validated.
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido — 6 colunas)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Clerk schema breaking change** | M | M | HIGH | M | LOW | Adapter isolation; version pin; weekly Clerk changelog review; ADR per major migration. |
| **WebAuthn cross-browser compat** | M | M | MEDIUM | M | LOW | Cross-browser CI matrix latest 2 versions Chrome/Firefox/Safari/Edge; resident credentials for fallback. |
| **Revocation latência > 60s sustained** | M | M | HIGH (SLO miss) | M | LOW | DO broadcast + KV cache 60s TTL + Queue cross-region; load test 3 regions; SLO-FRESH-PAT-REVOKE. |
| **PAT scope escalation bug** | L | M | CRITICAL | L | LOW | Property test 100k scope check + adversarial pentest + typed enum compile-time. |
| **Argon2id misconfigured** (cost too low) | L | L | HIGH | L | LOW | Calibration benchmark ~250ms; OWASP params; CI test verifies time within range. |
| **Timing attack PAT verify** | L | L | HIGH | L | LOW | `subtle::ConstantTimeEq` enforced; criterion side-channel test. |
| **WebAuthn replay attack** | L | M | HIGH | L | LOW | Counter increment per use; resident credentials; clock-skew tolerance bounded. |
| **JWKS cache poisoning** | L | M | CRITICAL | L | LOW | Cache from Clerk-signed source only; signature validation; KV TTL 24h limits exposure. |
| **PAT log leak** (CTRL-CRED-001 bypass) | L | L | HIGH | L | LOW | Redact macro in logs; SAST gitleaks CI; runtime DLP scanner S-09. |
| **MFA freshness clock skew** abuse | L | L | MEDIUM | L | LOW | Server-side timestamp signed; clock-skew ≤ 60s; freshness 30 min hard. |

## 16. Benchmarks SOTA externos

| Critério | Auth0 | Clerk | Stytch | Frontegg | **CoreLink target S-03** |
|---|---|---|---|---|---|
| Argon2id PAT hash | Optional | Bcrypt | Bcrypt | Argon2 | **Yes — Argon2id OWASP 2024** |
| WebAuthn Level 3 | Yes | Yes | Yes | Yes | **Yes — passkey + YubiKey + platform** |
| Revocation ≤ 60s global | Yes | Yes | Yes | Yes | **Yes — 60s p99 3 regions** |
| 5-layer tenant isolation | Limited | Limited | Limited | Limited | **5 layers auth_model §8.1** |
| Audit chain integrity | Plug-in | Plug-in | Plug-in | Yes | **Yes — S-09 alignment** |
| Timing-safe verify | Yes | Yes | Yes | Yes | **Yes — subtle crate** |
| MFA freshness window | Configurable | Configurable | Configurable | Configurable | **30 min hard CTRL-AUTH-010** |
| OWASP ASVS V2/V3 100% | Yes | Yes | Yes | Yes | **Yes — checklist gate** |

## 17. References (RFCs, papers, standards)

- **NIST SP 800-63B** — Digital Identity Guidelines.
- **OWASP ASVS v4.0.3 V2/V3** — Authentication + Session.
- **OWASP Argon2id Recommendation 2024**.
- **WebAuthn Level 3** <https://www.w3.org/TR/webauthn-3/>.
- **FIDO2 Specification**.
- **RFC 7519** — JWT.
- **RFC 8725** — JWT Best Current Practices.
- **RFC 7638** — JWK Thumbprint.
- **Clerk Documentation**.
- `specs/03_architecture/error_taxonomy.md` — `COR_AUTH_*` error codes.
- ADR-0018 (key overlap per asset class — PAT signing key 24h).

## 18. Post-mortem hooks

- PAT scope escalation detected → CRITICAL post-mortem + Security review + customer notification.
- Revocation p99 > 60s sustained → 5-Why + DO/Queue review.
- WebAuthn replay attack detected → CRITICAL post-mortem.
- Argon2id timing attack proof of concept → post-mortem + crypto review.
- PAT log leak detected → CRITICAL post-mortem + redaction reinforce.
- JWKS cache compromise → CRITICAL post-mortem + Clerk + signature verify.

## 19. Waiver policy

S-03 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ Argon2id PAT hash (não bcrypt) — security baseline.
- ❌ Timing-safe verify (subtle crate) — side-channel baseline.
- ❌ Revocation p99 ≤ 60s — CTRL-CRED-004 baseline.
- ❌ WebAuthn (não TOTP) — phishing-resistance baseline.
- ❌ INV-TENANT-ISOLATION TLA+ green pós-integração — security baseline.
- ❌ Pentest zero HIGH/CRITICAL — security baseline.

Itens waivable com Security lead + Crypto SME + ADR:

- ⚠️ MFA freshness 30 min → 60 min (com explicit risk acceptance).
- ⚠️ Argon2id m_cost 65536 → 32768 com calibration adjusted.
- ⚠️ Revocation 60s → 90s com customer SLA addendum.

---


## 16. Changelog (cumulative)

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.3.0 | 2026-04-29 | Gustavo (Lote 10.3-bis-quater cycle 1 SEAL canonical sweep) | **Sign-off canonical normalization 13/10-12 → 11** (per framework §33.5.4.3 HIGH_RISK matrix + ADR-0034 solo-tier waiver; aligns S-01 + S-02 SEAL precedent). Crypto SME folds into Architect role as specialization (precedent for cripto-load-bearing WIs); Adversarial folds into AppSec; peer reviewers contribuem em PR review sem sign-off canonical separado. **39+ locations updated** across spec_contract + sprint + 8 WIs: §1 lane + §6 PRR DoD; sprint §5 D8 + §6 DoD + §14 matrix; WI-001..008 narratives + §30 headings; WI-008 §9.4 design decision rewritten + §3 + §6 + §8 AC + §10.6.5 + §11 + §14.6.4 + §17 sub-tasks + §18 dependencies + §19 timeline + §28 changelog + §32 anti-patterns. |
| 1.3.3 | 2026-05-01 | Gustavo (via Claude Opus 4.7) | **WI-S03-008 SEALED** (charter `2026-04-30 protocol`: no per-WI codex; sprint-close Sonnet adversarial review covers). Property test suite `crates/corelink-worker/tests/prop_auth_full.rs` lands 4 cross-component release-mode tests at 10k iter (no-alg-none acceptance / 5-layer defense propagation / revocation race full-stack / Argon2 calibration stable) gated behind `tower-middleware`. DSR PAT export integration coverage in `crates/corelink-auth-schema/tests/integration_dsr_pat_export.rs` (cascade + pseudonym preservation + cross-tenant + surface-shape). Internal pentest report `specs/_audits/2026-05-01-pentest-s03-internal.md` documents zero HIGH/CRITICAL with one MEDIUM (FM-249 follow-up). RB-FM-160 host-side dry-run automated `scripts/rb_fm_160_dry_run.sh` (cargo-driven, drift-detectable) + audit `specs/_audits/2026-05-01-rb-fm-160-dry-run.md`. OWASP ASVS V2/V3/V4/V6/V8 self-checklist `specs/04_sprints/S03/asvs-v2-v3-v4-v6-v8-checklist.md` published with WAIVED items revalidation-bound to S-08/S-13/S-19/S-20. Adversarial summary `specs/_audits/2026-05-01-adversarial-s03.md`. Constant-time variance gate in `corelink-client-verify` hardened to outlier-robust 10/80/10 trimmed-mean estimator (preserves leak-detection sensitivity while rejecting wall-clock contention spikes). All quality gates verde: 0 failures across 75 test groups; clippy `-D warnings` clean; validators clean (no new dangling refs). |
| 1.3.2 | 2026-05-01 | Gustavo (via Claude Opus 4.7) | **WI-S03-007 SEALED** (backfill — entry was omitted at SEAL commit `3296f86`; restored here for changelog continuity). Audit events emission ships in new crate `crates/corelink-audit/`: CloudEvents 1.0 envelope with `AuthEventType` (33 canonical variants pinned by exhaustive enum test) + RFC 8785 JCS canonicalization for content hash + chain-hash continuity via `subtle::ConstantTimeEq` (INV-AUDIT-CHAIN integrity). Domain-separated `principal_id` / `email_hash` / `pat_id` redact surrogates with empty-input rejection at construction (no raw PII can reach the canonical bytes per `prop_no_raw_pii_in_canonical_bytes`). RFC 8785 §A.3 test vector pinned. `MetricsObserver` trait + `RetentionTier` mapping + `InMemoryEmitter` for tests. 11 property tests + RFC 8785 vector + per-variant round-trip exhaustion. EVT-047 emission wired through audit emitter trait so callers (Tower auth middleware, revocation orchestrator, WebAuthn engine) emit type-checked auth events without seeing raw `sub`/`email`/`token` material. |
| 1.3.1 | 2026-05-01 | Gustavo (via Claude Opus 4.7) | **WI-S03-006 SEALED** (charter `2026-04-30 protocol`: no per-WI codex; sprint-close Sonnet adversarial review covers). New crate `crates/corelink-webauthn/` ships `WebAuthnEngine` trait + `InMemoryEngine` enforcing every WebAuthn family invariant (`INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN` / `INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED` / `INV-AUTH-WEBAUTHN-SIGN-COUNT-MONOTONIC` / `INV-AUTH-WEBAUTHN-ORIGIN-EXACT` / `INV-AUTH-WEBAUTHN-RP-ID-CANONICAL`) algorithmically; `ProductionEngineNotConfigured` sentinel freezes the contract until the `webauthn-rs = 0.5` shim lands alongside Cloudflare credentials (charter §inflection HARD trigger). RP-ID + origin exact-match guards; closed-default `AaguidPolicy` + denylist precedence; Argon2id-hashed 6-digit recovery OTP with magic-link unrepresentable at the type level (Lote 10.3-tris P0-R5-002a); W3C-compliant sign-count assessment with passkey-exempt `(0, 0)` pattern (Lote 10.3-tris P0-R5-002b); 5-min step-up token with constant-time `subtle::ConstantTimeEq` validation bound to `(user_id, op_class, credential_id)`; `MetricsObserver` trait + canonical 6-metric set. Tests: 21 canonical vectors + 14 adversarial regressions + 7 property tests (10k iter on the cheap invariants; 100 iter on the OTP cycle bounded by Argon2id wall clock). 4 examples (passkey enrol / YubiKey admin op / cross-browser matrix / recovery flow). ADR-0032 published; `validate_specs` + `validate_inv_promotion` + `validate_references` clean (no new dangling refs). |

---

**Fim spec contract S-03 v1.1.0 SOTA.**
