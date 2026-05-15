# STRIDE deep dive — `corelink-pat` (Personal Access Token issuance & verification)

- **Crate:** `crates/corelink-pat`
- **Date:** 2026-05-15
- **Owner:** Security Lead + Identity
- **Pentest scope:** Yes — engagement 2026-06-15 (P0 surface — primary tenant authentication)
- **Coarse references:** matrix-stride-ctrl.csv THR-S-001/002 (AST-TOKEN×TB-1), THR-I-006 (token in log), THR-E-001 (scope confusion)
- **Pentest doc cross-ref:** §3.1 Identity & Auth
- **SOC 2 cross-ref:** CC3.2 (risk ID — authn), CC6.1 (logical access), CC6.6 (encryption of credentials)

## 1. Trust boundaries

| Boundary | Caller | Callee | Auth/authz | Output |
|---|---|---|---|---|
| **TB-pat-1** | Admin / dashboard issuing token (admin-api) | `corelink-pat::issue(tenant_id, scopes, …)` | Clerk session + WebAuthn (CTRL-AUTH-010) + dual-approval for `admin:*` scopes | Plaintext token returned ONCE; hash stored (Argon2id + HMAC prefix) |
| **TB-pat-2** | External client (CLI, SDK, CI) → CF edge → Worker | `corelink-pat::verify(presented_token)` | Token presented in `Authorization: Bearer …` | `(tenant_id, scopes, key_id)` on success, sanitized error on fail |
| **TB-pat-3** | Token revocation (admin or auto) | `corelink-pat::revoke(prefix)` | Admin scope `pat:revoke` (own-tenant); platform scope for cross-tenant emergency | Token marked revoked; cache invalidation |
| **TB-pat-4** | Rotation / scope-update | `corelink-pat::rotate` | Owner-token + WebAuthn step-up | New token + grace overlap of old |

## 2. STRIDE per boundary

### 2.1 TB-pat-1 (issue)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Attacker steals admin session, issues token under own tenant | CTRL-AUTH-010 (WebAuthn + session bound to UA+IP+PKCE); INV-ADMIN-MFA-FRESHNESS 30 min | `crates/corelink-clerk/tests/adversarial.rs` |
| **T** | Issued token payload mutated before persistence | Token = `prefix.scope_compressed.sig` where `sig = HMAC(secret, prefix‖scopes‖tenant_id)`; persisted hash includes all fields | `crates/corelink-pat/tests/canonical_vectors.rs` (canonical test vectors) |
| **R** | Admin denies issuing high-privilege token | Audit chain emission pre+post issue; dual-approval signatures for `admin:*` scopes | INV-AUDIT-APPEND-ONLY + `crates/corelink-dual-approval/tests/prop_dual_approval.rs` |
| **I** | Plaintext token leaks via response log or trace | INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED — CI grep gate `tools/pat_plaintext_lint/`; clippy custom lint; once-only return contract | `crates/corelink-pat/tests/mutation_kills.rs` + CI lint — FM-AUTH-003 |
| **D** | Issue-storm attack to exhaust Argon2id CPU | Per-admin rate limit on issue endpoint; Argon2id cost tuned to ≤ 250 ms; fast-fail HMAC prefix before Argon2id | `crates/corelink-pat/tests/constant_time.rs` — FM-AUTH-004 |
| **E** | Caller crafts request with elevated scope set | INV-AUTH-PAT-SCOPE-DB-IS-SOT — scopes stored in DB `pat.scopes BIGINT`, never inferred from prefix; scope set must be subset of admin's own scopes | property test 10k attempts — FM-AUTH-005 |

### 2.2 TB-pat-2 (verify)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Token forgery via HMAC weakness | HMAC-SHA256 (CTRL-AUTH-001) with 256-bit secret in CF Secrets; constant-time compare | `crates/corelink-pat/tests/adversarial.rs` + `crates/corelink-pat/tests/constant_time.rs` |
| **T** | Mutate scope field after capture (extension attack) | INV-AUTH-PAT-HMAC-SIG-VERIFIED — full payload signed; tamper invalidates sig; tests cover all bit-flip positions | `crates/corelink-pat/tests/prop_pat.rs` (proptest 30k bit-flips) |
| **R** | Customer claims they didn't make API call | INV-AUTH-AUDIT-PRE-POST-ORDERING — pre+post audit emission with `pat.key_id` (not token plaintext) | INV-AUDIT-APPEND-ONLY + adversarial test |
| **I** | Verify error message reveals existence/scope of token | Sanitized error envelope (CTRL-NET-004); identical timing for unknown-prefix vs bad-sig (Mann-Whitney 3-prong); plaintext never logged on fail | `crates/corelink-pat/tests/constant_time.rs` + CTRL-ISO-004 timing layer |
| **D** | Argon2id verify storm | Fast-fail HMAC prefix check before Argon2id (INV-AUTH-PAT-HMAC-SIG-VERIFIED); per-IP rate limit; tenant-scoped bucket | constant-time bench + load test — FM-AUTH-004 |
| **E** | PAT scope confusion (read PAT used as admin) | INV-AUTH-PAT-SCOPE-DB-IS-SOT — scopes from DB row, never from prefix; verb-level scope check (CTRL-AUTHZ-001) | property test 10k attempts — FM-AUTH-005 |

