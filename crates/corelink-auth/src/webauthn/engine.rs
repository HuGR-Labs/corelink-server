//! Engine surface — registration + authentication ceremony orchestration.
//!
//! Two engines ship in this crate:
//!
//! 1. [`InMemoryEngine`] — host-side fake used by every property test
//!    and adversarial regression. Enforces the canonical
//!    `INV-AUTH-WEBAUTHN-*` invariants algorithmically without a
//!    real authenticator.
//! 2. [`ProductionEngineNotConfigured`] — a sentinel that returns
//!    [`WebAuthnError::EngineNotConfigured`] from every method. The
//!    real `webauthn-rs = 0.5` adapter is wired in a downstream WI
//!    (charter trait-abstraction-defer pattern; staging Cloudflare
//!    + real authenticators are an inflection point).

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use super::challenge::{
    AuthenticationChallenge, ChallengeId, ChallengePurpose, ChallengeTtl, RegistrationChallenge,
    StoredChallenge,
};
use super::clock::EngineClock;
use super::cose::parse_cose_algorithm;
use super::credential::{Credential, CredentialId, RegistrationResponse};
use super::flags::AuthenticatorFlags;
use super::metrics::{CeremonyResult, MetricsObserver, NoopMetrics};
use super::sign_count::{assess as assess_sign_count, SignCount, SignCountSeverity};
use super::store::{
    AuthenticatorAttachment, ChallengeStore, CredentialStore, InMemoryChallengeStore,
    InMemoryCredentialStore, InMemoryRecoveryOtpStore, RecoveryOtpStore,
};
use super::types::UserAccountId;
use super::{
    AaguidPolicy, ChallengeBytes, Origin, OriginAllowlist, RecoveryRateLimit, RpId, WebAuthnError,
};

/// Ceremony classification — drives the strict UV requirement at
/// finish-time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ceremony {
    /// Standard authentication (UV recommended but not strictly
    /// required; the engine config decides).
    Authentication,
    /// Admin step-up — UV strictly required (`INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN`).
    AdminStepUp,
}

/// Outcome of a successful authentication ceremony.
#[derive(Debug, Clone)]
pub struct AuthenticationOutcome {
    /// Credential that satisfied the assertion.
    credential_id: CredentialId,
    /// User account.
    user: UserAccountId,
    /// Sign-count after canonical assessment.
    persisted_sign_count: SignCount,
    /// Whether the ceremony was admin step-up.
    admin_step_up: bool,
}

impl AuthenticationOutcome {
    /// True when the ceremony was a successful authentication.
    /// Authoritative — this type is only constructed on success.
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        true
    }

    /// Credential id used.
    #[must_use]
    pub fn credential_id(&self) -> &CredentialId {
        &self.credential_id
    }

    /// User account.
    #[must_use]
    pub fn user(&self) -> UserAccountId {
        self.user
    }

    /// Sign-count after the assessment.
    #[must_use]
    pub fn persisted_sign_count(&self) -> SignCount {
        self.persisted_sign_count
    }

    /// Whether this was an admin step-up ceremony.
    #[must_use]
    pub fn is_admin_step_up(&self) -> bool {
        self.admin_step_up
    }
}

/// Authentication response — what the browser sends to
/// `finish_authentication`.
#[derive(Debug, Clone)]
pub struct AuthenticationResponse {
    /// Echoed challenge id.
    pub challenge_id: ChallengeId,
    /// Credential id selected by the user.
    pub credential_id: CredentialId,
    /// Authenticator-data flags.
    pub flags: AuthenticatorFlags,
    /// Sign-count reported by the authenticator.
    pub sign_count: SignCount,
    /// Origin reported by the client.
    pub origin: Origin,
    /// `clientDataJSON.type` echo (`webauthn.get` for authentication).
    pub client_data_type: String,
}

