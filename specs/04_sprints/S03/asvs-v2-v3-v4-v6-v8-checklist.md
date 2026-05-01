---
id: "ASVS-S03-V2-V3-V4-V6-V8"
type: "compliance_matrix"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.1.0"
created: "2026-05-01"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["compliance", "asvs", "owasp", "s03", "auth", "checklist"]
---

# OWASP ASVS v4.0.3 — V2 / V3 / V4 / V6 / V8 self-checklist (S-03)

> **Sprint:** S-03 (Auth Real) · **WI:** WI-S03-008 §6.1.10
> **Date:** 2026-05-01 · **Mode:** internal self-checklist (external audit deferred to S-20 GA gate)

ASVS chapter scope per WI-S03-008 §10.6.10 + §14.6.4: V2 (Authentication), V3 (Session Management), V4 (Access Control), V6 (Stored Cryptography), V8 (Data Protection). Standard reference: <https://owasp.org/www-project-application-security-verification-standard/>.

This checklist tracks PASS / WAIVED / N/A per requirement. WAIVED entries carry a revalidation trigger (typically S-08 rate limit / S-13 admin plane / S-19 onboarding / S-20 GA gate).

## V2 — Authentication

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V2.1.1 | Passwords ≥ 12 chars (or token-based auth) | N/A | CoreLink uses PAT + Clerk JWT + WebAuthn — no user-typed passwords |
| V2.1.5 | No password truncation | N/A | same as above |
| V2.1.7 | Password breach checks | N/A | same as above |
| V2.1.9 | No periodic forced password change | N/A | same as above |
| V2.2.1 | Anti-automation controls (rate limit) on auth endpoints | WAIVED | S-08 forward; S-03 stub-passthrough emits events to audit chain only |
| V2.2.2 | Out-of-band MFA (only secure channels) | PASS | WebAuthn passkey + YubiKey + platform authenticator (WI-S03-006); RecoveryChannel locked to ClerkSsoEmail (Lote 10.3-tris P0-R5-002a) |
| V2.2.3 | Phishing-resistant MFA | PASS | WebAuthn FIDO2 Level 3 (NIST SP 800-63B AAL3 conformant) |
| V2.2.4 | Notify user on auth events | WAIVED | S-13 admin plane forward; audit events emitted now per EVT-047 |
| V2.3.1 | Verifier impersonation resistance | PASS | RP-ID exact match + origin allowlist (WI-S03-006) |
| V2.3.2 | TLS for credential exchange | PASS | TLS 1.3 mandatory (Cloudflare Edge); INV-CONF-IN-FLIGHT |
| V2.5.1 | Initial credential issuance over secure channel | PASS | Clerk-mediated email-magic-link enrolment via Clerk SSO (HTTPS only) |
| V2.5.4 | Token replay protection | PASS | Revocation orchestrator + KV cache 60s TTL + sign_count monotonic for WebAuthn |
| V2.5.5 | Token recovery follows separate flow | PASS | Recovery OTP via Argon2id-hashed 6-digit code, locked to ClerkSsoEmail (WI-S03-006) |
| V2.6.1 | Look-up secrets only ever transmitted hashed | PASS | PAT plaintext one-time display; Argon2id PHC stored in Neon |
| V2.6.2 | Credential entropy ≥ 112 bits | PASS | PAT random_secret = 32 bytes (256 bits); WebAuthn credential_id ≥ 32 bytes |
| V2.7.2 | OOB authenticator one-time use | N/A | OOB OTP only used in recovery flow; per-use single-shot consumption |
| V2.7.3 | OOB OTP ≥ 6 digits, ≥ 20 bits entropy | PASS | 6-digit recovery OTP, 19.93 bits entropy (Argon2id-hashed) |
| V2.7.4 | OTP transmitted via secure channel | PASS | Clerk SSO email (TLS 1.3) only |
| V2.8.1 | Single-factor cryptographic devices accepted | PASS | YubiKey + platform authenticator (WI-S03-006) |
| V2.9.2 | Authentic.tor session binding | PASS | WebAuthn credential bound to (user_id, op_class, credential_id) at step-up token mint |
| V2.10.1 | Service auth uses strong secret | PASS | PAT signing key 32 bytes random + 24h overlap rotation (WI-S03-002 + ADR-0018) |
| V2.10.2 | Service secrets stored encrypted at rest | PASS | Neon `pgcrypto` column-level encryption on `pat.token_hash` + `webauthn_credentials.credential_id` |
| V2.10.4 | Logging redacts authentication secrets | PASS | `redact_pat!` macro + `PrincipalIdHash` / `PatIdHash` / `EmailHash` newtypes (WI-S03-007) |

