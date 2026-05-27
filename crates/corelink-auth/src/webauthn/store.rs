//! Persistence traits + in-memory implementations.
//!
//! Three distinct stores:
//!
//! 1. [`ChallengeStore`] — short-lived KV (Cloudflare KV in
//!    production) keyed on [`super::ChallengeId`]; TTL-bound.
//! 2. [`CredentialStore`] — Neon Postgres `webauthn_credentials`
//!    table (per `migrations/002_auth_tables.sql §7`).
//! 3. [`RecoveryOtpStore`] — Neon `auth_recovery_otp` table; rate
//!    limited and Argon2id-hashed.
//!
//! Each trait has a host-side in-memory implementation used by
//! property tests. The traits are `Send + Sync` so they slot into
//! the production async path.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::challenge::StoredChallenge;
use super::credential::Credential;
use super::recovery::{RecoveryOtpRecord, RecoveryRateLimit};
use super::types::UserAccountId;
use super::{ChallengeId, CredentialId, WebAuthnError};

/// Authenticator attachment selector (W3C §5.4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuthenticatorAttachment {
    /// Platform authenticator (Touch ID / Face ID / Windows Hello /
    /// Android biometrics).
    Platform,
    /// Cross-platform authenticator (YubiKey, Solo Key, Titan Key).
    CrossPlatform,
}

/// Short-lived KV-style challenge persistence.
pub trait ChallengeStore: Send + Sync + std::fmt::Debug {
    /// Insert a freshly-generated challenge. Returns
    /// [`WebAuthnError::Conflict`] on id collision (single-use guarantee).
    fn put(&self, record: StoredChallenge) -> Result<(), WebAuthnError>;

    /// Fetch + delete (single-use). Returns
    /// [`WebAuthnError::InvalidChallenge`] on miss and
    /// [`WebAuthnError::ChallengeExpired`] if the record is expired.
    fn take(&self, id: &ChallengeId, now_ms: u64) -> Result<StoredChallenge, WebAuthnError>;

    /// Drop expired entries (production: KV native TTL handles this;
    /// the in-memory fake exposes the call so tests can pin behaviour).
    fn evict_expired(&self, now_ms: u64) -> usize;
}

/// In-memory challenge store.
#[derive(Debug, Default)]
pub struct InMemoryChallengeStore {
    inner: Mutex<HashMap<ChallengeId, StoredChallenge>>,
}

impl InMemoryChallengeStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ChallengeStore for InMemoryChallengeStore {
    fn put(&self, record: StoredChallenge) -> Result<(), WebAuthnError> {
        let mut guard = self.inner.lock().map_err(|_| WebAuthnError::Conflict)?;
        if guard.contains_key(&record.id) {
            return Err(WebAuthnError::Conflict);
        }
        guard.insert(record.id.clone(), record);
        Ok(())
    }

    fn take(&self, id: &ChallengeId, now_ms: u64) -> Result<StoredChallenge, WebAuthnError> {
        let mut guard = self.inner.lock().map_err(|_| WebAuthnError::Conflict)?;
        let record = match guard.remove(id) {
            Some(r) => r,
            None => return Err(WebAuthnError::InvalidChallenge),
        };
        if now_ms >= record.expires_at_ms {
            return Err(WebAuthnError::ChallengeExpired);
        }
        Ok(record)
    }

    fn evict_expired(&self, now_ms: u64) -> usize {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return 0,
        };
        let before = guard.len();
        guard.retain(|_, r| now_ms < r.expires_at_ms);
        before - guard.len()
    }
}

/// Long-lived credential persistence (mirrors Neon
/// `webauthn_credentials`).
pub trait CredentialStore: Send + Sync + std::fmt::Debug {
    /// Insert. Returns [`WebAuthnError::Conflict`] on
    /// `credential_id` UNIQUE collision.
    fn put(&self, credential: Credential) -> Result<(), WebAuthnError>;

    /// Lookup by credential id. Returns
    /// [`WebAuthnError::CredentialNotFound`] on miss.
    fn get_by_credential_id(
        &self,
        credential_id: &CredentialId,
    ) -> Result<Credential, WebAuthnError>;