impl AuthenticationResponse {
    /// Test-only constructor.
    #[doc(hidden)]
    #[must_use]
    pub fn synthetic_for_test(
        challenge_id: ChallengeId,
        credential_id: CredentialId,
        flags: AuthenticatorFlags,
        sign_count: SignCount,
        origin: Origin,
    ) -> Self {
        Self {
            challenge_id,
            credential_id,
            flags,
            sign_count,
            origin,
            client_data_type: "webauthn.get".to_owned(),
        }
    }
}

/// Engine configuration (frozen after `build`).
pub struct EngineConfig {
    rp_id: RpId,
    rp_name: String,
    origins: OriginAllowlist,
    aaguids: AaguidPolicy,
    challenge_ttl: ChallengeTtl,
    metrics: Arc<dyn MetricsObserver>,
    require_uv_for_admin: bool,
    require_attestation: bool,
}

impl fmt::Debug for EngineConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineConfig")
            .field("rp_id", &self.rp_id)
            .field("rp_name", &self.rp_name)
            .field("origins_len", &self.origins.len())
            .field("aaguid_allow_count", &self.aaguids.allow_iter().count())
            .field("aaguid_deny_count", &self.aaguids.deny_iter().count())
            .field("challenge_ttl", &self.challenge_ttl.as_duration())
            .field("require_uv_for_admin", &self.require_uv_for_admin)
            .field("require_attestation", &self.require_attestation)
            .finish_non_exhaustive()
    }
}

impl EngineConfig {
    /// Construct a fluent builder.
    pub fn builder(rp_id: RpId, rp_name: impl Into<String>) -> EngineConfigBuilder {
        EngineConfigBuilder {
            rp_id,
            rp_name: rp_name.into(),
            origins: None,
            aaguids: None,
            challenge_ttl: ChallengeTtl::default(),
            metrics: None,
            require_uv_for_admin: true,
            require_attestation: true,
        }
    }

    /// RP-ID.
    #[must_use]
    pub fn rp_id(&self) -> &RpId {
        &self.rp_id
    }

    /// RP display name.
    #[must_use]
    pub fn rp_name(&self) -> &str {
        &self.rp_name
    }

    /// Origin allowlist.
    #[must_use]
    pub fn origins(&self) -> &OriginAllowlist {
        &self.origins
    }

    /// AAGUID policy.
    #[must_use]
    pub fn aaguids(&self) -> &AaguidPolicy {
        &self.aaguids
    }

    /// Challenge TTL.
    #[must_use]
    pub fn challenge_ttl(&self) -> ChallengeTtl {
        self.challenge_ttl
    }

    /// Metrics observer.
    #[must_use]
    pub fn metrics(&self) -> &Arc<dyn MetricsObserver> {
        &self.metrics
    }
}

/// Fluent builder for [`EngineConfig`].
#[derive(Debug)]
pub struct EngineConfigBuilder {
    rp_id: RpId,
    rp_name: String,
    origins: Option<OriginAllowlist>,
    aaguids: Option<AaguidPolicy>,
    challenge_ttl: ChallengeTtl,
    metrics: Option<Arc<dyn MetricsObserver>>,
    require_uv_for_admin: bool,
    require_attestation: bool,
}

impl EngineConfigBuilder {
    /// Configure the origin allowlist.
    #[must_use]
    pub fn origins(mut self, origins: OriginAllowlist) -> Self {
        self.origins = Some(origins);
        self
    }

    /// Configure the AAGUID policy.
    #[must_use]
    pub fn aaguids(mut self, aaguids: AaguidPolicy) -> Self {
        self.aaguids = Some(aaguids);
        self
    }

    /// Override challenge TTL.
    #[must_use]
    pub fn challenge_ttl(mut self, ttl: ChallengeTtl) -> Self {
        self.challenge_ttl = ttl;
        self
    }

