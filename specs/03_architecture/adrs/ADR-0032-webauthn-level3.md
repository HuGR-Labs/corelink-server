---
id: "ADR-0032"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-01"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "webauthn", "passkey", "yubikey", "mfa", "level3", "auth", "s03", "high-risk"]
---

# ADR-0032 — WebAuthn Level 3 implementation strategy + AAGUID allowlist policy + step-up flow design + 6-digit OTP recovery

## Status

FROZEN (S-03 WI-S03-006 ratified — SEALED 2026-05-01).

## Context

The auth domain `auth_model.md §1.6` + `security_model.md §CTRL-AUTH-010`
+ `key_management.md §3.13` mandate phishing-resistant MFA on every
admin operation (mass revoke, billing change, tenant DELETE, admin
user management). The W3C WebAuthn Level 3 recommendation (2024) is
the canonical source for the ceremony surface; FIDO2 + CTAP2 cover
the authenticator side. Five
load-bearing invariants from `invariant_registry.md` constrain the
design:

- **INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN** (CRITICAL) — admin step-up
  demands `flags & UV != 0`. UP alone is insufficient (an unattended
  unlocked YubiKey would otherwise satisfy admin gating).
- **INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED** (CRITICAL) — registration
  validates the attestation chain; the engine cannot accept a
  self-attested or absent attestation.
- **INV-AUTH-WEBAUTHN-SIGN-COUNT-MONOTONIC** (HIGH) — the
  authenticator-reported counter is strictly increasing (W3C-compliant
  policy below).
- **INV-AUTH-WEBAUTHN-ORIGIN-EXACT** (CRITICAL) — origin allowlist
  is **exact match**; no prefix / regex / suffix matching.
- **INV-AUTH-WEBAUTHN-RP-ID-CANONICAL** (CRITICAL) — RP-ID is
  `corelink.humangr.com` eTLD+1, never a subdomain.

The canonical questions:

1. **Hand-roll vs `webauthn-rs = 0.5`** — the W3C L3 ceremony is
   intricate (CBOR/COSE attestation parsing, AAGUID-vs-attestation
   correlation, FIDO MDS metadata). Rolling our own = full security
   posture loss; out-of-scope at S-03.
2. **Production engine wiring window** — the production engine
   needs Cloudflare KV (challenge store), Neon (credential store),
   real authenticators, and a staging deploy. Per charter
   §inflection (Cloudflare credentials are a HARD inflection
   point), the production shim is wired in a downstream WI alongside
   real CF account setup. The S-03 SEAL ships the **trait surface +
   in-memory engine + algorithmic invariant enforcement** so every
   adversarial-test, property-test, and sprint-close audit runs
   host-side without that inflection.
3. **AAGUID allowlist policy** — FIDO MDS is authoritative but
   network-loaded. A compromised MDS feed could silently widen the
   policy. We use a closed-default explicit allowlist (config-side)
   with a sibling denylist that takes precedence; MDS metadata
   enriches the trace but does not gate accept/reject.
4. **Step-up token semantics** — a single ceremony per admin op is
   UX-hostile (5 prompts in a row to revoke 5 tokens). We mint a
   short-lived (5 min default) `StepUpToken` bound to
   `(user_id, op_class, credential_id)` so subsequent admin ops in
   the same session reuse the ceremony.
5. **Recovery flow** (Lote 10.3-tris P0-R5-002a) — when all
   credentials are lost, the user re-enrols via Clerk SSO email +
   **6-digit OTP**. **Magic-link recovery is rejected** — it is
   phishing-prone (a click on a malicious link defeats the channel).
   The OTP is Argon2id-hashed at rest, single-use,
   rate-limited (3 generations / hour / user; 5 verify attempts /
   OTP), and TTL-bounded at 600 s.
6. **Sign-count regression policy** (Lote 10.3-tris P0-R5-002b) —
   the original "first regression = SEV-1" policy was alert-fatigue
   bait; passkey providers (Apple iCloud Keychain, 1Password) report
   `sign_count = 0` always. The W3C-compliant policy enumerates
   three cases: (a) `(0, 0)` is passkey-exempt; (b) strictly
   increasing is monotonic; (c) regression is SEV-2 first, SEV-1
   only after forensic confirmation (≥ 2 independent signals — IP
   geolocation mismatch + UA fingerprint mismatch + SRE acknowledgement).
   The "≥ 3 in 24 h" threshold from §28 R-004 is REMOVED — it was
   creating a 24-72 h false safety window that absorbed real
   cloned-authenticator attacks.

## Decision

### Core surface

- New crate `crates/corelink-webauthn/` carrying the canonical
  ceremony surface, in-memory engine, AAGUID policy, recovery OTP,
  and step-up token primitives. Crate-strict lints
  (`#![forbid(unsafe_code)]` + Cargo `[lints]` denying
  `unwrap_used` / `expect_used` / `panic` / `indexing_slicing`) match
  the precedent set by `corelink-pat` / `corelink-clerk` /
  `corelink-auth-schema`.