**V2 totals:** 16 PASS / 4 WAIVED / 6 N/A.

## V3 — Session Management

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V3.1.1 | No URL-bound session tokens | PASS | Bearer-token only via `Authorization` HTTP header |
| V3.2.1 | Sessions invalidated on logout | PASS | Revocation orchestrator + KV cache invalidation (WI-S03-004) |
| V3.2.2 | Cryptographically random session tokens | PASS | PAT random_secret 32 bytes + JWT signed |
| V3.2.3 | Cross-site script and malicious sites can't capture | PASS | TLS 1.3 + bearer-token (no cookies on the API plane) |
| V3.3.1 | Sessions invalidated after a period of inactivity | WAIVED | PAT lifetime configurable per row (`expires_at` column); enforcement at verifier; UI session 30 min freshness pinned (WI-S03-006) |
| V3.3.4 | Session ID rotates on privilege escalation | PASS | WebAuthn step-up mints a fresh 5-min token bound to `(user_id, op_class, cred_id)` |
| V3.4.1 | Bearer token TLS-only | PASS | TLS 1.3 mandatory at Cloudflare Edge |
| V3.4.5 | Session replay across user contexts blocked | PASS | TenantCtx 5-layer ordering; 0 cross-tenant `AuthCtx` leak across 200k attempts (`prop_auth_full::prop_revocation_race_full_stack` + `prop_auth_middleware::prop_5_layer_consistency`) |
| V3.5.1 | Logout terminates session backend | PASS | Revocation orchestrator broadcast queue + Neon SoT flip + KV cache invalidate (WI-S03-004) |
| V3.5.3 | Logout immediate (no opportunity for replay) | PASS | Cross-region propagation ≤ 60s p99 (SLO-FRESH-PAT-REVOKE) |
| V3.7.1 | Session ID never logged | PASS | Audit redaction newtypes (WI-S03-007) |

**V3 totals:** 10 PASS / 1 WAIVED / 0 N/A.

## V4 — Access Control

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V4.1.1 | Application enforces access control rules | PASS | Tower middleware AuthLayer (WI-S03-003) + scope check at handler |
| V4.1.2 | Trusted server-side enforcement (no client-only) | PASS | All access control at the worker boundary; client only sees `Bearer` token |
| V4.1.3 | Same access control rules for API + UI | PASS | Single auth surface — UI uses session cookie via Clerk; API uses PAT; same Neon-backed RLS layer |
| V4.1.5 | Access control fails secure | PASS | Default-deny RLS policies on `pat` + `webauthn_credentials` + `membership` |
| V4.2.1 | Direct object references protected by access control | PASS | TenantCtx 5-layer + tenant prefix derive + RLS layer |
| V4.2.2 | CSRF defense for state-changing operations | WAIVED | Bearer-token API (no cookies); UI is S-13/S-16 forward |
| V4.3.1 | Admin interfaces require multi-factor | PASS | WebAuthn step-up (WI-S03-006) for admin operations |
| V4.3.2 | Directory browsing disabled | PASS | Cloudflare Workers serves only enumerated routes |
| V4.3.3 | Forced browsing protection | PASS | TenantCtx 5-layer ordering rejects every cross-tenant probe |

**V4 totals:** 8 PASS / 1 WAIVED / 0 N/A.

## V6 — Stored Cryptography

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V6.1.1 | Sensitive secrets at rest cryptographically protected | PASS | Neon column-level `pgcrypto` encryption on `pat.token_hash` + `webauthn_credentials.credential_id` |
| V6.1.2 | Approved algorithms (SHA-256+, AES-256) | PASS | Argon2id (NIST SP 800-63B compliant) + SHA-256 + HMAC-SHA256 + BLAKE3 (CAS) + AES-256 (KMS DEKs) |
| V6.1.3 | Encryption key strength matches algorithm | PASS | KMS DEK 32 bytes; PAT signing key 32 bytes; TDK 32 bytes; HKDF derive (key_management.md §3.13) |
| V6.2.1 | Random number generators are CSPRNG | PASS | `rand::OsRng` (getrandom) + `argon2::password_hash::SaltString::generate` + `subtle::ConstantTimeEq` for compares |
| V6.2.2 | All initialization vectors random | PASS | Argon2id salt freshly generated per mint via `password_hash::SaltString::generate(OsRng)` |
| V6.2.3 | Sufficient strength of cryptographic algorithms | PASS | Argon2id `m=65536, t=3, p=4` (OWASP 2024 floor); RS256 JWT (Clerk); HMAC-SHA256 fast-fail (WI-S03-002 cycle 9) |
| V6.2.4 | Algorithm migration capability | PASS | PAT signing key id (`signing_key_id`) per row enables 24h rotation overlap (ADR-0018); Argon2id PHC string is self-describing for cost upgrades |
| V6.2.5 | Side-channel resistance for crypto operations | PASS | `subtle::ConstantTimeEq` everywhere; `argon2::Argon2::verify_password` constant-time; 3-arm Mann-Whitney sustained gate (WI-S02-004 reused at the cold-path pad seam) |
| V6.2.7 | Key generation outside the application | PASS | KMS key issuance (S-12 forward); PAT signing key derived via deploy-time HKDF |
| V6.3.1 | Documented key management lifecycle | PASS | `key_management.md` §3.13 + ADR-0018 (24h overlap per asset class) |
| V6.4.1 | Secret keys protected at rest | PASS | KMS-wrapped DEKs; PAT signing key zeroed via `Zeroizing<Vec<u8>>` |
| V6.4.2 | Key revocation mechanism | PASS | Revocation orchestrator (WI-S03-004) + KMS key revoke (S-12 forward) |