    /// Inject a metrics observer.
    #[must_use]
    pub fn metrics(mut self, metrics: Arc<dyn MetricsObserver>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Override the admin-UV requirement (default `true`; only
    /// disable in property-test scaffolding).
    #[doc(hidden)]
    #[must_use]
    pub fn require_uv_for_admin(mut self, value: bool) -> Self {
        self.require_uv_for_admin = value;
        self
    }

    /// Override the attestation requirement (default `true`).
    #[doc(hidden)]
    #[must_use]
    pub fn require_attestation(mut self, value: bool) -> Self {
        self.require_attestation = value;
        self
    }

    /// Finalize. Validates that the origin allowlist (if set) is
    /// consistent with the canonical RP-ID.
    pub fn build(self) -> Result<EngineConfig, WebAuthnError> {
        let origins = self.origins.unwrap_or_default();
        if !origins.is_empty() {
            origins.require_consistency_with(&self.rp_id)?;
        }
        let aaguids = self.aaguids.unwrap_or_else(AaguidPolicy::empty);
        let metrics: Arc<dyn MetricsObserver> = self
            .metrics
            .unwrap_or_else(|| Arc::new(NoopMetrics));
        Ok(EngineConfig {
            rp_id: self.rp_id,
            rp_name: self.rp_name,
            origins,
            aaguids,
            challenge_ttl: self.challenge_ttl,
            metrics,
            require_uv_for_admin: self.require_uv_for_admin,
            require_attestation: self.require_attestation,
        })
    }
}

/// Canonical engine surface.
pub trait WebAuthnEngine: Send + Sync + std::fmt::Debug {
    /// Start a registration ceremony.
    fn start_registration(
        &self,
        user: UserAccountId,
        attachment: AuthenticatorAttachment,
    ) -> Result<RegistrationChallenge, WebAuthnError>;

    /// Finish a registration ceremony.
    fn finish_registration(
        &self,
        challenge_id: &ChallengeId,
        response: RegistrationResponse,
    ) -> Result<CredentialId, WebAuthnError>;

    /// Start an authentication ceremony.
    fn start_authentication(
        &self,
        user: UserAccountId,
        ceremony: Ceremony,
    ) -> Result<AuthenticationChallenge, WebAuthnError>;

    /// Finish an authentication ceremony.
    fn finish_authentication(
        &self,
        challenge_id: &ChallengeId,
        response: AuthenticationResponse,
    ) -> Result<AuthenticationOutcome, WebAuthnError>;
}

/// In-memory engine — algorithmic invariants only; no signature
/// verification. Production parity is delivered by the
/// `webauthn-rs` adapter shim wired in a downstream WI.
pub struct InMemoryEngine<C: EngineClock> {
    config: EngineConfig,
    clock: C,
    challenges: Arc<dyn ChallengeStore>,
    credentials: Arc<dyn CredentialStore>,
    recovery: Arc<dyn RecoveryOtpStore>,
}

impl<C: EngineClock> fmt::Debug for InMemoryEngine<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryEngine")
            .field("config", &self.config)
            .field("clock", &self.clock)
            .finish_non_exhaustive()
    }
}

impl<C: EngineClock> InMemoryEngine<C> {
    /// Construct with the canonical default in-memory stores.
    #[must_use]
    pub fn new(config: EngineConfig, clock: C) -> Self {
        Self {
            config,
            clock,
            challenges: Arc::new(InMemoryChallengeStore::new()),
            credentials: Arc::new(InMemoryCredentialStore::new()),
            recovery: Arc::new(InMemoryRecoveryOtpStore::new(RecoveryRateLimit::canonical())),
        }
    }

    /// Construct with explicit store handles (useful for sharing the
    /// in-memory credential store across an engine + an
    /// integration-test rig).
    #[must_use]
    pub fn with_stores(
        config: EngineConfig,
        clock: C,
        challenges: Arc<dyn ChallengeStore>,
        credentials: Arc<dyn CredentialStore>,
        recovery: Arc<dyn RecoveryOtpStore>,
    ) -> Self {
        Self {
            config,
            clock,
            challenges,
            credentials,
            recovery,
        }
    }

    /// Borrow the credential store (for tests + recovery flow).
    #[must_use]
    pub fn credential_store(&self) -> &Arc<dyn CredentialStore> {
        &self.credentials
    }

