//! Ack-vector taxonomy.

/// Canonical 3-vector taxonomy for how the on-call engineer
/// acknowledged the synthetic page per WI-S20-006 §2.1. The
/// `#[non_exhaustive]` marker reserves additive growth for follow-on
/// WIs (e.g. dedicated chat-bot vector, voice-call vector).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AckVector {
    /// PagerDuty mobile push notification (preferred — lowest latency).
    MobilePush,
    /// SMS fallback (secondary; ~10s SMS gateway hop).
    Sms,
    /// Email (tertiary; usually delayed; flagged in dashboard if used
    /// for a SEV-2 because it implies the mobile/SMS path failed).
    Email,
}

impl AckVector {
    /// Canonical wire slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MobilePush => "mobile_push",
            Self::Sms => "sms",
            Self::Email => "email",
        }
    }

    /// Whether this vector is considered the preferred (lowest-latency)
    /// channel for SEV-2 ack. Used by the dashboard to highlight rows
    /// where the on-call had to fall back.
    #[must_use]
    pub const fn is_preferred(self) -> bool {
        matches!(self, Self::MobilePush)
    }
}

impl core::fmt::Display for AckVector {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-element list of [`AckVector`] for surface-stability
/// regression tests.
#[must_use]
pub const fn canonical_ack_vectors() -> &'static [AckVector; 3] {
    &[AckVector::MobilePush, AckVector::Sms, AckVector::Email]
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn ack_vector_labels_canonical() {
        assert_eq!(AckVector::MobilePush.as_str(), "mobile_push");
        assert_eq!(AckVector::Sms.as_str(), "sms");
        assert_eq!(AckVector::Email.as_str(), "email");
    }

    #[test]
    fn only_mobile_push_is_preferred() {
        assert!(AckVector::MobilePush.is_preferred());
        assert!(!AckVector::Sms.is_preferred());
        assert!(!AckVector::Email.is_preferred());
    }

    #[test]
    fn canonical_ack_vectors_surface_stable() {
        let vs = canonical_ack_vectors();
        assert_eq!(vs.len(), 3);
        assert_eq!(vs[0], AckVector::MobilePush);
    }
}