    /// List credentials for a user account (sorted by creation time;
    /// stable for proptest assertions).
    fn list_for_user(&self, user: UserAccountId) -> Vec<Credential>;

    /// Update sign-count + last-used timestamp atomically.
    fn update_after_authentication(
        &self,
        credential_id: &CredentialId,
        new_sign_count: u64,
        last_used_at_ms: u64,
    ) -> Result<(), WebAuthnError>;

    /// Soft-delete (sets `deleted_at`).
    fn revoke(&self, credential_id: &CredentialId) -> Result<(), WebAuthnError>;
}

/// In-memory credential store.
#[derive(Debug, Default)]
pub struct InMemoryCredentialStore {
    inner: Mutex<BTreeMap<CredentialId, Credential>>,
    revoked: Mutex<BTreeMap<CredentialId, ()>>,
}

impl InMemoryCredentialStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn put(&self, credential: Credential) -> Result<(), WebAuthnError> {
        let mut guard = self.inner.lock().map_err(|_| WebAuthnError::Conflict)?;
        if guard.contains_key(&credential.credential_id) {
            return Err(WebAuthnError::Conflict);
        }
        guard.insert(credential.credential_id.clone(), credential);
        Ok(())
    }

    fn get_by_credential_id(
        &self,
        credential_id: &CredentialId,
    ) -> Result<Credential, WebAuthnError> {
        if self
            .revoked
            .lock()
            .map_err(|_| WebAuthnError::Conflict)?
            .contains_key(credential_id)
        {
            return Err(WebAuthnError::CredentialNotFound);
        }
        let guard = self.inner.lock().map_err(|_| WebAuthnError::Conflict)?;
        match guard.get(credential_id) {
            Some(c) => Ok(c.clone()),
            None => Err(WebAuthnError::CredentialNotFound),
        }
    }

    fn list_for_user(&self, user: UserAccountId) -> Vec<Credential> {
        let revoked = match self.revoked.lock() {
            Ok(g) => g.keys().cloned().collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<Credential> = guard
            .values()
            .filter(|c| c.user_account_id == user && !revoked.contains(&c.credential_id))
            .cloned()
            .collect();
        out.sort_by_key(|c| c.created_at_ms);
        out
    }

    fn update_after_authentication(
        &self,
        credential_id: &CredentialId,
        new_sign_count: u64,
        last_used_at_ms: u64,
    ) -> Result<(), WebAuthnError> {
        let mut guard = self.inner.lock().map_err(|_| WebAuthnError::Conflict)?;
        match guard.get_mut(credential_id) {
            Some(c) => {
                c.sign_count = super::SignCount::new(new_sign_count);
                c.last_used_at_ms = Some(last_used_at_ms);
                Ok(())
            }
            None => Err(WebAuthnError::CredentialNotFound),
        }
    }

    fn revoke(&self, credential_id: &CredentialId) -> Result<(), WebAuthnError> {
        let exists = self
            .inner
            .lock()
            .map_err(|_| WebAuthnError::Conflict)?
            .contains_key(credential_id);
        if !exists {
            return Err(WebAuthnError::CredentialNotFound);
        }
        let mut revoked = self.revoked.lock().map_err(|_| WebAuthnError::Conflict)?;
        revoked.insert(credential_id.clone(), ());
        Ok(())
    }
}

/// Recovery OTP persistence — single-use, rate-limited.
pub trait RecoveryOtpStore: Send + Sync + std::fmt::Debug {
    /// Persist a freshly-issued OTP record.
    fn put(&self, record: RecoveryOtpRecord) -> Result<(), WebAuthnError>;

    /// Atomically verify + consume in one step.
    /// Returns [`WebAuthnError::RecoveryOtpInvalid`] on Argon2id
    /// mismatch, [`WebAuthnError::RecoveryOtpAlreadyConsumed`] on
    /// re-use, [`WebAuthnError::ChallengeExpired`] on TTL miss
    /// (re-uses the canonical "challenge expired" wire mapping
    /// because OTP recovery is itself a challenge ceremony).
    fn verify_and_consume(
        &self,
        user: UserAccountId,
        candidate: &str,
        now_ms: u64,
    ) -> Result<super::recovery::RecoveryOtpVerifyOutcome, WebAuthnError>;