- `WebAuthnEngine` trait with `start_registration` /
  `finish_registration` / `start_authentication` /
  `finish_authentication`. Two implementations ship at SEAL:
  - `InMemoryEngine` — host-side fake enforcing every algorithmic
    invariant; the load-bearing surface for property tests +
    adversarial regressions.
  - `ProductionEngineNotConfigured` — sentinel returning
    `WebAuthnError::EngineNotConfigured` until the
    `feature = "host-server"` shim lands alongside real CF + Neon
    credentials.
- `ChallengeStore` / `CredentialStore` / `RecoveryOtpStore` traits
  with in-memory implementations (`InMemoryChallengeStore`,
  `InMemoryCredentialStore`, `InMemoryRecoveryOtpStore`). Each
  in-memory store enforces the canonical UNIQUE / single-use /
  rate-limit invariants algorithmically.

### RP-ID + origin policy

- `RpId::new` rejects `localhost`, `127.0.0.1`, `0.0.0.0`, `::1`,
  any single-label string, and any uppercase / non-canonical
  characters. SHA-256 over the canonical bytes is the
  authenticator-side `rpIdHash`.
- `Origin::parse` requires `https://`, rejects userinfo / path /
  query / fragment, and lowercases the host.
- `OriginAllowlist::contains` is exact match (`BTreeSet` lookup;
  no prefix / regex).
- `OriginAllowlist::require_consistency_with(rp_id)` rejects
  origins whose host is not a sub-label of (or equal to) the
  canonical RP-ID. Builder-time invariant.

### AAGUID policy

- `AaguidPolicy` carries an allowlist + denylist (both `BTreeSet`).
  Closed-default semantics: a missing allowlist entry yields
  `AaguidNotAllowed`; a denylist hit yields `AaguidDenied` (denylist
  takes precedence).
- Hard-coded synthetic AAGUIDs for fixtures: YubiKey 5, Touch ID,
  Windows Hello, Android biometrics, iCloud Keychain passkey, and
  the deprecated YubiKey 4 (used by the
  `test_aaguid_denylist_blocks_deprecated_authenticator` adversarial
  regression).

### Step-up token

- 5 min TTL default; 32-byte CSPRNG secret; constant-time
  `subtle::ConstantTimeEq` validation.
- Bound to `(user_id, op_class, credential_id)` — a token minted
  for `mass_revoke` cannot be replayed against `billing_change`
  even if the secret leaks.
- `Drop` zeroizes the secret bytes.

### Recovery OTP (Lote 10.3-tris P0-R5-002a)

- 6 decimal digits; CSPRNG-drawn via `rand_core::OsRng`.
- Argon2id PHC (OWASP-2024 floor parameters: m = 65536 KiB, t = 3,
  p = 4, output 32 bytes); same parameters as `corelink-pat` so the
  hash cost is canonical across the auth stack.
- TTL 600 s default (canonical floor under the §6.1.10 ceiling).
- `RecoveryChannel` enum has exactly one variant
  (`ClerkSsoEmail`) — magic-link recovery is **unrepresentable** at
  the type level. Adding a `MagicLink` variant would require an ADR
  amendment + a sprint-contract anti-scope inversion.
- Single-use: `verify_and_consume` performs an atomic
  `attempts_remaining -= 1` followed by an Argon2id verify; success
  sets `consumed_at_ms`, re-use returns `RecoveryOtpAlreadyConsumed`.
- Rate limit: 3 generations / hour / user; 5 verify attempts / OTP.

### Sign-count assessment (Lote 10.3-tris P0-R5-002b)

- `assess(stored, incoming)` returns one of three severities:
  - `PasskeyExempt` for the `(0, 0)` canonical passkey pattern.
  - `Monotonic` for `incoming > stored` (persists incoming).
  - `Sev2InvestigationRequired` otherwise — caller emits
    `WebAuthnError::SignCountRegression { got, stored }` and the
    metrics observer increments `sign_count_regression_total`.
- The W3C-compliant policy distinguishes signal from noise by
  exempting the passkey canonical pattern; the original §9.9
  SEV-1-on-first-regression rule fired spuriously on every passkey
  login.

### COSE algorithm policy

- `parse_cose_algorithm(value)` accepts the IANA-registered values
  for ES256 (-7), EdDSA (-8), RS256 (-257). All others — including
  the well-known `alg: 0` "none" attack — return
  `WebAuthnError::Malformed("alg none" | "unknown cose algorithm")`.

## Consequences

