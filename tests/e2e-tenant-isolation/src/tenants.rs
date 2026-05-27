//! Deterministic two-tenant fixture (TENANT_A / TENANT_B) used across
//! all 12 adversarial scenarios.
//!
//! Each tenant carries:
//!
//! - a stable UUIDv7 hex string (parsed lazily inside the constructor;
//!   the constants are deliberately fixed so failures in one scenario
//!   are reproducible without seed environment variables),
//! - the per-region [`corelink_tenant_path::TenantDerivationKey`] used
//!   to derive its R2 prefix (the same TDK is shared across tenants —
//!   this mirrors production where one region holds one TDK and
//!   derives every tenant prefix under it via HMAC-SHA256),
//! - the [`corelink_tenant_path::TenantPrefix`] (HMAC16 — 16 ASCII
//!   chars) used for CAS / D1 partitioning,
//! - the [`corelink_audit::TenantId`] mirror for emitter call sites.

use corelink_audit::TenantId as AuditTenantId;
use corelink_tenant_path::{derive_prefix, TenantDerivationKey, TenantPrefix};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Fixed UUIDv7 string for TENANT_A. Deterministic across test runs.
const TENANT_A_UUID: &str = "01938af0-aaaa-7111-8222-000000000001";

/// Fixed UUIDv7 string for TENANT_B. Deterministic across test runs.
const TENANT_B_UUID: &str = "01938af0-bbbb-7111-8222-000000000002";

/// Shared TDK bytes — both tenants live in the same region, so they
/// share the per-region derivation key. The HMAC ensures their
/// prefixes do not collide (96-bit output entropy; see
/// `corelink-tenant-path` docs).
const SHARED_TDK_BYTES: [u8; 32] = [
    0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
    0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0, 0xf0, 0x01,
];

/// Errors that can surface while building the deterministic tenant
/// fixture. These should never occur in practice (the constants are
/// chosen to parse cleanly), but we surface a `Result` from the
/// constructor so the test scaffolding stays panic-free per the
/// `#![deny(clippy::expect_used)]` lint.
#[derive(Debug, thiserror::Error)]
pub enum TenantCtxError {
    /// The hard-coded UUID string failed to parse.
    #[error("hard-coded tenant UUID parse error: {0}")]
    UuidParse(#[from] uuid::Error),
}

/// Per-tenant context: the canonical UUIDv7, derived prefix, and audit
/// mirror id.
#[derive(Debug)]
pub struct TenantCtx {
    tenant_id: Uuid,
    prefix: TenantPrefix,
}

impl TenantCtx {
    /// Build the TENANT_A fixture.
    ///
    /// # Errors
    ///
    /// Returns [`TenantCtxError::UuidParse`] only if the hard-coded
    /// UUID constant somehow becomes invalid (defence-in-depth — the
    /// constant is tested in `tenant_a_round_trip`).
    pub fn tenant_a() -> Result<Self, TenantCtxError> {
        Self::from_uuid_str(TENANT_A_UUID)
    }

    /// Build the TENANT_B fixture.
    ///
    /// # Errors
    ///
    /// Returns [`TenantCtxError::UuidParse`] under the same condition
    /// as [`Self::tenant_a`].
    pub fn tenant_b() -> Result<Self, TenantCtxError> {
        Self::from_uuid_str(TENANT_B_UUID)
    }

    fn from_uuid_str(s: &str) -> Result<Self, TenantCtxError> {
        let tenant_id = Uuid::parse_str(s)?;
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new(SHARED_TDK_BYTES));
        let prefix = derive_prefix(&tdk, tenant_id);
        Ok(Self { tenant_id, prefix })
    }

    /// Borrow the canonical UUIDv7.
    #[must_use]
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    /// Borrow the derived R2 prefix.
    #[must_use]
    pub fn prefix(&self) -> &TenantPrefix {
        &self.prefix
    }

    /// Mirror id for the `corelink-audit` envelope.
    #[must_use]
    pub fn audit_tenant_id(&self) -> AuditTenantId {
        AuditTenantId::from_uuid(self.tenant_id)
    }
}

/// Constant-time tenant id equality — used by fake stores when
/// authorising a JWT-claimed tenant against the resource tenant. A
/// real network stack would expose a timing oracle for naive `==`;
/// the harness uses `subtle::ConstantTimeEq` as defence-in-depth so
/// the assertion shape matches what production middleware does.
#[must_use]
pub fn ct_tenant_eq(a: &Uuid, b: &Uuid) -> bool {
    let aa = a.as_bytes();
    let bb = b.as_bytes();
    aa.ct_eq(bb).unwrap_u8() == 1
}