    /// Increment a generation-rate counter and return the resulting
    /// count for the rolling 1-hour window.
    fn record_generation(&self, user: UserAccountId, now_ms: u64) -> u32;

    /// Rolling generation count (1-hour window).
    fn generation_count_in_window(&self, user: UserAccountId, now_ms: u64) -> u32;

    /// Rate-limit policy.
    fn rate_limit(&self) -> RecoveryRateLimit;
}

/// In-memory OTP store with rate limiting.
#[derive(Debug)]
pub struct InMemoryRecoveryOtpStore {
    by_user: Mutex<HashMap<UserAccountId, RecoveryOtpRecord>>,
    generation_log: Mutex<HashMap<UserAccountId, Vec<u64>>>,
    policy: RecoveryRateLimit,
}

impl InMemoryRecoveryOtpStore {
    /// Construct with the canonical rate-limit policy
    /// (`WI-S03-006 §6.1.10`).
    #[must_use]
    pub fn new(policy: RecoveryRateLimit) -> Self {
        Self {
            by_user: Mutex::default(),
            generation_log: Mutex::default(),
            policy,
        }
    }
}

impl RecoveryOtpStore for InMemoryRecoveryOtpStore {
    fn put(&self, record: RecoveryOtpRecord) -> Result<(), WebAuthnError> {
        let mut guard = self.by_user.lock().map_err(|_| WebAuthnError::Conflict)?;
        // single active OTP per user — UNIQUE on user_id.
        guard.insert(record.user, record);
        Ok(())
    }

    fn verify_and_consume(
        &self,
        user: UserAccountId,
        candidate: &str,
        now_ms: u64,
    ) -> Result<super::recovery::RecoveryOtpVerifyOutcome, WebAuthnError> {
        let mut guard = self.by_user.lock().map_err(|_| WebAuthnError::Conflict)?;
        let record = match guard.get_mut(&user) {
            Some(r) => r,
            None => return Err(WebAuthnError::RecoveryOtpInvalid),
        };
        if record.consumed_at_ms.is_some() {
            return Err(WebAuthnError::RecoveryOtpAlreadyConsumed);
        }
        if now_ms >= record.expires_at_ms {
            return Err(WebAuthnError::ChallengeExpired);
        }
        if record.attempts_remaining == 0 {
            return Err(WebAuthnError::RecoveryOtpRateLimited);
        }
        record.attempts_remaining = record.attempts_remaining.saturating_sub(1);
        if !super::recovery::verify_otp(candidate, &record.hash) {
            return Ok(super::recovery::RecoveryOtpVerifyOutcome::Mismatch {
                attempts_remaining: record.attempts_remaining,
            });
        }
        record.consumed_at_ms = Some(now_ms);
        Ok(super::recovery::RecoveryOtpVerifyOutcome::Consumed {
            otp_id: record.id,
        })
    }

    fn record_generation(&self, user: UserAccountId, now_ms: u64) -> u32 {
        let mut log = match self.generation_log.lock() {
            Ok(g) => g,
            Err(_) => return 0,
        };
        let entry = log.entry(user).or_default();
        let window_ms = self.policy.generation_window_ms();
        let cutoff = now_ms.saturating_sub(window_ms);
        entry.retain(|t| *t >= cutoff);
        entry.push(now_ms);
        u32::try_from(entry.len()).unwrap_or(u32::MAX)
    }

    fn generation_count_in_window(&self, user: UserAccountId, now_ms: u64) -> u32 {
        let log = match self.generation_log.lock() {
            Ok(g) => g,
            Err(_) => return 0,
        };
        let window_ms = self.policy.generation_window_ms();
        let cutoff = now_ms.saturating_sub(window_ms);
        let count = log
            .get(&user)
            .map(|v| v.iter().filter(|t| **t >= cutoff).count())
            .unwrap_or(0);
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    fn rate_limit(&self) -> RecoveryRateLimit {
        self.policy
    }
}