- Every WebAuthn invariant family member
  (`INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN`,
  `INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED`,
  `INV-AUTH-WEBAUTHN-SIGN-COUNT-MONOTONIC`,
  `INV-AUTH-WEBAUTHN-ORIGIN-EXACT`,
  `INV-AUTH-WEBAUTHN-RP-ID-CANONICAL`) is enforced algorithmically
  by the in-memory engine + adversarial regressions. The
  cross-browser Playwright matrix (16 scenarios; 4 browsers × 4
  ceremonies) lands as `e2e/webauthn/` in a downstream WI alongside
  real authenticators.
- The trait surface freezes the production-shim contract; swapping
  in `webauthn-rs = 0.5` does not require touching the call sites.
- The recovery OTP flow is the canonical "lost device" path; the
  type system blocks any future drift toward magic-link recovery.
- The step-up token is the canonical admin-op gate; the Tower
  middleware (WI-S13-XXX admin plane) consumes the token directly
  and emits `auth.admin_op.webauthn_authenticated` audit events
  per the `corelink.auth.webauthn.admin_op_step_up_total{op}`
  metric label.
- Argon2id mint+verify is the wall-clock bottleneck for the
  recovery OTP property tests (≈ 200 ms per cycle at OWASP-2024
  floor parameters); the proptest `prop_recovery_otp_single_use_100`
  bounds the iteration count at 100 to fit inside the
  per-test CI budget while the cheap invariants
  (`prop_challenge_uniqueness_10k`, `prop_sign_count_assess`,
  `prop_origin_allowlist_strict`, `prop_aaguid_policy`) carry the
  10k case load.

## Alternatives considered

- **Hand-rolled CBOR/COSE parser**: rejected — full security posture
  loss; out-of-scope.
- **MDS-only AAGUID policy**: rejected — single network source of
  truth; compromised feed widens policy silently.
- **`origins_allowlist` with prefix match**: rejected by
  `INV-AUTH-WEBAUTHN-ORIGIN-EXACT`.
- **RP-ID = full hostname**: rejected by W3C §5.1.2 (must be
  domain or eTLD+1; using a full host means credentials are not
  shared across `app.corelink.humangr.com` + `admin.corelink.humangr.com`).
- **Magic-link recovery**: rejected by sprint contract §10
  anti-scope (phishing-prone clicks).
- **Skip attestation in registration**: rejected by
  `INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED`.
- **Single ceremony per admin op (no step-up token)**: rejected on
  UX grounds (5 prompts to revoke 5 tokens).
- **Resident-key-only authentication**: rejected at SEAL
  (`allowCredentials` carries the canonical credential list); the
  resident-key UX path is a post-GA optimisation.

## §A1. OTP-vs-magic-link rationale (Lote 10.3-tris addendum)

Magic links are a phishing primitive at the user side — a single
click on an attacker-spoofed email link binds the recovery flow to
the attacker's session. Treating the recovery channel as a click
primitive contradicts the WebAuthn phishing-resistance posture.

The 6-digit OTP forces a second factor: the user must read the
digits from the email AND type them into the legitimate origin.
A phishing site that captures the digits cannot complete the
ceremony because the OTP is bound to the legitimate Clerk SSO
session + the legitimate `app.corelink.humangr.com` origin (which is in
the allowlist). The 10-min TTL bounds the attacker's window; the
3-generation / hour rate limit bounds the brute-force surface; the
5-verify / OTP attempts limit prevents online guessing of the
20-bit space.

The remaining residual risk is the email channel itself
(SIM-swap-style attacks against the email account). Out-of-scope
for this WI; covered by Clerk SSO security posture +
`auth.webauthn.recovery_otp_*` audit events allowing forensic
reconstruction.

## References

- W3C Web Authentication: An API for accessing Public Key
  Credentials Level 3 — Recommendation 2024.
- FIDO Alliance Security Reference v2.2 — §3.4 (sign-count semantics).
- `WI-S03-006-webauthn-level3-admin.md` (canonical work item).
- `WI-S03-005-neon-schema-auth-tables.md` §6.1 — column-encrypted
  `webauthn_credentials.public_key`.
- `auth_model.md §1.6` — service identity exclusion (WebAuthn is
  human-only).
- `security_model.md §CTRL-AUTH-010` — MFA + session bind.
- `key_management.md §3.13` — column encryption for WebAuthn keys.
- `privacy_model.md §82, §88` — credential PII handling.
- `invariant_registry.md` — INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN,
  INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED,
  INV-AUTH-WEBAUTHN-SIGN-COUNT-MONOTONIC,
  INV-AUTH-WEBAUTHN-ORIGIN-EXACT,
  INV-AUTH-WEBAUTHN-RP-ID-CANONICAL.
- `framework.md §17` — Neon control-plane + RLS + RPO/RTO.
- `corelink_autonomous_execution_charter.md` (charter
  trait-abstraction-defer pattern; `webauthn-rs` shim deferred
  alongside Cloudflare credential inflection point).