    /// Borrow the recovery store.
    #[must_use]
    pub fn recovery_store(&self) -> &Arc<dyn RecoveryOtpStore> {
        &self.recovery
    }

    /// Borrow the engine config.
    #[must_use]
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Borrow the clock (test helper for FixedClock advance).
    #[must_use]
    pub fn clock(&self) -> &C {
        &self.clock
    }

    fn ttl_ms(&self) -> u64 {
        u64::try_from(self.config.challenge_ttl.as_duration().as_millis()).unwrap_or(u64::MAX)
    }

    fn validate_origin_and_rp(
        &self,
        origin: &Origin,
        rp_id_hash_check: Option<&[u8; 32]>,
    ) -> Result<(), WebAuthnError> {
        if !self.config.origins.contains(origin) {
            self.config
                .metrics
                .record_authentication(CeremonyResult::OriginMismatch);
            return Err(WebAuthnError::OriginMismatch);
        }
        if let Some(hash) = rp_id_hash_check {
            if hash != &self.config.rp_id.hash() {
                self.config
                    .metrics
                    .record_authentication(CeremonyResult::RpIdMismatch);
                return Err(WebAuthnError::RpIdMismatch);
            }
        }
        Ok(())
    }
}

impl<C: EngineClock> WebAuthnEngine for InMemoryEngine<C> {
    fn start_registration(
        &self,
        user: UserAccountId,
        attachment: AuthenticatorAttachment,
    ) -> Result<RegistrationChallenge, WebAuthnError> {
        let id = ChallengeId::generate()?;
        let bytes = ChallengeBytes::generate()?;
        let now = self.clock.now_ms();
        let expires = now.saturating_add(self.ttl_ms());

        self.challenges.put(StoredChallenge {
            id: id.clone(),
            bytes_hex: hex::encode(bytes.as_bytes()),
            user,
            purpose: ChallengePurpose::Registration,
            registration_attachment: Some(attachment),
            require_user_verification: true,
            admin_step_up: false,
            allowed_credentials: Vec::new(),
            expected_aaguid: None,
            expected_origin_in_allowlist: None,
            expires_at_ms: expires,
        })?;

        Ok(RegistrationChallenge::new(
            id,
            bytes,
            user,
            self.config.rp_id.clone(),
            self.config.rp_name.clone(),
            attachment,
            expires,
        ))
    }

