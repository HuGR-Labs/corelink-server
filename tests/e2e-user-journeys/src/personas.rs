//! # Personas — the actors the journeys impersonate.
//!
//! A persona binds together (tenant, token, expectation): who is making the
//! call, with which PAT, and whether the call is expected to SUCCEED or be
//! DENIED. Journey modules ask for a persona and get back a [`Resolved`]
//! handle (or a gate reason when the persona's token env var is absent).
//!
//! The persona ids (P1..P12) follow the journey-matrix plan. The matrix file
//! was not present in-tree at harness-freeze time (flagged in the CARD); these
//! ids are the canonical seed the fleet fills against.

use crate::harness::{Config, TokenKind};

/// The cast of actors. Each maps to a [`TokenKind`] and an [`Expectation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persona {
    /// P1 — primary tenant, read+write cache PAT. Happy path.
    P1ReadWrite,
    /// P2 — primary tenant, read-only PAT. Reads succeed; writes denied.
    P2ReadOnly,
    /// P3 — primary tenant, admin-scoped PAT.
    P3Admin,
    /// P4 — a revoked PAT. Every call must be denied.
    P4Revoked,
    /// P5 — an expired PAT. Every call must be denied.
    P5Expired,
    /// P6 — a DIFFERENT tenant's valid PAT (isolation adversary).
    P6TenantB,
    /// P7 — Free-plan tenant.
    P7Free,
    /// P8 — Solo-plan tenant.
    P8Solo,
    /// P9 — Pro-plan tenant.
    P9Pro,
    /// P10 — Enterprise-plan tenant.
    P10Enterprise,
    /// P11 — past-due subscription (billing/quota denial).
    P11PastDue,
    /// P12 — fully anonymous (no Authorization header at all).
    P12Anonymous,
}

/// Whether a persona's calls are expected to succeed or be denied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expectation {
    /// Authenticated, capable: protected calls should succeed (2xx).
    Allowed,
    /// Authenticated but read-only: writes should be denied, reads allowed.
    ReadOnly,
    /// Must be denied on every protected call (revoked/expired/anon/cross).
    Denied,
}

/// A persona resolved against the live [`Config`]: the tenant + token + the
/// expectation. `token` is `None` only for the anonymous persona.
pub struct Resolved<'a> {
    /// The persona this was resolved from.
    pub persona: Persona,
    /// The tenant id this persona addresses in path-tenant surfaces.
    pub tenant: String,
    /// The bearer token, or `None` for the anonymous persona.
    pub token: Option<&'a str>,
    /// What the journey should assert for this persona.
    pub expectation: Expectation,
}

impl Persona {
    /// The token slot this persona draws from (anonymous draws none).
    pub fn token_kind(self) -> Option<TokenKind> {
        Some(match self {
            Persona::P1ReadWrite => TokenKind::ReadWrite,
            Persona::P2ReadOnly => TokenKind::ReadOnly,
            Persona::P3Admin => TokenKind::Admin,
            Persona::P4Revoked => TokenKind::Revoked,
            Persona::P5Expired => TokenKind::Expired,
            Persona::P6TenantB => TokenKind::TenantB,
            Persona::P7Free => TokenKind::Free,
            Persona::P8Solo => TokenKind::Solo,
            Persona::P9Pro => TokenKind::Pro,
            Persona::P10Enterprise => TokenKind::Enterprise,
            Persona::P11PastDue => TokenKind::PastDue,
            Persona::P12Anonymous => return None,
        })
    }

    /// The expectation for this persona's protected calls.
    pub fn expectation(self) -> Expectation {
        match self {
            Persona::P2ReadOnly => Expectation::ReadOnly,
            Persona::P4Revoked
            | Persona::P5Expired
            | Persona::P6TenantB
            | Persona::P12Anonymous => Expectation::Denied,
            _ => Expectation::Allowed,
        }
    }

    /// Resolve the persona against the config. Returns `Err(reason)` when the
    /// backing token env var is absent (→ the journey should GATE on it).
    pub fn resolve<'a>(self, cfg: &'a Config) -> Result<Resolved<'a>, String> {
        // Tenant B persona addresses the second tenant; everyone else the
        // primary tenant.
        let tenant = match self {
            Persona::P6TenantB => cfg
                .tenant_b
                .clone()
                .ok_or_else(|| "CORELINK_E2E_TENANT_B not set".to_string())?,
            _ => cfg.tenant_or_anon().to_string(),
        };

        match self.token_kind() {
            None => Ok(Resolved {
                persona: self,
                tenant,
                token: None,
                expectation: self.expectation(),
            }),
            Some(kind) => {
                let token = cfg
                    .token(kind)
                    .ok_or_else(|| format!("token for {self:?} ({kind:?}) not set"))?;
                Ok(Resolved {
                    persona: self,
                    tenant,
                    token: Some(token),
                    expectation: self.expectation(),
                })
            }
        }
    }
}