### 2.3 TB-pat-3 (revoke)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Forged revocation removes legitimate token | `pat:revoke` scope is tenant-scoped; admin scope required for other tenants; audit emission | property test |
| **T** | Revoke flag flipped back via direct DB | Audit chain entry on revoke; D1 CHECK constraint on revocation log append-only | INV-AUDIT-APPEND-ONLY |
| **R** | "We never revoked X" / "We did revoke X" disputes | Audit entry with caller identity + reason | INV-AUDIT-APPEND-ONLY |
| **I** | Enumerate prefixes by revocation behavior | Revoke is idempotent; 200 OK regardless of pre-state; sanitized error | adversarial test |
| **D** | Mass revoke storm | Rate-limited per admin; audit-chain queue absorbs spike | k6 |
| **E** | Self-revoke escalation (admin revokes peer's token to lock them out) | RBAC: revoke own-tenant only unless platform scope; platform-scope ops dual-approval | dual-approval test |

### 2.4 TB-pat-4 (rotate)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Attacker with stolen token rotates to extend lifetime | Rotation requires WebAuthn step-up + bound original key_id | adversarial regression |
| **T** | New token's scope set widened during rotation | New scope must be subset of old (`new ⊆ old`); enforced server-side | property test 10k |
| **R** | "I never rotated" | Audit entry with old/new key_id | INV-AUDIT-APPEND-ONLY |
| **I** | Old token info leaked via rotation response | Response includes only new token + new key_id; old token plaintext never re-sent | `crates/corelink-pat/tests/adversarial.rs` |
| **D** | Rotation storm | Per-token rate limit | benches |
| **E** | Rotate escalates scope | Strict subset rule + WebAuthn | property test |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-PAT-01 | Customer-side token storage (CI secrets, env vars) outside CoreLink control | MEDIUM | Documented in customer onboarding; secret-scanning recommendation; FM-252 (leaked PAT) auto-revoke |
| RR-PAT-02 | Argon2id parameters trade off CPU vs UX — verify storm DoS budget tight | LOW | Fast-fail HMAC short-circuit; per-IP rate limit; observability SLI |
| RR-PAT-03 | rsa Marvin verify-only mitigation (F-002 WAIVED) — RSA-PSS verify on legacy clients | LOW (CVSS 3.7 post-mitigation) | ADR-S20-RSA-MARVIN-MITIGATION; migration to aws-lc-rs R-6 |

## 4. Adversarial test pointers

- `crates/corelink-pat/tests/adversarial.rs` — forgery, plaintext-leak, scope-confusion regressions
- `crates/corelink-pat/tests/prop_pat.rs` — 30k proptest bit-flips, scope-subset rotation
- `crates/corelink-pat/tests/canonical_vectors.rs` — golden vectors for HMAC compatibility
- `crates/corelink-pat/tests/constant_time.rs` — Mann-Whitney 3-prong timing
- `crates/corelink-pat/tests/mutation_kills.rs` — mutation baseline
- `tools/pat_plaintext_lint/` — CI grep gate (FM-AUTH-003)

## 5. Cross-references

- Invariants: INV-AUTH-PAT-HMAC-SIG-VERIFIED, INV-AUTH-PAT-SCOPE-DB-IS-SOT, INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED, INV-AUTH-AUDIT-PRE-POST-ORDERING, INV-AUTH-ISS-EXACT-MATCH, INV-ADMIN-MFA-FRESHNESS, INV-TENANT-ISOLATION
- Controls: CTRL-AUTH-001 (HMAC PAT), CTRL-AUTH-004 (path HMAC), CTRL-AUTH-007 (nonce), CTRL-AUTH-010 (MFA), CTRL-AUTHZ-001, CTRL-NET-004, CTRL-CRED-001
- Failure modes: FM-AUTH-001..005 (pentest §3.1), FM-252 (leaked PAT), FM-257 (replay)
- SOC 2: CC3.2, CC6.1, CC6.6