    fn finish_registration(
        &self,
        challenge_id: &ChallengeId,
        response: RegistrationResponse,
    ) -> Result<CredentialId, WebAuthnError> {
        let now = self.clock.now_ms();
        let stored = self.challenges.take(challenge_id, now)?;
        if stored.purpose != ChallengePurpose::Registration {
            self.config
                .metrics
                .record_registration(CeremonyResult::Other);
            return Err(WebAuthnError::InvalidChallenge);
        }
        if &response.challenge_id != challenge_id {
            self.config
                .metrics
                .record_registration(CeremonyResult::ChallengeInvalid);
            return Err(WebAuthnError::InvalidChallenge);
        }
        if response.client_data_type != "webauthn.create" {
            self.config
                .metrics
                .record_registration(CeremonyResult::Other);
            return Err(WebAuthnError::Malformed("clientDataJSON.type"));
        }

        // Origin allowlist exact match.
        if !self.config.origins.contains(&response.origin) {
            self.config
                .metrics
                .record_registration(CeremonyResult::OriginMismatch);
            return Err(WebAuthnError::OriginMismatch);
        }

        // RP-ID consistency: the response origin host must be a
        // sub-label of (or equal to) the canonical RP-ID.
        if !response.origin.is_subdomain_of(&self.config.rp_id) {
            self.config
                .metrics
                .record_registration(CeremonyResult::RpIdMismatch);
            return Err(WebAuthnError::RpIdMismatch);
        }

        // Flag enforcement.
        if !response.flags.user_presence() {
            self.config
                .metrics
                .record_registration(CeremonyResult::UpRequired);
            return Err(WebAuthnError::UserPresenceMissing);
        }
        if self.config.require_uv_for_admin && !response.flags.user_verification() {
            self.config
                .metrics
                .record_registration(CeremonyResult::UvRequired);
            return Err(WebAuthnError::UserVerificationMissing);
        }

        // Attestation requirement.
        if self.config.require_attestation && !response.attestation_present {
            self.config
                .metrics
                .record_registration(CeremonyResult::AttestationFailed);
            return Err(WebAuthnError::AttestationInvalid);
        }

        // COSE algorithm allowlist (rejects `alg: 0` "none").
        let cose_alg = parse_cose_algorithm(response.cose_algorithm_value).inspect_err(|_| {
            self.config
                .metrics
                .record_registration(CeremonyResult::Other);
        })?;

        // AAGUID policy.
        self.config.aaguids.evaluate(response.aaguid).inspect_err(|e| match e {
            WebAuthnError::AaguidDenied => self
                .config
                .metrics
                .record_registration(CeremonyResult::AaguidDenied),
            WebAuthnError::AaguidNotAllowed => self
                .config
                .metrics
                .record_registration(CeremonyResult::AaguidNotAllowed),
            _ => self
                .config
                .metrics
                .record_registration(CeremonyResult::Other),
        })?;

        let credential = Credential::from_registration(
            stored.user,
            response.credential_id.clone(),
            response.public_key_bytes,
            cose_alg,
            response.flags,
            response.sign_count,
            response.aaguid,
            now,
        );
        self.credentials.put(credential)?;

        self.config.metrics.record_registration(CeremonyResult::Ok);
        self.config.metrics.record_aaguid(response.aaguid);

        Ok(response.credential_id)
    }

    fn start_authentication(
        &self,
        user: UserAccountId,
        ceremony: Ceremony,
    ) -> Result<AuthenticationChallenge, WebAuthnError> {
        let id = ChallengeId::generate()?;
        let bytes = ChallengeBytes::generate()?;
        let now = self.clock.now_ms();
        let expires = now.saturating_add(self.ttl_ms());

        let credentials = self.credentials.list_for_user(user);
        if credentials.is_empty() {
            return Err(WebAuthnError::CredentialNotFound);
        }
        let allowed: Vec<CredentialId> = credentials
            .iter()
            .map(|c| c.credential_id.clone())
            .collect();

        let admin_step_up = matches!(ceremony, Ceremony::AdminStepUp);
        let require_uv = admin_step_up && self.config.require_uv_for_admin;

        self.challenges.put(StoredChallenge {
            id: id.clone(),
            bytes_hex: hex::encode(bytes.as_bytes()),
            user,
            purpose: ChallengePurpose::Authentication,
            registration_attachment: None,
            require_user_verification: require_uv,
            admin_step_up,
            allowed_credentials: allowed.clone(),
            expected_aaguid: None,
            expected_origin_in_allowlist: None,
            expires_at_ms: expires,
        })?;

        Ok(AuthenticationChallenge::new(
            id,
            bytes,
            user,
            self.config.rp_id.clone(),
            allowed,
            require_uv,
            expires,
            admin_step_up,
        ))
    }

