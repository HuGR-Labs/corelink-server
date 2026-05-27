//! [`AuthenticatorFlags`] — the W3C `authData.flags` byte view.
//!
//! Only the four flag bits we care about (UP, UV, BE, BS) are
//! surfaced. The byte layout matches the W3C §6.1 wire format so a
//! production engine can hydrate this newtype directly from the
//! authenticator-data prefix without a re-parse.

use serde::{Deserialize, Serialize};

/// User-Presence flag (bit 0).
const FLAG_UP: u8 = 0b0000_0001;
/// User-Verified flag (bit 2).
const FLAG_UV: u8 = 0b0000_0100;
/// Backup-Eligible flag (bit 3).
const FLAG_BE: u8 = 0b0000_1000;
/// Backup-State flag (bit 4).
const FLAG_BS: u8 = 0b0001_0000;

/// Parsed flag byte from the authenticator-data prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AuthenticatorFlags(u8);

impl AuthenticatorFlags {
    /// Construct from a raw flag byte (W3C `authData[32]`).
    #[must_use]
    pub const fn from_byte(byte: u8) -> Self {
        Self(byte)
    }

    /// Convenience: only UP set.
    #[must_use]
    pub const fn up_only() -> Self {
        Self(FLAG_UP)
    }

    /// Convenience: UP + UV (canonical admin-step-up shape).
    #[must_use]
    pub const fn up_uv() -> Self {
        Self(FLAG_UP | FLAG_UV)
    }

    /// Convenience: UP + UV + BE (passkey-eligible).
    #[must_use]
    pub const fn up_uv_be() -> Self {
        Self(FLAG_UP | FLAG_UV | FLAG_BE)
    }

    /// Convenience: UP + UV + BE + BS (passkey already synced).
    #[must_use]
    pub const fn up_uv_be_bs() -> Self {
        Self(FLAG_UP | FLAG_UV | FLAG_BE | FLAG_BS)
    }

    /// Raw byte view (for serialization parity).
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self.0
    }

    /// User-Presence bit set.
    #[must_use]
    pub const fn user_presence(self) -> bool {
        self.0 & FLAG_UP != 0
    }

    /// User-Verified bit set.
    #[must_use]
    pub const fn user_verification(self) -> bool {
        self.0 & FLAG_UV != 0
    }

    /// Backup-Eligible bit set.
    #[must_use]
    pub const fn backup_eligible(self) -> bool {
        self.0 & FLAG_BE != 0
    }

    /// Backup-State bit set.
    #[must_use]
    pub const fn backup_state(self) -> bool {
        self.0 & FLAG_BS != 0
    }
}
