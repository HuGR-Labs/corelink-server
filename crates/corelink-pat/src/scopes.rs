//! `PatScopes` u64 bitset — compact representation of the 13 canonical
//! `auth_model.md §3.1` scopes plus 51 reserved bits for future
//! expansion (cap = 64; ADR-0026 documents the migration path to u128
//! when the canonical scope catalog exceeds 64 entries).
//!
//! # Layout
//!
//! Bits 0..=11 cover the 13 canonical scopes (note: `cache-rw` is an
//! alias of `cache-r | cache-w` and shares no dedicated bit). Bits
//! 12..=63 are reserved; the `Self::from_u64` constructor masks
//! reserved bits to zero so a forward-compatible DB row whose bitset
//! includes future flags gracefully degrades on a current-generation
//! reader instead of returning a malformed scope set.
//!
//! # Why u64 (not `Vec<String>`)
//!
//! The middleware permission check is on the hot path of every
//! authenticated request. Bitwise `(scopes & required) == required`
//! is a single instruction with no allocation; a `Vec<String>` walk
//! would allocate per check. The wire representation can still be
//! human-readable (the [`PatScopes::names`] helper returns a
//! `Vec<&'static str>` for API responses; the canonical wire form for
//! gRPC + dashboards is the colon-form set per `auth_model.md §3.1`).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Compact u64 bitset of PAT scopes. Construct via the canonical
/// constants ([`SCOPE_CACHE_R`], …) `|`'d together, or via
/// [`PatScopes::from_u64`] from a DB column.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PatScopes(u64);

/// `cache:r` — read CAS blobs + AC results.
pub const SCOPE_CACHE_R: u64 = 1 << 0;
/// `cache:w` — write CAS blobs + AC results.
pub const SCOPE_CACHE_W: u64 = 1 << 1;
/// `cache:find-missing` — execute `FindMissingBlobs` (no implicit download).
pub const SCOPE_CACHE_FIND: u64 = 1 << 2;
/// `cache:delete` — admin blob deletion (rare).
pub const SCOPE_CACHE_DELETE: u64 = 1 << 3;
/// `admin:tenant-read` — list tenant config.
pub const SCOPE_ADMIN_TENANT_R: u64 = 1 << 4;
/// `admin:tenant-write` — mutate tenant config.
pub const SCOPE_ADMIN_TENANT_W: u64 = 1 << 5;
/// `admin:tokens` — mint / revoke tokens.
pub const SCOPE_ADMIN_TOKENS: u64 = 1 << 6;
/// `admin:billing` — view / mutate billing surface.
pub const SCOPE_ADMIN_BILLING: u64 = 1 << 7;
/// `admin:audit` — read tenant audit logs.
pub const SCOPE_ADMIN_AUDIT: u64 = 1 << 8;
/// `admin:users` — manage tenant users.
pub const SCOPE_ADMIN_USERS: u64 = 1 << 9;
/// `execute:action` — Phase 2 executor reports action start.
pub const SCOPE_EXECUTE_ACTION: u64 = 1 << 10;
/// `report:result` — Phase 2 executor reports result.
pub const SCOPE_REPORT_RESULT: u64 = 1 << 11;

/// Convenience union: read + write CAS in a single mask.
pub const SCOPE_CACHE_RW: u64 = SCOPE_CACHE_R | SCOPE_CACHE_W;

/// Mask of all 12 currently-known canonical bits. Any bit outside this
/// mask is reserved for future expansion and silently dropped by
/// [`PatScopes::from_u64`].
pub const SCOPE_KNOWN_MASK: u64 = SCOPE_CACHE_R
    | SCOPE_CACHE_W
    | SCOPE_CACHE_FIND
    | SCOPE_CACHE_DELETE
    | SCOPE_ADMIN_TENANT_R
    | SCOPE_ADMIN_TENANT_W
    | SCOPE_ADMIN_TOKENS
    | SCOPE_ADMIN_BILLING
    | SCOPE_ADMIN_AUDIT
    | SCOPE_ADMIN_USERS
    | SCOPE_EXECUTE_ACTION
    | SCOPE_REPORT_RESULT;

