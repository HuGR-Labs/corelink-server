//! Canonical [`TenantId`] newtype — wave-33 Stage 0 cross-cutting type.
//!
//! Wraps a `Uuid` (UUIDv7 in production) and is the canonical tenant
//! identifier across every CoreLink context crate. Mirrors the prior
//! `corelink_meta::types::TenantId` shape so migrations from existing
//! scattered copies (audit / meta / pat / survey, etc.) drop in with
//! `pub use corelink_core::TenantId` once Stage 1 streams land.
//!
//! Behaviour: identical to the canonical `corelink-meta` version. The
//! text form is canonical UUIDv7 hyphenated lowercase (per
//! `specs/03_architecture/data_model.md §2.1`).

use core::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Newtype over [`Uuid`] for the canonical CoreLink tenant identifier.
///
/// Stored in D1 as canonical UUIDv7 text form
/// (`01938af0-abcd-7123-8456-..`). The newtype enforces "the canonical
/// text form is the only thing that ever leaves Rust into a SQL
/// parameter" and gives a single place to plug a future text-form
/// validator without churning every call site.
///
/// Construct via [`Self::from_uuid`] or `From<Uuid>`; render canonical
/// text via [`Self::to_canonical_text`] or [`fmt::Display`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct TenantId(Uuid);

impl TenantId {
    /// Wrap a [`Uuid`] into the canonical newtype.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Borrow the inner [`Uuid`].
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Render the UUIDv7 in canonical hyphenated lowercase text form.
    ///
    /// This is the exact byte sequence written to D1 `tenant_id`
    /// columns and to `audit_outbox.tenant_id`.
    #[must_use]
    pub fn to_canonical_text(&self) -> String {
        format!("{}", self.0)
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<Uuid> for TenantId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn tenant_id_round_trips_through_uuid() {
        let raw = Uuid::nil();
        let t = TenantId::from_uuid(raw);
        assert_eq!(*t.as_uuid(), raw);
        assert_eq!(t.to_canonical_text(), "00000000-0000-0000-0000-000000000000");
        assert_eq!(format!("{t}"), t.to_canonical_text());
    }

    #[test]
    fn tenant_id_from_uuid_via_from_trait() {
        let raw = Uuid::nil();
        let t: TenantId = raw.into();
        assert_eq!(*t.as_uuid(), raw);
    }
}
