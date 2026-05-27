---
id: "WI-S03-006"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.3.0"
created: "2026-04-25"
updated: "2026-05-01"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005", "FF-HR-009"]
parent: "S-03"
assignee: "Gustavo Schneiter"
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
  - "OBSERVABILITY-MODEL"
tags: ["wi", "s03", "auth", "webauthn", "passkey", "yubikey", "mfa", "level3", "high-risk"]
---

# WI-S03-006 — WebAuthn Level 3 Admin Flows + Phishing-Resistant MFA + Cross-Browser (Chrome/Firefox/Safari/Edge)

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-03](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S03-006 |
| Título | WebAuthn Level 3 — registration + authentication + admin flows + cross-browser (Chrome/Firefox/Safari/Edge) + passkey + YubiKey + platform authenticator |
| Sprint | S-03 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (admin op MFA bypass = privilege escalation), FF-HR-005 (WebAuthn cripto = COSE key + attestation chain), FF-HR-009 (Layer 0 — phishing-resistant MFA é primeira linha) |

## 1. Intent

Implementar WebAuthn Level 3 (W3C Recommendation 2024) em `crates/corelink-webauthn/` para fluxos admin críticos:

```rust
pub struct WebAuthnAdapter {
    rp_id: String,                  // Relying Party ID = "corelink.humangr.com" (eTLD+1)
    rp_name: String,                // "CoreLink"
    origins_allowlist: Vec<Url>,    // ["https://app.corelink.humangr.com", "https://admin.corelink.humangr.com"]
    challenge_store: Arc<ChallengeStore>,
    credential_store: Arc<dyn CredentialStore>,  // backed by Neon webauthn_credentials table
}

impl WebAuthnAdapter {
    /// Registration ceremony — generate challenge for new credential.
    pub async fn start_registration(
        &self,
        user: &UserAccount,
        authenticator_attachment: AuthenticatorAttachment,  // Platform | CrossPlatform
    ) -> Result<RegistrationChallenge, WebAuthnError>;

    /// Registration ceremony — verify attestation + persist credential.
    pub async fn finish_registration(
        &self,
        challenge_id: ChallengeId,
        response: RegistrationResponse,
    ) -> Result<CredentialId, WebAuthnError>;

    /// Authentication ceremony — generate challenge for existing user.
    pub async fn start_authentication(
        &self,
        user_account_id: UserAccountId,
    ) -> Result<AuthenticationChallenge, WebAuthnError>;

    /// Authentication ceremony — verify assertion + sign_count + return success.
    pub async fn finish_authentication(
        &self,
        challenge_id: ChallengeId,
        response: AuthenticationResponse,
    ) -> Result<AuthenticationOutcome, WebAuthnError>;
}

#[derive(Debug, Clone, Copy)]
pub enum AuthenticatorAttachment {
    Platform,        // Touch ID, Windows Hello, Android biometrics
    CrossPlatform,   // YubiKey, Solo Key, Titan Key
}

#[derive(Debug, Error)]
pub enum WebAuthnError {
    #[error("invalid challenge OR challenge expired (TTL 300s)")]
    InvalidChallenge,
    #[error("origin mismatch (got {got}, expected one of allowlist)")]
    OriginMismatch { got: String },
    #[error("RP ID hash mismatch")]
    RpIdMismatch,
    #[error("user verification (UV) flag missing — user did not biometric/PIN")]
    UserVerificationMissing,
    #[error("user presence (UP) flag missing")]
    UserPresenceMissing,
    #[error("attestation verification failed: {0}")]
    AttestationInvalid(String),
    #[error("signature invalid")]
    SignatureInvalid,
    #[error("sign_count regression (replay attack? got {got}, stored {stored})")]
    SignCountRegression { got: u64, stored: u64 },
    #[error("credential not found")]
    CredentialNotFound,
    #[error("malformed CBOR/COSE: {0}")]
    Malformed(String),
}
```

Crate dependencies: `webauthn-rs = "0.5"` (audited; W3C L3 compliant); `coset = "0.3"` (COSE key parsing); `ciborium = "0.2"` (CBOR); `subtle` (constant-time comparisons).

**Use cases em S-03**:
1. Admin operations (create/revoke tokens, mass revoke, billing changes) require WebAuthn step-up MFA.
2. Sign-in via Clerk SSO + WebAuthn step-up (Clerk supports WebAuthn natively but CoreLink admin ops require **dedicated WebAuthn ceremony per critical op** — not just "logged in" via Clerk).
3. Passkey enrollment as primary credential (replaces password; phishing-resistant).
4. YubiKey + platform authenticator + iCloud Keychain passkey support (≥ 3 devices typical user; enables recovery).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

WebAuthn é a base do phishing-resistant authentication moderno. Implementação errada = open door para attackers via fake authenticator OR origin spoofing. Bugs catastróficos:

1. **RP ID confusion**: atacante hospeda `evil.corelink.humangr.com.attacker.com`; cliente browser detects `rp_id = "corelink.humangr.com"` mas origin `"evil.corelink.humangr.com.attacker.com"`. Browser rejeita; mas server-side: weak validation pode aceitar. Mitigação: server validates `rp_id` é eTLD+1 (não host); origins allowlist exact match (não prefix match).

2. **Origin allowlist bypass**: dev environment `localhost` allowed; deploy to prod with localhost still in allowlist. Mitigação: env-specific config; production deploy guard rejects localhost in `origins_allowlist`.

3. **User verification (UV) flag bypass**: cliente sends authenticator data com UV flag = 0 (no biometric); server accepts ("user is present, that's enough"). Mitigação: explicitly require `flags & UV != 0` em authentication path; reject UV=0 (anti-phishing weak factor).

4. **sign_count regression**: cliente sends sign_count = 5; stored sign_count = 10. **Replay attack signal** OR cloned authenticator. Mitigação: reject + alert SEV-1 + force credential re-registration. Documented em `webauthn-rs` as W3C-compliant behavior.

5. **Attestation verification skip**: easier impl skips attestation check ("trust user's authenticator"). Atacante uses fake authenticator without legit chain. Mitigação: attestation **required em registration** (Direct attestation); validates AAGUID against FIDO MDS (Metadata Service); maintains AAGUID allowlist (e.g., reject deprecated authenticators).

6. **Challenge replay**: server generates challenge, stores, expires after some time. Bug: TTL too long (1h+) = atacante captures challenge + replays. Mitigação: TTL 300s (5min); challenge stored em CF KV com TTL native; one-time use (delete after consume).

7. **CBOR/COSE parsing bugs**: malformed CBOR causes panic OR untrusted parse. Mitigação: `ciborium` crate (audited); fuzz testing; reject on parse error.

8. **Cross-browser inconsistencies**: Chrome handles attestation differently than Firefox; Safari has WebKit-specific quirks (e.g., backup_eligible flag handling); Edge varies em Windows Hello integration. Mitigação: cross-browser CI test matrix + Playwright E2E em todos os 4 browsers.

9. **Counter rollover (sign_count = u32 max)**: rare but possible em authenticators que use counter. `webauthn-rs` handles em u64 storage; mitigated.

10. **Backup eligible / Backup state mishandling** (passkey ecosystem): credentials sync via iCloud Keychain etc. `backup_eligible` + `backup_state` flags em registration — server must support BE = true para passkey enrollment. Bug: server rejects passkey assuming "must be device-bound" → broken passkey support. Mitigação: explicit handling; allow BE=1 com policy decision (enterprise tier may restrict).

**Atacante adversarial scenarios**:

- **Phishing attack**: atacante hosts `corelink.humangr.com-evil.com`; user navigates accidentally; browser refuses to send credential (origin mismatch enforced by browser); WebAuthn é phishing-resistant by design. Defense é client-side mostly; server reinforces via origin allowlist.

- **Authenticator cloning**: atacante steals YubiKey + extracts private key (extremely difficult; tamper-resistant HW). If success, sign_count regression catches replay. Mitigação layered: HW tamper resistance + sign_count + audit anomaly detection.

- **Man-in-the-middle (MitM) com TLS pinning bypass**: atacante intercepts WebAuthn ceremony; tries to inject. Mitigação: WebAuthn ceremony binds to TLS channel ID; MitM detected.

- **Stolen passkey (synced via cloud)**: user's iCloud account compromised → atacante restores passkeys. Out-of-scope WebAuthn (account-level security; user responsibility); CoreLink emit `auth.webauthn.new_device_used` event for anomaly detection.

- **Insider abuse during admin op**: insider has WebAuthn credential registered; performs malicious admin op. Mitigação: audit chain logs every WebAuthn-protected op; quorum required (S-XX forward dual-control).

**Risk justification HIGH_RISK**:

- **FF-HR-002**: admin op without WebAuthn = privilege escalation possible (e.g., insider with stolen Clerk session can mass-revoke).
- **FF-HR-005**: WebAuthn é cripto-coordenado boundary; COSE key parsing + CBOR + attestation = multiple cripto layers.
- **FF-HR-009**: phishing-resistant MFA é Layer 0 — primeira linha de defense.
- **Reversibility**: WebAuthn bypass detection difícil (cliente vê "authenticated" badge); proativo property test + Mann-Whitney + cross-browser CI.

11 sign-offs canonical incl. Architect (Crypto SME specialization mandatory for CBOR/COSE + attestation), AppSec (origin allowlist + RP ID), Privacy (biometric data handling — does CoreLink ever see? No — biometric stays em authenticator).

## 3. Customer Impact & Journey

**Persona 1 — User enrolling first passkey**:
- Dashboard → Account → "Add passkey" → browser prompts platform authenticator (Touch ID / Face ID / Windows Hello).
- Server: `start_registration` → returns challenge + RP info + authenticator selection criteria.
- Browser: WebAuthn API navigator.credentials.create() → user biometric → returns RegistrationResponse.
- Server: `finish_registration` → verify attestation + persist credential em Neon (WI-S03-005).
- Customer-visible: enrollment ≤ 30s end-to-end (user biometric ~5s; rest server-side).
- Recovery: customer can register multiple authenticators (passkey + YubiKey + iPhone passkey = 3 devices).

**Persona 2 — Admin performing critical op (revoke all tokens)**:
- Admin dashboard → "Revoke all tokens" → confirm modal with "Use your security key".
- Step-up flow: server `start_authentication` → browser WebAuthn API navigator.credentials.get() → user touches YubiKey OR uses platform.
- Server: `finish_authentication` → verify signature + sign_count + return AuthenticationOutcome.
- If success: admin op proceeds via WI-S03-004 mass revoke.
- Audit: `auth.admin_op.webauthn_authenticated` event em chain.
- Customer-visible: step-up adds ~10s latency; necessary for high-trust ops.

**Persona 3 — Compliance auditor verifying MFA enforcement**:
- Audit query: `SELECT user_account_id, op, webauthn_credential_id, authenticated_at FROM admin_op_audit WHERE webauthn_authenticated = false AND op_type IN (mass_revoke, billing_change, ...)`.
- Expected: 0 rows (admin ops always require WebAuthn step-up).
- Drift = SEV-1 alert + post-mortem.

**SLA addendum**:
- WebAuthn ceremony p99 ≤ 30s end-to-end (includes user interaction).
- Server-side validation p99 ≤ 200ms.
- Cross-browser support: Chrome 116+, Firefox 119+, Safari 17+, Edge 116+ (latest evergreen).
- Authenticator support: passkey (W3C), platform authenticators (Touch ID / Face ID / Windows Hello / Android biometrics), CTAP2 hardware keys (YubiKey 5+, Solo 2, Titan).

## 4. Capability Mapping

- **CAP-AUTH-008** (WebAuthn MFA Level 3) — IMPLEMENTA primary.
- **CAP-AUTH-009** (Phishing-resistant admin ops) — IMPLEMENTA primary.
- Trace: `auth_model.md §1.6 (Service identity exclusion: WebAuthn is human-only)` + `security_model.md CTRL-AUTH-010 (MFA + session bind)` + `key_management.md §3.13 (column encryption for WebAuthn keys via WI-S03-005)`.

## 5. Tipo

WebAuthn cripto adapter + admin flow integration; HIGH_RISK; FF-HR-002 + FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-webauthn/` workspace member**:
   - `WebAuthnAdapter` struct + Cargo.toml deps.
   - `webauthn-rs = "0.5"` (audited W3C L3-compliant).
   - `coset` (COSE key parsing).
   - `ciborium` (CBOR).
   - `subtle` (constant-time).

2. **Registration ceremony** (`start_registration` + `finish_registration`):
   - Generate 32-byte CSPRNG challenge.
   - Store em CF KV: `webauthn:challenge:<challenge_id>` TTL 300s.
   - Return `RegistrationChallenge` proto W3C-compliant.
   - On finish: verify attestation, parse COSE public key, validate AAGUID against FIDO MDS allowlist, persist credential em `webauthn_credentials` table (WI-S03-005).
   - Public key encrypted via pgcrypto (column-level; vide WI-S03-005 §6.1.2).

3. **Authentication ceremony** (`start_authentication` + `finish_authentication`):
   - Generate challenge + lookup credential by `user_account_id` (allow_list per credential discovery).
   - Verify assertion: signature OK + sign_count > stored sign_count + UV flag set + UP flag set + RP ID hash match + origin in allowlist.
   - On success: update `last_used_at` + `sign_count` em DB (WI-S03-005); emit audit `auth.webauthn.authenticated`.

4. **Admin op step-up integration**:
   - Tower middleware layer `require_webauthn_step_up()` aplicável per route.
   - Routes protected: mass revoke, billing changes, tenant DELETE, admin user management.
   - Step-up token: 5min TTL; bound to (user, admin_op_id, webauthn_credential_id).
   - Reuse: subsequent admin ops em mesma session com step-up token válido skip extra ceremony.

5. **AAGUID allowlist + FIDO MDS**:
   - Maintain allowlist em config: known-trustworthy AAGUIDs (YubiKey 5 series, Touch ID, Windows Hello, etc.).
   - On finish_registration: lookup AAGUID em `aaguid_allowlist`; reject if missing OR explicit denylist.
   - FIDO MDS (Metadata Service) integration: pull metadata; verify authenticator class (HW vs SW); enforce policy (enterprise tier may require HW only).

6. **Cross-browser CI test matrix** (Playwright):
   - Chrome 116+ (latest stable + previous).
   - Firefox 119+ (latest + ESR).
   - Safari 17+ (macOS + iOS Simulator).
   - Edge 116+ (Chromium-based; subset of Chrome but distinct in Windows Hello integration).
   - 4 browsers × 4 ceremonies (register, authenticate, ceremony with hardware key, ceremony with platform) = 16 test scenarios.

7. **Métricas**:
   - `corelink.auth.webauthn.registration_total{result}` (counter; result ∈ ok|attestation_failed|aaguid_denied|origin_mismatch|...).
   - `corelink.auth.webauthn.authentication_total{result}` (counter).
   - `corelink.auth.webauthn.duration_ms_bucket{ceremony, browser}` (histogram).
   - `corelink.auth.webauthn.aaguid_count{aaguid}` (counter; popularity tracking).
   - `corelink.auth.webauthn.sign_count_regression_total` (counter; alert > 0).
   - `corelink.auth.webauthn.admin_op_step_up_total{op}` (counter).

8. **Property tests** (10k iter PR; 100k nightly):
   - `prop_challenge_uniqueness`: 100k challenges; assert all unique (32-byte CSPRNG; collision ~0).
   - `prop_sign_count_monotonic`: 1k random ceremonies per credential; assert sign_count strictly increasing OR alert.
   - `prop_origin_allowlist_strict`: 1000 random origins; assert exact match (no prefix bypass).
   - `prop_replay_resistance`: capture valid response; replay; assert SignCountRegression.

9. **Adversarial regression tests** (W3C spec known attacks):
   - `test_alg_none_rejected`: malformed COSE alg field → reject.
   - `test_origin_spoof`: `evil.corelink.humangr.com.attacker.com` rejected.
   - `test_rp_id_confusion`: cliente sends rp_id = "attacker.com" → reject.
   - `test_uv_required`: UV=0 in admin step-up → reject.
   - `test_attestation_chain_invalid`: malformed cert chain → reject.
   - `test_aaguid_denylist`: deprecated authenticator AAGUID → reject.

10. **Recovery flow** (Lote 10.3-tris P0-R5-002a fix — magic link REPLACED com 6-digit OTP; sprint contract §10 anti-scope alignment):
    - User loses device → registers backup authenticator pre-emptively (best practice em UX).
    - If all credentials lost: account recovery via **Clerk SSO email + 6-digit OTP code** (NOT magic link — Lote 10.3-tris P0-R5-002a fix; sprint contract §10 anti-scope explicitly rejects magic link due to phishing-prone clicks):
      - **OTP generation**: server-side cryptographically random 6 decimal digits (0-9; ~20 bits entropy); generated via `rand::OsRng` em DPO authority server.
      - **TTL**: 10 minutes (fixed; documented em ADR-0032).
      - **Single-use**: persisted em `auth_recovery_otp(user_id, otp_hash, expires_at_ms, consumed_at_ms)` — UNIQUE on user_id ensures only one active OTP per user; `consumed_at_ms IS NOT NULL` rejects re-use.
      - **Hashing at rest**: OTP NEVER stored plaintext; Argon2id-hashed (same parameters as PAT) before INSERT; verified via constant-time compare.
      - **Delivery**: email channel (Clerk SSO email infrastructure); subject line + body include "do NOT click links — type the 6 digits manually" anti-phishing instruction.
      - **Rate limiting**: 3 OTP generation attempts per user per hour; 5 verify attempts per OTP before invalidation; circuit breaker via WI-S03-008 chaos suite.
      - **Audit**: emit `auth.webauthn.recovery_otp_generated` (pre-issue) + `auth.webauthn.recovery_otp_consumed` (post-success) + `auth.webauthn.recovery_otp_failed` (verify fail) — all in audit chain S-09.
    - Tracked em `auth.webauthn.recovery_initiated` + 3 new sub-events audit chain.
    - Cross-reference: ADR-0032 §A1 "OTP-vs-magic-link rationale" (forward addendum to be added in Lote 10.3-tris).

11. **rustdoc + 4 examples**:
    - `examples/passkey_enroll.rs`.
    - `examples/yubikey_admin_op.rs`.
    - `examples/cross_browser_test.rs`.
    - `examples/recovery_flow.rs`.

### 6.2 Out-of-scope (deferred)

- **WebAuthn for non-admin operations** (require for every PAT verify): cost prohibitive; PAT verification is sufficient com WebAuthn step-up em admin only.
- **Resident key (discoverable credentials) UX**: pós-GA optimization (current uses allow_list).
- **Passwordless sign-in primary** (WebAuthn-only without Clerk SSO): pós-GA; Clerk handles SSO; CoreLink admin step-up.
- **CTAP HID transport mocking em CI**: testing is browser-based via Playwright; no native CTAP testing (real authenticator emulation in CI is complex).
- **WebAuthn for service identity (mTLS executor)**: Fase 2 (Remote Execution).
- **Custom authenticator (CoreLink-branded)**: pós-GA (not core CoreLink mission).
- **FIDO2 NFC support**: covered by browser; no special server config needed.

## 7. Anti-Scope

- ❌ TOTP/SMS as fallback (non-phishing-resistant; defeats purpose).
- ❌ U2F (legacy WebAuthn predecessor; deprecated).
- ❌ Origin allowlist regex (exact match only).
- ❌ RP ID = full hostname (must be eTLD+1).
- ❌ Skip attestation em registration (mandatory).
- ❌ Accept UV=0 em admin step-up (require biometric/PIN).
- ❌ Challenge TTL > 600s (replay surface).
- ❌ Custom CBOR parser (use ciborium audited).
- ❌ Custom COSE key parser (use coset).
- ❌ Resident key only (require allow_list em authentication for now).
- ❌ Single-credential-per-user (allow ≥ 3 for recovery).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: WebAuthn Level 3 — registration + authentication + admin step-up

  Background:
    Given WebAuthnAdapter configured com:
      rp_id = "corelink.humangr.com"
      rp_name = "CoreLink"
      origins_allowlist = ["https://app.corelink.humangr.com", "https://admin.corelink.humangr.com"]
    And aaguid_allowlist contains YubiKey 5 + Touch ID + Windows Hello + Android biometrics + iCloud Keychain passkey
    And FIDO MDS metadata cached (24h TTL)

  Scenario: Register passkey via Touch ID (Safari macOS)
    Given user U_1 logged in via Clerk
    When start_registration(U_1, AuthenticatorAttachment::Platform) called
    Then challenge generated 32-byte CSPRNG; KV TTL 300s
    And RegistrationChallenge returned com authenticatorSelection.userVerification="required"
    When user touches Touch ID; browser returns RegistrationResponse
    When finish_registration(challenge_id, response) called
    Then attestation verified; AAGUID em allowlist
    And public_key COSE parsed; encrypted via pgcrypto (WI-S03-005)
    And credential persisted em webauthn_credentials table
    And metric corelink.auth.webauthn.registration_total{result="ok"} incremented

  Scenario: Register YubiKey 5 (Chrome on Windows)
    Given user U_1 with passkey enrolled
    When start_registration(U_1, AuthenticatorAttachment::CrossPlatform) called
    Then RegistrationChallenge returned
    When user inserts YubiKey + touches
    Then finish_registration verifies; AAGUID YubiKey 5 em allowlist
    And second credential persisted; user has 2 credentials total

  Scenario: Authentication ceremony — happy path
    Given user U_1 with 2 credentials enrolled
    When start_authentication(U_1) called
    Then AuthenticationChallenge with allow_list = [cred_passkey, cred_yubikey] returned
    When user picks YubiKey + touches
    And browser sends AuthenticationResponse
    When finish_authentication(challenge_id, response) called
    Then signature verified; sign_count > stored sign_count
    And UV flag set; UP flag set
    And RP ID hash match; origin em allowlist
    And sign_count updated em DB; last_used_at = now()
    And audit "auth.webauthn.authenticated" emitted to outbox
    And AuthenticationOutcome::Success returned

  Scenario: Admin op step-up enforced
    Given user U_1 admin role; logged in via Clerk
    When admin attempts POST /api/v1/admin/tokens/revoke-all
    Then middleware (WI-S03-003) detects route protected
    And require_webauthn_step_up layer engaged
    And user prompted for WebAuthn ceremony
    When user completes; step_up_token issued (5min TTL; bound to op + cred_id)
    Then admin op proceeds; mass revoke executes (WI-S03-004)
    And audit "auth.admin_op.webauthn_authenticated" event emitted

  Scenario: Origin spoof rejected
    Given browser sends ceremony with origin = "https://evil.corelink.humangr.com.attacker.com"
    When finish_authentication validates origin
    Then origin NOT em allowlist (allowlist exact match)
    And response 401 com error_code COR_AUTH_ORIGIN_MISMATCH
    And audit "auth.webauthn.origin_attack_attempt" emitted

  Scenario: RP ID confusion rejected
    Given browser sends ceremony with rp_id = "attacker.com"
    When finish_authentication validates RP ID hash
    Then hash mismatch; response 401 com error_code COR_AUTH_RP_ID_MISMATCH

  Scenario: User Verification (UV) flag missing in admin step-up
    Given user uses authenticator without biometric (UV=0; e.g., security key without PIN)
    When admin step-up requires UV=1
    Then finish_authentication returns UserVerificationMissing
    And response 403 com error_code COR_AUTH_UV_REQUIRED
    And user prompted to use authenticator with biometric/PIN

  Scenario: sign_count regression detected (replay attack signal)
    Given credential C_1 stored sign_count = 100
    When AuthenticationResponse arrives with sign_count = 50
    Then SignCountRegression detected
    And response 401 com error_code COR_AUTH_SIGN_COUNT_REGRESSION
    And metric corelink_auth_webauthn_sign_count_regression_total += 1 (canonical underscored per observability_model.md §4.1)
    And **SEV-2 alert fires** (first regression; W3C-compliant policy per Lote 10.3-tris P0-R5-002b + cycle 4 codex SEAL alignment com §28 R-004 + §31 v1.2.0; SEV-1 only após forensic confirmation)
    And user notified via email "potential security issue with your authenticator"

  Scenario: Challenge expired (TTL 300s)
    Given challenge generated at T+0
    When finish_authentication called at T+301s
    Then challenge KV expired
    And response 401 com error_code COR_AUTH_CHALLENGE_EXPIRED
    And user retries from start

  Scenario: AAGUID denylist (deprecated authenticator)
    Given AAGUID = <deprecated YubiKey 4>
    When finish_registration called
    Then AAGUID em denylist
    And response 403 com error_code COR_AUTH_AUTHENTICATOR_DEPRECATED
    And user prompted to use YubiKey 5 OR newer

  Scenario: Cross-browser CI matrix passes
    Given Playwright CI runs Chrome + Firefox + Safari + Edge
    When 16 test scenarios (4 browsers × 4 ceremonies)
    Then 16/16 pass em CI
    And browser-specific quirks documented em report

  Scenario: Passkey backup_eligible handling
    Given user enrolls passkey on iPhone (backup_eligible = true; backup_state = false initially)
    When passkey syncs to iPad via iCloud Keychain
    And user authenticates from iPad (backup_state = true after sync)
    Then server accepts; metric corelink.auth.webauthn.passkey_synced_total += 1
    And subsequent ceremonies from iPad valid
```

## 9. Design Decisions

### 9.1 Why webauthn-rs 0.5 (não hand-roll)

- W3C Level 3 spec is intricate (CBOR, COSE, attestation, AAGUID, MDS).
- `webauthn-rs` 0.5 (2024) é audited; W3C compliant; FIDO MDS integration; cross-browser tested upstream.
- Hand-roll = full security posture loss; out of scope for spec-first iteration.
- Compatible com axum + tonic ecosystem.

### 9.2 Why RP ID = eTLD+1 ("corelink.humangr.com")

- W3C spec: RP ID must be domain or eTLD+1; not subdomain.
- Allows credentials shared across `app.corelink.humangr.com` + `admin.corelink.humangr.com` + `api.corelink.humangr.com`.
- Cookies + WebAuthn align em scoping.

### 9.3 Why UV flag required em admin step-up

- UV (User Verification) = biometric OR PIN.
- UP alone = "user is present" (touch); insufficient for admin (e.g., insider grabs unlocked YubiKey).
- UV = phishing-resistant + attended-session signal.
- Trade-off: user with UV=0 only authenticator (e.g., bare YubiKey without PIN) blocked from admin → must enroll PIN OR use different authenticator. Acceptable for security tier.

### 9.4 Why challenge TTL 300s

- < 60s: UX hostile; user typing PIN takes time.
- 300s = practical balance.
- > 600s: replay surface broadens.

### 9.5 Why AAGUID allowlist em config (não pure FIDO MDS)

- FIDO MDS é authoritative source mas: (a) network call; (b) MDS itself can be compromised in transit.
- Allowlist em config = known-good explicit; CoreLink admin operates allowlist.
- MDS enriches metadata (transports, etc.) mas final decision em CoreLink config.
- Rejected: MDS-only (single source of truth concern; TOFU dynamics).

### 9.6 Why step-up token (não re-ceremony per op)

- Single ceremony for series of admin ops em same session = UX win (avoid 5× WebAuthn prompts).
- Token TTL 5min bounded.
- Token bound to (user_id, op_class, cred_id) — cannot be reused for different op class.

### 9.7 Why public key encrypted (pgcrypto column)

- Public key is technically not secret (it's PUBLIC) but in WebAuthn ecosystem, leak of all enrolled public keys + AAGUIDs gives attacker:
  - Map of authenticator types per user (privacy implication).
  - Ability to forge attestation in some weak attestation modes.
- Encrypted at rest mitigates insider exfiltration impact (defense-in-depth).
- Note: COSE public keys are not high-value secrets; encryption is mostly privacy + cautionary; cost is minimal.

### 9.8 Why cross-browser CI matrix mandatory

- WebAuthn implementation drift between browsers em em corner cases: Safari WebKit attestation handling, Firefox CBOR parsing differences, Edge Windows Hello integration, Chrome passkey UI.
- Bug em cross-browser = production breakage para minority users.
- Matrix em CI = early detection.

### 9.9 Why sign_count regression policy is W3C-COMPLIANT (Lote 10.3-tris P0-R5-002b)

**Lote 10.3-tris fix** — corrige contradição entre §9.9 ("first regression = SEV-1") e §28 R-004 ("≥ 3 regressions in 24h = alert threshold"). Ambas não podem ser true. Resolução adopta W3C WebAuthn L3 §6.1.1 recomendação:

**Policy** (3 cases per W3C):
1. **`sign_count = 0` always (passkey behavior)**: authenticator nunca incrementa counter. **EXEMPT** from regression tracking — não é replay signal; é configuração canônica de passkey. Sign in/out repetidamente é OK; storage mantém `sign_count = 0` permanentemente.
2. **`sign_count` initial = 0, then non-zero**: authenticator começou em 0 e está agora reportando counter. Track from primeira non-zero observation; regression check against last non-zero stored.
3. **`sign_count` initial ≥ 1 + regression detected**: este é o cloned-authenticator OR replay signal:
    - **First regression**: SEV-2 alert (page SRE; investigate; do NOT page user). Document em `auth.webauthn.sign_count_regression` audit event with full context (got, stored, credential_id, ip, user_agent, prior_8_authentications_history).
    - **Forensic confirmation** (≥ 2 independent signals: SEV-2 alert ack + IP geolocation mismatch ≥ 1000km from prior auth + user_agent fingerprint mismatch): **escalate to SEV-1**; force credential re-registration; notify user via secure channel (NOT the same channel as auth).
    - **Threshold "≥ 3 in 24h" REMOVED** — was creating 24h-72h false safety window where real cloned attack absorbed silently. Sonnet R5 P0-R5-002b confirms removal.

**Why this is correct**: passkeys legitimately report `sign_count = 0` always (Apple iCloud Keychain, 1Password, etc.). Original §9.9 SEV-1-on-first-regression would fire spurious alerts on every passkey login from compliant authenticators → alert fatigue → real cloned-authenticator attack ignored. W3C-compliant policy distinguishes signal from noise.

**Audit emission** (added em §6.1.x):
- `auth.webauthn.sign_count_observation` (every auth; counter value persisted; deltas captured for forensic).
- `auth.webauthn.sign_count_regression_detected` (SEV-2; first regression).
- `auth.webauthn.sign_count_regression_confirmed` (SEV-1; forensic confirmation).

**§28 R-004 update**: remove "≥ 3 in 24h" threshold; replace with "single regression = SEV-2 + investigation; forensic confirmation = SEV-1 + force re-registration".

**Reference**: W3C WebAuthn L3 §6.1.1 "Authenticator Counters"; FIDO Alliance Security Reference v2.2 §3.4.

### 9.10 ADR potencial?

Sim — **ADR-0032**: "WebAuthn Level 3 implementation strategy + AAGUID allowlist policy + step-up flow design". Documenta trade-offs (UV required, challenge TTL, encrypted public key, cross-browser matrix). Whitelist em validate_references.py.

## 10. Completeness Criteria SOTA

- [ ] **10.5.1** Property tests 10k iter (PR) + 100k iter (nightly) → 0 panics, 0 false-accepts (EVT-002).
- [ ] **10.5.2** Adversarial regression tests (6 attack classes) — 100% rejected (EVT-040).
- [ ] **10.5.3** Cross-browser CI matrix 16/16 scenarios green (Chrome + Firefox + Safari + Edge) (EVT-018).
- [ ] **10.5.4** Real authenticator E2E test em staging com pelo menos: 1 passkey + 1 YubiKey 5 + 1 Touch ID + 1 Windows Hello (EVT-018).
- [ ] **10.5.5** Admin step-up flow E2E green: mass revoke + billing change + tenant DELETE all require WebAuthn (EVT-040).
- [ ] **10.5.6** Sign_count regression detection: synthetic replay → 100% caught + **SEV-2 alert** (first regression; W3C-compliant per P0-R5-002b + cycle 4 codex SEAL) (EVT-022).
- [ ] **10.5.7** Cargo-audit + cargo-deny clean (no CVE em webauthn-rs OR coset OR ciborium).
- [ ] **10.5.8** Cargo-fuzz target em CBOR/COSE deserialize 1h corpus; 0 panics (EVT-008).
- [ ] **10.5.9** OWASP ASVS V2.1 (general authenticator security) + V2.5 (multi-factor authenticator) 100% pass (EVT-002).
- [ ] **10.5.10** FIDO Alliance compliance test suite (open-source); pass 100% applicable tests.
- [ ] **10.5.11** Cost regression gate: WebAuthn ceremony p99 ≤ 200ms server-side; CI bench.

## 11. DoD

- [ ] `corelink-webauthn` crate compila + integration tests green.
- [ ] All Gherkin scenarios green.
- [ ] Property tests 10k green em CI.
- [ ] Cross-browser Playwright CI matrix 16/16 green.
- [ ] Real authenticator E2E em staging green.
- [ ] Admin step-up flows tested (mass revoke + billing + tenant DELETE).
- [ ] Cargo-audit + cargo-deny + cargo-fuzz clean.
- [ ] Métricas (6 listadas §6.1.7) emitted.
- [ ] AAGUID allowlist + denylist configured + reviewed.
- [ ] FIDO MDS integration cached + tested.
- [ ] rustdoc + 4 examples.
- [ ] ADR-0032 published.
- [ ] Crypto SME + AppSec + Privacy + Security Lead + UX Researcher reviews.
- [ ] PRR Architect sign-off.

## 12. Invariants Validated

- **INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN** (CRITICAL): admin step-up demands UV=1; CI test enforces.
- **INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED** (CRITICAL): registration verifies attestation; cargo-fuzz + adversarial test.
- **INV-AUTH-WEBAUTHN-SIGN-COUNT-MONOTONIC** (HIGH): sign_count strictly increasing; replay regression catches.
- **INV-AUTH-WEBAUTHN-ORIGIN-EXACT** (CRITICAL): origin allowlist exact match (no prefix bypass).
- **INV-AUTH-WEBAUTHN-RP-ID-CANONICAL** (CRITICAL): RP ID = "corelink.humangr.com" eTLD+1; never subdomain.

TLA+ alignment: planned `webauthn_ceremony.tla` (S-09 ou pós); modela challenge → response → verify state machine.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| `corelink-webauthn` crate | `crates/corelink-webauthn/` | Rust workspace member |
| WebAuthnAdapter | `crates/corelink-webauthn/src/adapter.rs` | Rust |
| ChallengeStore (CF KV) | `crates/corelink-webauthn/src/challenge_store.rs` | Rust |
| CredentialStore (Neon) | `crates/corelink-webauthn/src/credential_store.rs` | Rust |
| Step-up middleware | `crates/corelink-worker/src/middleware/webauthn_step_up.rs` | Rust |
| AAGUID allowlist config | `config/aaguid_allowlist.toml` | TOML |
| Property tests | `crates/corelink-webauthn/tests/prop_webauthn.rs` | Rust |
| Adversarial regression | `crates/corelink-webauthn/tests/adversarial.rs` | Rust |
| Cross-browser Playwright | `e2e/webauthn/` (Playwright JS) | TypeScript |
| Fuzz target CBOR/COSE | `crates/corelink-webauthn/fuzz/fuzz_targets/cose_parse.rs` | cargo-fuzz |
| ADR-0032 | `specs/03_architecture/adrs/ADR-0032-webauthn-level3.md` | Markdown |
| Examples (4) | `crates/corelink-webauthn/examples/` | Rust |

## 14. Quality Standards SOTA

- **14.5.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.5.2** rustdoc 100% public API + 4 examples + threat model README.
- **14.5.3** Test coverage ≥ 95% (cripto crate; higher bar).
- **14.5.4** Latência: ceremony server-side p99 ≤ 200ms; end-to-end p99 ≤ 30s (includes user interaction).
- **14.5.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` + cargo-fuzz 1h CBOR/COSE.
- **14.5.6** Métricas RED + per-ceremony histograms + cross-browser breakdown.
- **14.5.7** Runbook: RB-FM-WEBAUTHN-MDS-OUTAGE.
- **14.5.8** Breaking changes em ChallengeStore/CredentialStore = bump major + dual-write window.
- **14.5.9** Memory bounded: per-ceremony ≤ 16 KiB stack (CBOR/COSE parsing).
- **14.5.10** Cost regression gate: ceremony p99 ≤ 200ms; CI bench.

## 15. Chaos Experiments

1. **Origin spoof attack**: red team simulates `evil.corelink.humangr.com.attacker.com` ceremony; verify rejection. Hypothesis: exact match wins. Procedure: Playwright simulate; assert.

2. **Replay attack with stale response**: capture valid ceremony response; replay 5 min later; verify SignCountRegression OR challenge expired catches.

3. **AAGUID denylist enforcement**: synthetic deprecated AAGUID (YubiKey 4); verify rejection.

4. **CBOR malformed input fuzz**: cargo-fuzz 1h corpus; 0 panics. Hypothesis: ciborium robust.

5. **Cross-browser quirk discovery**: full Playwright matrix + observe breakage. Document edge cases per browser.

6. **FIDO MDS outage**: simulate MDS endpoint 503; verify graceful degradation (use cached metadata 24h TTL).

7. **Admin step-up bypass attempt**: synthetic patch removing step-up middleware from route; verify integration test catches.

8. **Challenge KV outage**: simulate KV unavailable; verify ceremony fails gracefully (503; user retries).

9. **High-volume registration storm**: 100 users simultaneously enroll passkey; verify backend handles + Worker memory bound.

10. **Recovery flow** (lost device): user with single credential loses device; tests **OTP recovery flow** (Clerk SSO email + 6-digit OTP per Lote 10.3-tris P0-R5-002a + ADR-0032 §A1; magic link explicitly REJECTED em sprint contract §10 anti-scope) → re-register → invalidate old. Cycle 4 codex SEAL.

## 16. PRR

PRR HIGH_RISK 11 sign-offs canonical gated em WI-S03-008 ship gate.

- [ ] All Gherkin green.
- [ ] Adversarial regression + cross-browser green.
- [ ] Real authenticator E2E green.
- [ ] FIDO compliance suite pass.
- [ ] OWASP ASVS V2.1+V2.5 100%.
- [ ] ADR-0032 published.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate scaffold + Cargo.toml deps | 1.5h |
| ST-002 | WebAuthnAdapter struct + builder | 2h |
| ST-003 | ChallengeStore (CF KV) impl | 2h |
| ST-004 | CredentialStore (Neon integration) | 2.5h |
| ST-005 | start_registration ceremony | 3h |
| ST-006 | finish_registration + attestation verification | 4h |
| ST-007 | start_authentication ceremony | 2h |
| ST-008 | finish_authentication + sign_count check | 3h |
| ST-009 | AAGUID allowlist + FIDO MDS integration | 4h |
| ST-010 | Step-up middleware + token issuance | 3h |
| ST-011 | Métricas (6) + trace spans | 2h |
| ST-012 | Property tests 10k iter | 3h |
| ST-013 | Adversarial regression tests (6 attack classes) | 4h |
| ST-014 | Cross-browser Playwright matrix (16 scenarios) | 8h |
| ST-015 | Real authenticator E2E em staging | 4h |
| ST-016 | Cargo-fuzz CBOR/COSE 1h corpus | 2h |
| ST-017 | Recovery flow integration | 3h |
| ST-018 | rustdoc + 4 examples + threat model README | 3h |
| ST-019 | ADR-0032 redação | 2h |
| ST-020 | Crypto SME + AppSec + Privacy + UX review iteration | 4h |
| ST-021 | OWASP ASVS V2.1 + V2.5 self-checklist | 2h |
| ST-022 | FIDO Alliance compliance suite run | 3h |

**Total Optimistic**: ~66h. **PERT** (O=58h, M=66h, P=98h): **~71h**.

## 18. Dependencies

### Hard blockers

- WI-S03-005 (Neon schema) — `webauthn_credentials` table must exist.
- WI-S03-003 (middleware) — step-up layer integration.
- AAGUID allowlist sourced (operations team task; ~1 week initial curation).
- Cross-browser CI infrastructure (Playwright + browser binaries em runners).
- Real authenticator hardware available em staging (≥ 3 YubiKey 5 + Touch ID-equipped Mac + Windows Hello PC + Android device).

### Soft blockers

- FIDO MDS network availability (degradação graceful).
- WI-S03-007 (audit events) — não bloqueante; consumer.

### Outbound

- WI-S03-008 (ship gate).
- WI-S13-XXX admin plane consume step-up middleware.

## 19. Effort PERT

O: 58h, M: 66h, P: 98h → PERT **71h**.

## 20. Time-boxing

**80h hard limit**. Se exceder → escalation: split em sub-WI (core ceremonies vs cross-browser CI matrix vs admin step-up integration).

## 21. Observability

6 métricas listadas §6.1.7. Trace span `webauthn.ceremony` com attributes:
- `webauthn.ceremony` (registration|authentication)
- `webauthn.result` (ok|attestation_failed|aaguid_denied|...)
- `webauthn.aaguid` (string; popularity tracking)
- `webauthn.browser` (chrome|firefox|safari|edge|other; from User-Agent)
- `webauthn.duration_ms`

Logs structured JSON; INFO em ok; WARN em rejected; ERROR em sign_count_regression.

Dashboard widget DASH-AUTH:
- Ceremony rate per type.
- Cross-browser breakdown.
- AAGUID popularity.
- Sign_count regression rate (alert > 0).
- Admin step-up frequency.

## 22. Cost Analysis

**Per-ceremony cost** (server-side):
- Worker invocation: $0.50/M.
- KV (challenge store): $0.50/M.
- Neon query (credential lookup + sign_count update): ~$1/M.
- COSE/CBOR parse CPU: ~5ms.
- Per-ceremony total: ~$0.000005.

**TCO 12m projection** (assume 100 admin ops/dia × 11 sign-offs canonical em S-03 + 100k registrations/yr + 1M authentications/yr):
- Registrations: 100k × $0.000005 = $0.50/yr.
- Authentications: 1M × $0.000005 = $5/yr.
- Cross-browser CI: ~$200/mês compute = $2.4k/yr.
- FIDO MDS: free.
- **Total**: ~$2.4k/yr (dominated by CI infra).

**Cost regression gate**: ceremony p99 ≤ 200ms; ≤ $0.000010 per ceremony.

## 23. API Contract

- POST `/api/v1/webauthn/register/start` → returns RegistrationChallenge.
- POST `/api/v1/webauthn/register/finish` → returns CredentialId.
- POST `/api/v1/webauthn/authenticate/start` → returns AuthenticationChallenge.
- POST `/api/v1/webauthn/authenticate/finish` → returns AuthenticationOutcome (+ step_up_token se admin context).
- DELETE `/api/v1/webauthn/credentials/:id` → deletes credential (recovery flow).
- GET `/api/v1/webauthn/credentials` → list user credentials.

Erro mapping:
- `InvalidChallenge` → 401 `COR_AUTH_CHALLENGE_INVALID`.
- `OriginMismatch` → 401 `COR_AUTH_ORIGIN_MISMATCH`.
- `RpIdMismatch` → 401 `COR_AUTH_RP_ID_MISMATCH`.
- `UserVerificationMissing` → 403 `COR_AUTH_UV_REQUIRED`.
- `AttestationInvalid` → 403 `COR_AUTH_ATTESTATION_INVALID`.
- `SignCountRegression` → 401 `COR_AUTH_SIGN_COUNT_REGRESSION` + **SEV-2 alert** (first regression W3C-compliant per P0-R5-002b; SEV-1 only após forensic confirmation via §28 R-004 escalation).
- `CredentialNotFound` → 404 `COR_AUTH_CREDENTIAL_NOT_FOUND`.

## 24. Post-mortem Hooks

- WebAuthn bypass exploit detected → CRITICAL post-mortem + breach review.
- Sign_count regression: first event SEV-2 (per Lote 10.3-tris P0-R5-002b W3C-compliant policy + cycle 4 codex SEAL); forensic confirmation escalates to SEV-1 + force re-registration + user notification. **NOTE: "≥3 events em 24h" threshold REMOVED per P0-R5-002b**; cycle 5 codex SEAL alignment.
- Cross-browser CI matrix red sustained > 1 day → SEV-2.
- Origin spoof attempts spike → SEV-2 (active attack indicator).
- AAGUID denylist activation (deprecated authenticator detected em prod) → SEV-2 + customer notification.
- FIDO MDS outage > 24h → SEV-3 (cache served).

## 25. Rollback / Recovery

Hot rollback via Wrangler. Per-credential admin override: admin can DELETE credential via API (logged audit).

User recovery: lost device → **6-digit OTP via Clerk SSO email** (per Lote 10.3-tris P0-R5-002a + ADR-0032; magic link rejected em sprint contract §10 anti-scope) → re-register passkey → invalidate old. Cycle 4 codex SEAL.

RTO ≤ 30 min (deploy rollback); RPO 0 (stateless ceremony; credentials in Neon).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: WebAuthn é phishing-resistant; origin + RP ID + attestation prevent. UV required em admin step-up = additional defense.
- **Tampering**: signature verify + sign_count monotonicity + AAGUID allowlist.
- **Repudiation**: audit chain logs all ceremonies; immutable.
- **Information disclosure**: public key encrypted (defense-in-depth); biometric data NEVER leaves authenticator.
- **DoS**: ceremony rate limit S-08 forward; challenge KV bounded.
- **Elevation of privilege**: admin step-up enforced via middleware; insider abuse logged.

**LINDDUN delta**:
- **Linkability**: credentials linked to user_account_id; cross-tenant isolation via membership table.
- **Identifiability**: user_account_id pseudonymous; biometric never seen.
- **Non-repudiation**: append-only audit + sign_count monotonic.
- **Detectability**: ceremony evidence em chain; AAGUID popularity for forensics.
- **Disclosure of information**: response error_codes não revelam tenant size, etc.
- **Unawareness**: dashboard explica WebAuthn; user controls credentials.
- **Non-compliance**: NIST SP 800-63B AAL3 + WebAuthn L3 + FIDO2 = compliance with strongest auth tiers.

## 27. Knowledge Transfer

- **Tech talk** (2h): "WebAuthn Level 3 — phishing-resistant MFA + admin step-up patterns".
- **Doc** `docs/internal/webauthn-architecture.md` — sequence diagrams (registration, authentication, step-up, recovery).
- **Doc** `docs/internal/aaguid-allowlist.md` — allowlist policy + curation workflow.
- **Doc** `docs/internal/cross-browser-quirks.md` — browser-specific notes.
- **ADR-0032** — design rationale.
- **Workshop** (2h): com Crypto SME + AppSec + UX Researcher + handler authors — adversarial walkthrough.
- **Onboarding test** (5 questions): RP ID rationale, UV vs UP, attestation purpose, sign_count semantics, step-up token TTL.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | webauthn-rs CVE em upgrade | L | M | CRITICAL | L | LOW | cargo-audit weekly + version pin + property test |
| R-002 | Cross-browser drift breaks production | M | H | HIGH | M | LOW | CI matrix 16 scenarios + report quirks |
| R-003 | AAGUID allowlist outdated (new authenticators) | M | M | LOW | M | LOW | Quarterly review + customer feedback channel |
| R-004 | sign_count regression false positive (counter reset) | L | M | MEDIUM | L | LOW | **W3C-COMPLIANT POLICY** (Lote 10.3-tris P0-R5-002b): `sign_count = 0` always = EXEMPT (passkey behavior canonical); first regression = SEV-2 + investigate; forensic confirmation (≥2 independent signals: SRE ack + IP geolocation mismatch + UA fingerprint mismatch) = SEV-1 + force credential re-registration. Threshold "≥ 3 in 24h" REMOVED — was creating 24h-72h false safety window absorbing real cloned-authenticator attacks silently. |
| R-005 | Origin allowlist misconfigured em prod (localhost) | L | L | CRITICAL | L | LOW | Prod deploy guard + chaos PR |
| R-006 | Attestation chain compromised (CA breach) | L | L | CRITICAL | L | LOW | FIDO MDS validates chain; allowlist enforces |
| R-007 | UV bypass via legacy authenticator | L | M | HIGH | L | LOW | UV required em admin; allow_credentials filtered |
| R-008 | Challenge replay (TTL > 300s drift) | L | L | HIGH | L | LOW | TTL hard-coded + KV native expiry |
| R-009 | CBOR/COSE parser panic | L | M | HIGH | L | LOW | ciborium audited + cargo-fuzz 1h |
| R-010 | Recovery flow abuse (account takeover via Clerk SSO) | L | M | CRITICAL | L | LOW | Email verification + audit + rate limit |
| R-011 | FIDO MDS outage breaks new registrations | L | H | MEDIUM | L | LOW | 24h cache; degradation graceful |
| R-012 | Admin step-up bypass via missing layer | L | H | CRITICAL | L | LOW | Compile-time route definition + integration test |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME review WebAuthn flow + step-up.
2. **Crypto (D+3)**: Crypto SME pair-program attestation + COSE parsing.
3. **Code (D+7)**: peer review.
4. **Adversarial (D+9)**: red team session — origin spoof, replay, RP confusion.
5. **AppSec (D+10)**: AppSec review insider abuse + recovery flow.
6. **UX (D+11)**: UX Researcher review user flows + cross-browser experience.
7. **Cross-browser (D+13)**: Playwright matrix runtime.
8. **PRR (D+14)**: Architect sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; recovery flow design review_ (com Crypto SME specialization mandatory: CBOR/COSE + attestation + WebAuthn Level 3 spec compliance (mandatory pair-program)) | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 6 | Engineer (S-03 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; biometric data handling (no server-side; authenticator-only)_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — origin allowlist + RP ID + magic link removal validation_ | _pending_ | _pending_ |

> Crypto SME folds into Architect role specialization (cycle 1 codex SEAL alignment per framework §33.5.4.3 + ADR-0034 solo-tier waiver). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S03-006 (Lote 10.3); SOTA full (W3C L3 + 4 browsers + passkey + YubiKey + admin step-up + 5 INVs + 10 chaos + 12-row risk + ADR-0032). |
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.3-tris cross-WI sync) | Lote 10.3bis cross-WI references absorbed: `auth.webauthn.new_device_used` event added (coordinates with WI-S03-007 §6.1.3 expanded enum 23→33); webauthn-rs library version pin upgraded para `=0.5.x` exact patch with sha256 `.crate` checksum verification (WI-006 was skipped by Lote 10.3bis cycle — this fixes Sonnet R5 P0-R5-002c version freeze defect). |
| 1.2.0 | 2026-04-25 | Gustavo (Lote 10.3-tris Sonnet R5 P0 fixes) | **P0-R5-002a — Recovery flow magic link → 6-digit OTP** (sprint contract §10 anti-scope alignment); 10min TTL; single-use; Argon2id-hashed at rest; 3 generation/hour + 5 verify/OTP rate limits; 3 audit sub-events. **P0-R5-002b — sign_count W3C-compliant policy** (§9.9 + §28 R-004 contradiction resolved): `sign_count=0` exempt; first regression SEV-2; forensic confirmation SEV-1; "≥3 in 24h" threshold REMOVED. **P0-R5-002c — version freeze closure**: WI-006 now at v1.2.0 matching all other WIs. |
| 1.3.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7) | **SEAL — implementation phase** (charter `2026-04-30 protocol`: no per-WI codex; sprint-close Sonnet adversarial review covers). New crate `crates/corelink-webauthn/` with `WebAuthnEngine` trait + `InMemoryEngine` + `ProductionEngineNotConfigured` sentinel + `ChallengeStore` / `CredentialStore` / `RecoveryOtpStore` traits + in-memory implementations; `RpId` / `Origin` / `OriginAllowlist` exact-match guards (rejects `localhost` / loopback / single-label / non-eTLD+1 / non-`https://` origins / userinfo); `AaguidPolicy` closed-default allowlist + denylist (synthetic AAGUIDs for YubiKey 5, Touch ID, Windows Hello, Android biometrics, iCloud passkey, deprecated YubiKey 4); `parse_cose_algorithm` rejecting `alg: 0` "none"; `AuthenticatorFlags` UP/UV/BE/BS bit accessors; `SignCount` + `assess()` W3C-compliant policy (passkey-exempt for `(0, 0)`; monotonic for `incoming > stored`; `Sev2InvestigationRequired` otherwise); recovery OTP module (`mint_otp` / `verify_otp` Argon2id PHC, `RecoveryChannel` enum locked to `ClerkSsoEmail` only — magic-link unrepresentable at the type level); `StepUpToken` (5min default TTL; constant-time `subtle::ConstantTimeEq` validation; bound to `(user_id, op_class, credential_id)`); `MetricsObserver` trait + `NoopMetrics` + `MetricsRecorder` (canonical 6 metrics from §6.1.7). 21 canonical-vector tests; 14 adversarial regressions; 7 property tests (`prop_challenge_uniqueness_10k`, `prop_origin_allowlist_strict` 10k cases, `prop_sign_count_assess` 10k cases, `prop_aaguid_policy` 10k cases, `prop_replay_resistance_single_use`, `prop_recovery_otp_single_use_100`, `prop_origin_allowlist_no_prefix_bypass_10k`); 4 examples (`passkey_enroll`, `yubikey_admin_op`, `cross_browser_test`, `recovery_flow`). ADR-0032 published. Workspace member added; crate-strict lints (`#![forbid(unsafe_code)]` + Cargo `[lints]` denying `unwrap_used` / `expect_used` / `panic` / `indexing_slicing` / `mod_module_files`); zero clippy warnings under `-D warnings`. The production `webauthn-rs = 0.5` shim is deferred to a downstream WI alongside Cloudflare credentials per charter `§inflection` (CF account is HARD inflection); the `ProductionEngineNotConfigured` sentinel + trait surface freeze the contract. Cross-browser Playwright matrix (`e2e/webauthn/`; 16 scenarios) lands as part of the production-shim WI alongside real authenticators. Frontmatter promoted DRAFT → FROZEN, READY → DONE; v1.2.0 → v1.3.0. |

## 32. Anti-patterns evitados

- ❌ TOTP/SMS as MFA fallback.
- ❌ U2F (legacy).
- ❌ Origin allowlist regex.
- ❌ RP ID = full hostname.
- ❌ Skip attestation em registration.
- ❌ Accept UV=0 em admin step-up.
- ❌ Challenge TTL > 600s.
- ❌ Custom CBOR/COSE parser.
- ❌ Resident key only.
- ❌ Single-credential-per-user.
- ❌ Hard-roll WebAuthn lib.
- ❌ Step-up via session cookie alone (require ceremony per critical op).

---

**Fim WI-S03-006.** Próximo: WI-S03-007 (Audit events EVT-047 + chain integrity S-09 alignment + CloudEvents 1.0 envelope).