impl PatScopes {
    /// Construct an empty scope set (no permissions).
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Construct from a raw u64 bitset; reserved bits are masked off
    /// so a forward-compatible DB row never round-trips an unknown
    /// scope. Use [`PatScopes::from_u64_strict`] when you want to
    /// reject unknown bits.
    #[must_use]
    pub const fn from_u64(raw: u64) -> Self {
        Self(raw & SCOPE_KNOWN_MASK)
    }

    /// Like [`PatScopes::from_u64`] but returns `None` if `raw`
    /// contains any reserved bit. Used in deploy-time invariants
    /// where stale rows are intolerable.
    #[must_use]
    pub const fn from_u64_strict(raw: u64) -> Option<Self> {
        if raw & !SCOPE_KNOWN_MASK == 0 {
            Some(Self(raw))
        } else {
            None
        }
    }

    /// Construct from a single canonical scope constant.
    #[must_use]
    pub const fn single(scope: u64) -> Self {
        Self(scope & SCOPE_KNOWN_MASK)
    }

    /// Returns the raw u64 representation suitable for DB persistence
    /// (Neon `pat.scopes` BIGINT column, per data_model.md §4.1).
    #[must_use]
    pub const fn to_u64(self) -> u64 {
        self.0
    }

    /// `true` iff every bit in `required` is also set in `self`.
    /// Bitwise hot-path scope check used by the middleware.
    #[must_use]
    pub const fn has(self, required: u64) -> bool {
        let masked = required & SCOPE_KNOWN_MASK;
        (self.0 & masked) == masked
    }

    /// Returns a new `PatScopes` with the additional scope bit set.
    #[must_use]
    pub const fn add(self, scope: u64) -> Self {
        Self((self.0 | scope) & SCOPE_KNOWN_MASK)
    }

    /// Returns a new `PatScopes` with the given scope bit cleared.
    #[must_use]
    pub const fn remove(self, scope: u64) -> Self {
        Self(self.0 & !scope & SCOPE_KNOWN_MASK)
    }

    /// Bitwise union with another scope set.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self((self.0 | other.0) & SCOPE_KNOWN_MASK)
    }

    /// Bitwise intersection with another scope set.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0 & SCOPE_KNOWN_MASK)
    }

    /// Returns the canonical wire-form names for every set bit. Order
    /// matches the bit-position order so audit logs are deterministic.
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        const ENTRIES: &[(u64, &str)] = &[
            (SCOPE_CACHE_R, "cache:r"),
            (SCOPE_CACHE_W, "cache:w"),
            (SCOPE_CACHE_FIND, "cache:find-missing"),
            (SCOPE_CACHE_DELETE, "cache:delete"),
            (SCOPE_ADMIN_TENANT_R, "admin:tenant-read"),
            (SCOPE_ADMIN_TENANT_W, "admin:tenant-write"),
            (SCOPE_ADMIN_TOKENS, "admin:tokens"),
            (SCOPE_ADMIN_BILLING, "admin:billing"),
            (SCOPE_ADMIN_AUDIT, "admin:audit"),
            (SCOPE_ADMIN_USERS, "admin:users"),
            (SCOPE_EXECUTE_ACTION, "execute:action"),
            (SCOPE_REPORT_RESULT, "report:result"),
        ];
        let mut out = Vec::new();
        for (bit, name) in ENTRIES {
            if self.0 & *bit != 0 {
                out.push(*name);
            }
        }
        out
    }

    /// Is the scope set empty?
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Debug for PatScopes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Scope set is non-PII, safe to render as the canonical name list.
        f.debug_tuple("PatScopes").field(&self.names()).finish()
    }
}

impl Default for PatScopes {
    fn default() -> Self {
        Self::empty()
    }
}

impl std::ops::BitOr for PatScopes {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl std::ops::BitAnd for PatScopes {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        self.intersection(rhs)
    }
}