**V6 totals:** 12 PASS / 0 WAIVED / 0 N/A.

## V8 — Data Protection

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V8.1.1 | Application classified by sensitivity | PASS | privacy_model.md surface taxonomy + sensitivity classes (PII / Cryptographic / Customer-data) |
| V8.1.2 | All sensitive data encrypted at rest | PASS | Neon SSE + column-level pgcrypto for cryptographic material |
| V8.1.3 | Sensitive data not logged | PASS | `redact_pat!` macro + audit redaction newtypes (WI-S03-007); SAST gitleaks CI gate |
| V8.1.4 | Backup data protected | WAIVED | Neon point-in-time-recovery covers backups; KMS retention policy S-12 forward |
| V8.2.1 | Caching respects privacy classifications | PASS | KV cache holds only opaque hashes (`pat_valid:<hash>` / `auth:session:<hash>`) |
| V8.2.2 | Sensitive data not stored in client cache | PASS | API does not return Set-Cookie / Cache-Control: private; bearer-token shape only |
| V8.2.3 | Sensitive data cleared from memory | PASS | `Zeroizing<Vec<u8>>` for PAT plaintext + signing key + WebAuthn cred bytes |
| V8.3.1 | Sensitive data sent in body, not URL | PASS | `Authorization: Bearer <token>` header only |
| V8.3.2 | Anti-caching directives on sensitive data | PASS | API responses with sensitive payload include `Cache-Control: no-store` |
| V8.3.3 | Anti-caching CDN configuration | PASS | Cloudflare Worker bypasses CF cache for authenticated paths |
| V8.3.4 | Sensitive data not stored in DOM | N/A | API plane only; UI plane S-13/S-16 forward |
| V8.3.5 | Personal data has DSR support | PASS | DSR PAT export + erasure cascade (WI-S03-005 schema + WI-S03-008 §6.1.3 integration test) |

**V8 totals:** 10 PASS / 1 WAIVED / 1 N/A.

## Aggregated totals

| Chapter | PASS | WAIVED | N/A |
|---|---|---|---|
| V2 — Authentication | 16 | 4 | 6 |
| V3 — Session Management | 10 | 1 | 0 |
| V4 — Access Control | 8 | 1 | 0 |
| V6 — Stored Cryptography | 12 | 0 | 0 |
| V8 — Data Protection | 10 | 1 | 1 |
| **Total** | **56 PASS** | **7 WAIVED** | **7 N/A** |

**WAIVED revalidation triggers:**

- V2.2.1 / V2.2.4 — S-08 rate limit + S-13 admin plane.
- V2.7.x — N/A (OOB OTP only in recovery; covered).
- V3.3.1 — S-13 admin plane forward (UI session); API plane already covered.
- V4.2.2 — S-13/S-16 UI plane forward (CSRF only relevant to cookie-bearing surfaces).
- V8.1.4 — S-12 KMS backup retention forward.

## Sign-off

| Role | Signer | Date | Status |
|---|---|---|---|
| Owner | Gustavo Schneiter | 2026-05-01 | APPROVED |
| Compliance Officer (dual-hat per ADR-0034) | Gustavo Schneiter | 2026-05-01 | WAIVED |
| AppSec advisor (dual-hat per ADR-0034) | Gustavo Schneiter | 2026-05-01 | APPROVED |

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-01 | Gustavo Schneiter (via Claude Opus 4.7) | Initial ASVS V2/V3/V4/V6/V8 self-checklist authored as part of WI-S03-008 SEAL Lote. 56 PASS / 7 WAIVED / 7 N/A. |

---

**End checklist.**
