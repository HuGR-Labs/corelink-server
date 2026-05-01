# corelink-webauthn

WebAuthn Level 3 admin primitives (WI-S03-006). HIGH_RISK auth boundary
(FF-HR-002 / FF-HR-005 / FF-HR-009) — first line of defence against
phishing-resistant MFA bypass.

## What this crate ships

- `WebAuthnEngine` trait + `InMemoryEngine` enforcing every
  `INV-AUTH-WEBAUTHN` family invariant algorithmically.
- `ProductionEngineNotConfigured` sentinel — freezes the contract until
  the `webauthn-rs = 0.5` shim lands alongside Cloudflare credentials
  (charter `§inflection` HARD trigger; staging CF + real authenticators
  inflection point).
- `RpId` / `Origin` / `OriginAllowlist` — exact-match guards rejecting
  loopback, single-label, http://, userinfo, and prefix-bypass attacks.
- `AaguidPolicy` — closed-default allowlist with denylist precedence;
  synthetic AAGUIDs for YubiKey 5 / Touch ID / Windows Hello / Android
  biometrics / iCloud passkey / deprecated YubiKey 4.
- `AuthenticatorFlags` — UP / UV / BE / BS bit accessors mapped to the
  W3C `authData.flags` byte layout.
- `SignCount` + `assess()` — W3C-compliant policy with passkey-exempt
  `(0, 0)` pattern (Lote 10.3-tris P0-R5-002b).
- `recovery::*` — 6-digit Argon2id-hashed OTP with `RecoveryChannel`
  enum locked to `ClerkSsoEmail` (magic-link unrepresentable at the
  type level; Lote 10.3-tris P0-R5-002a).
- `step_up::StepUpToken` — 5-min default TTL; constant-time
  `subtle::ConstantTimeEq` validation; bound to `(user_id, op_class,
  credential_id)`.
- `MetricsObserver` trait + `NoopMetrics` + `MetricsRecorder` —
  canonical 6-metric set per `WI-S03-006 §6.1.7`.

## Test surface

- `tests/canonical_vectors.rs` — 21 tests pinning the canonical wire +
  type shape across refactors.
- `tests/adversarial.rs` — 14 regressions covering every
  `INV-AUTH-WEBAUTHN` attack class (`alg: none`, origin spoof, RP-ID
  confusion, UV downgrade, attestation absent, AAGUID denylist, AAGUID
  closed-default, sign-count regression, passkey exemption, challenge
  TTL replay, recovery OTP single-use, recovery OTP rate limit, magic
  link unrepresentable, step-up token op-class binding).
- `tests/prop_webauthn.rs` — 7 property tests (10k iter on the cheap
  invariants; 100 iter on the OTP cycle bounded by Argon2id wall
  clock).

## Examples

- `examples/passkey_enroll.rs` — Touch ID enrolment.
- `examples/yubikey_admin_op.rs` — admin step-up via YubiKey 5.
- `examples/cross_browser_test.rs` — Playwright matrix simulator
  (4 browsers × 4 ceremonies = 16 scenarios).
- `examples/recovery_flow.rs` — 6-digit OTP recovery (no magic link).

## Forbidden surface

- **No `unsafe`** — `#![forbid(unsafe_code)]`.
- **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
  library code (Cargo `[lints]` deny).
- **No `webauthn-rs` runtime dependency at SEAL** — production shim
  is gated behind `feature = "host-server"` (unwired in this Lote).

## References

- W3C Web Authentication: An API for accessing Public Key Credentials
  Level 3 — Recommendation 2024.
- ADR-0032 — design rationale.
- WI-S03-006 — canonical work item.
- `auth_model.md §1.6`, `security_model.md §CTRL-AUTH-010`,
  `key_management.md §3.13`, `invariant_registry.md` (5 invariants).