    fn finish_authentication(
        &self,
        challenge_id: &ChallengeId,
        response: AuthenticationResponse,
    ) -> Result<AuthenticationOutcome, WebAuthnError> {
        let now = self.clock.now_ms();
        let stored = self.challenges.take(challenge_id, now)?;
        if stored.purpose != ChallengePurpose::Authentication {
            return Err(WebAuthnError::InvalidChallenge);
        }
        if &response.challenge_id != challenge_id {
            self.config
                .metrics
                .record_authentication(CeremonyResult::ChallengeInvalid);
            return Err(WebAuthnError::InvalidChallenge);
        }
        if response.client_data_type != "webauthn.get" {
            return Err(WebAuthnError::Malformed("clientDataJSON.type"));
        }

        self.validate_origin_and_rp(&response.origin, None)?;
        if !response.origin.is_subdomain_of(&self.config.rp_id) {
            self.config
                .metrics
                .record_authentication(CeremonyResult::RpIdMismatch);
            return Err(WebAuthnError::RpIdMismatch);
        }

        // allow_credentials enforcement.
        if !stored
            .allowed_credentials
            .iter()
            .any(|c| c == &response.credential_id)
        {
            self.config
                .metrics
                .record_authentication(CeremonyResult::Other);
            return Err(WebAuthnError::CredentialNotFound);
        }

        // Flag enforcement.
        if !response.flags.user_presence() {
            self.config
                .metrics
                .record_authentication(CeremonyResult::UpRequired);
            return Err(WebAuthnError::UserPresenceMissing);
        }
        if stored.require_user_verification && !response.flags.user_verification() {
            self.config
                .metrics
                .record_authentication(CeremonyResult::UvRequired);
            return Err(WebAuthnError::UserVerificationMissing);
        }

        // Sign-count assessment.
        let credential = self
            .credentials
            .get_by_credential_id(&response.credential_id)?;
        let assessment = assess_sign_count(credential.sign_count, response.sign_count);
        match assessment.severity {
            SignCountSeverity::Sev2InvestigationRequired => {
                self.config.metrics.record_sign_count_regression();
                self.config
                    .metrics
                    .record_authentication(CeremonyResult::SignCountRegression);
                return Err(WebAuthnError::SignCountRegression {
                    got: response.sign_count.value(),
                    stored: credential.sign_count.value(),
                });
            }
            SignCountSeverity::Monotonic => {
                self.credentials.update_after_authentication(
                    &response.credential_id,
                    assessment.persisted.value(),
                    now,
                )?;
            }
            SignCountSeverity::PasskeyExempt => {
                self.credentials.update_after_authentication(
                    &response.credential_id,
                    0,
                    now,
                )?;
            }
        }

        self.config
            .metrics
            .record_authentication(CeremonyResult::Ok);
        if stored.admin_step_up {
            self.config.metrics.record_admin_step_up("step_up_ok");
        }

        Ok(AuthenticationOutcome {
            credential_id: response.credential_id,
            user: stored.user,
            persisted_sign_count: assessment.persisted,
            admin_step_up: stored.admin_step_up,
        })
    }
}

/// Production-engine sentinel — every method returns
/// [`WebAuthnError::EngineNotConfigured`] until the `feature =
/// "host-server"` shim lands.
#[derive(Debug, Default)]
pub struct ProductionEngineNotConfigured;

impl ProductionEngineNotConfigured {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl WebAuthnEngine for ProductionEngineNotConfigured {
    fn start_registration(
        &self,
        _user: UserAccountId,
        _attachment: AuthenticatorAttachment,
    ) -> Result<RegistrationChallenge, WebAuthnError> {
        Err(WebAuthnError::EngineNotConfigured)
    }

    fn finish_registration(
        &self,
        _challenge_id: &ChallengeId,
        _response: RegistrationResponse,
    ) -> Result<CredentialId, WebAuthnError> {
        Err(WebAuthnError::EngineNotConfigured)
    }

    fn start_authentication(
        &self,
        _user: UserAccountId,
        _ceremony: Ceremony,
    ) -> Result<AuthenticationChallenge, WebAuthnError> {
        Err(WebAuthnError::EngineNotConfigured)
    }

    fn finish_authentication(
        &self,
        _challenge_id: &ChallengeId,
        _response: AuthenticationResponse,
    ) -> Result<AuthenticationOutcome, WebAuthnError> {
        Err(WebAuthnError::EngineNotConfigured)
    }
}

/// Convenience: stash the canonical TTL for `start_authentication`
/// callers that wrap the engine in a Tower layer.
#[must_use]
pub const fn admin_step_up_default_ttl() -> Duration {
    Duration::from_secs(300)
}
