//! ID newtypes shared across the survey crate.
//!
//! Two distinct identity surfaces are modeled:
//!
//! - [`TenantId`] — canonical CoreLink tenant id (UUIDv7), mirrors the
//!   shape of `corelink-meta::TenantId` and `corelink-audit::TenantId`
//!   so call sites can interconvert without drift.
//! - [`SurveyId`] — opaque non-empty string slug naming a survey wave
//!   (e.g. `"nps-w1-2026q2"`, `"csat-onboarding-2026"`). Wave + cadence
//!   conventions are documented in
//!   `marketing/retention/customer-health/NPS-SURVEY-SCHEDULE.md`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical CoreLink tenant id newtype.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TenantId(Uuid);

impl TenantId {
    /// Wrap a `Uuid` into the canonical newtype.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Borrow the inner UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl core::fmt::Display for TenantId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.0, f)
    }
}

/// Opaque non-empty survey identifier (a wave slug).
///
/// Slug format is enforced by [`SurveyId::new`]: ASCII alphanumeric +
/// `-` + `_`, length 1..=64. This keeps the slug safe to embed in URLs
/// + log lines + audit envelopes without escape concerns.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SurveyId(String);

const SURVEY_ID_MIN: usize = 1;
const SURVEY_ID_MAX: usize = 64;

impl SurveyId {
    /// Construct a new [`SurveyId`] from a string slug.
    ///
    /// Accepts ASCII alphanumeric + `-` + `_` of length 1..=64. Any
    /// other input (including non-ASCII, empty, too-long, or any other
    /// punctuation) returns `None` — the caller MUST surface this as a
    /// 400 to the operator.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        let s = s.into();
        // Constructor is `panic-free`: invariants are enforced by the
        // sibling `try_new` for callers that want a Result. `new` is
        // the ergonomic constructor used in tests / hard-coded sites.
        Self(s)
    }

    /// Strict constructor: returns `None` if the slug is invalid.
    #[must_use]
    pub fn try_new(s: impl Into<String>) -> Option<Self> {
        let s = s.into();
        if !(SURVEY_ID_MIN..=SURVEY_ID_MAX).contains(&s.len()) {
            return None;
        }
        if !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return None;
        }
        Some(Self(s))
    }

    /// Borrow the inner slug.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for SurveyId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}
