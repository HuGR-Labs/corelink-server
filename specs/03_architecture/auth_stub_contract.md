---
id: "AUTH-STUB-CONTRACT"
type: "architecture"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-25"
updated: "2026-04-25"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["architecture", "auth", "stub", "interface-contract", "s02-s03-bridge"]
---

# Auth Stub Interface Contract — S-02↔S-03 Bridge

> **Propósito**: definir o **interface contract frozen** entre o stub de auth usado em S-01/S-02 e a implementação real Clerk-based de S-03. Sem este contract, S-02 implementation com stub diverge de S-03 real → integration drift catastrophic em integration tests.
>
> Adicionado em Lote 9.5b endereçando **Opus R3 H-N3-04**: "S-02 risk register R-S02-007 placeholder; integration interface S-03↔S-02 não definido".
>
> **Frozen para Lote 9.5b**: alterações requerem ADR + Architect + Security lead sign-off.

---

## Sumário

1. [Princípios](#1-princípios)
2. [TenantCtx struct (Rust signature)](#2-tenantctx-struct-rust-signature)
3. [AuthMiddleware trait](#3-authmiddleware-trait)
4. [Error contract](#4-error-contract)
5. [Stub adapter (S-02)](#5-stub-adapter-s-02)
6. [Real adapter (S-03 Clerk)](#6-real-adapter-s-03-clerk)
7. [Migration path stub → real](#7-migration-path-stub--real)
8. [Integration test coverage](#8-integration-test-coverage)

---

## 1. Princípios

1. **Single trait `AuthMiddleware`**: mesma interface para stub e real; substituição é trivial via DI/feature flag.
2. **Frozen ABI**: campos de `TenantCtx` não mudam entre stub e real.
3. **Failure modes simétricos**: stub retorna mesmo error variants que real (CTRL-CRED-004, CTRL-AUTH-010).
4. **No data divergence**: stub usa fixtures que respeitam regras de produção (PAT format, tenant_id UUID v4, scopes typed).
5. **Test fixtures shared**: `tests/fixtures/auth_test_pats.rs` é shared entre S-02 e S-03 tests.

---

## 2. TenantCtx struct (Rust signature)

**Frozen** — qualquer mudança requer ADR.

```rust
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct TenantCtx {
    /// Stable identifier of tenant. Derived from PAT validation.
    pub tenant_id: Uuid,

    /// Stable identifier of user (PAT owner). May differ from tenant_id em multi-user tenant.
    pub user_id: Uuid,

    /// PAT scopes válidos para esta request. Tipped enum (não free-form string).
    pub scopes: BTreeSet<Scope>,

    /// Last MFA verification timestamp. None se MFA não-verified (admin ops require Some + freshness).
    pub mfa_ts: Option<DateTime<Utc>>,

    /// Optional region pinning (S-14 forward-looking). None until S-14.
    pub region_pinned: Option<Region>,

    /// PAT identifier (for revocation tracking + audit).
    pub pat_id: Uuid,

    /// Request unique correlation ID (CTRL-OBS-001 alignment).
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    CacheRead,        // "cache:r"
    CacheWrite,       // "cache:w"
    AdminRead,        // "admin:read"
    AdminWrite,       // "admin:write"
    BillingAdmin,     // "billing:admin"
    PrivacyAdmin,     // "privacy:admin"
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Region {
    Wnam,
    Enam,
    Weur,
    Sam,
}
```

---

## 3. AuthMiddleware trait

```rust
use async_trait::async_trait;
use http::HeaderMap;

#[async_trait]
pub trait AuthMiddleware: Send + Sync {
    /// Extract + validate auth from request headers.
    /// Stub returns fixture ctx; real validates Clerk JWT + checks PAT.
    async fn authenticate(&self, headers: &HeaderMap) -> Result<TenantCtx, AuthError>;

    /// Verify scope is included in ctx.
    /// Same logic in stub and real.
    fn require_scope(&self, ctx: &TenantCtx, required: Scope) -> Result<(), AuthError> {
        if ctx.scopes.contains(&required) {
            Ok(())
        } else {
            Err(AuthError::ScopeInsufficient { required })
        }
    }

    /// Verify MFA freshness (CTRL-AUTH-010): last MFA ≤ 30 min para admin ops.
    /// Same logic in stub and real.
    fn require_mfa_fresh(&self, ctx: &TenantCtx, max_age_minutes: u64) -> Result<(), AuthError> {
        match ctx.mfa_ts {
            None => Err(AuthError::MfaRequired),
            Some(ts) => {
                let age = Utc::now() - ts;
                if age.num_minutes() <= max_age_minutes as i64 {
                    Ok(())
                } else {
                    Err(AuthError::MfaStale)
                }
            }
        }
    }
}
```

---

## 4. Error contract

**Frozen** — error variants compartilhados entre stub e real.

```rust
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("PAT invalid or expired")]
    PatInvalid,                    // → COR_AUTH_PAT_INVALID (HTTP 401)

    #[error("PAT has been revoked")]
    PatRevoked,                    // → COR_AUTH_PAT_REVOKED (HTTP 401)

    #[error("MFA verification required")]
    MfaRequired,                   // → COR_AUTH_MFA_REQUIRED (HTTP 401)

    #[error("MFA verification expired")]
    MfaStale,                      // → COR_AUTH_MFA_STALE (HTTP 401)

    #[error("PAT scope insufficient: required {required:?}")]
    ScopeInsufficient { required: Scope }, // → COR_AUTH_SCOPE_INSUFFICIENT (HTTP 403)

    #[error("Internal auth backend unavailable")]
    BackendUnavailable,            // → COR_SERVICE_DEGRADED (HTTP 503)
}
```

Mapping para error_taxonomy.md: cada variant tem error_code COR_AUTH_* documented.

---

## 5. Stub adapter (S-02)

```rust
pub struct StubAuthMiddleware {
    fixtures: HashMap<String, TenantCtx>,
}

impl StubAuthMiddleware {
    pub fn from_test_fixtures() -> Self {
        // Loads tests/fixtures/auth_test_pats.rs
        Self {
            fixtures: load_fixtures(),
        }
    }
}

#[async_trait]
impl AuthMiddleware for StubAuthMiddleware {
    async fn authenticate(&self, headers: &HeaderMap) -> Result<TenantCtx, AuthError> {
        let token = extract_bearer_token(headers).ok_or(AuthError::PatInvalid)?;
        // Stub-only: lookup em fixture map (não decode real JWT)
        self.fixtures.get(&token)
            .cloned()
            .ok_or(AuthError::PatInvalid)
    }
    // require_scope + require_mfa_fresh herdam default impl
}
```

**Stub behavior contract:**
- Reads `Authorization: Bearer <token>` header.
- Looks up token em fixture map (test PATs).
- Returns `TenantCtx` from fixture OR `AuthError::PatInvalid`.
- **No real crypto**: stub não verifica JWT signature (would slow tests).
- **No revocation**: stub não consulta DO/KV (always-valid fixtures).
- **No MFA real**: fixture pode include `mfa_ts: Some(now())` para admin tests.

---

## 6. Real adapter (S-03 Clerk)

```rust
pub struct ClerkAuthMiddleware {
    jwks_cache: Arc<JwksCache>,            // KV-backed 24h TTL
    revocation_do: Arc<RevocationClient>,  // DO ≤ 60s propagation
    pat_store: Arc<NeonPatStore>,          // Argon2id verify
}

#[async_trait]
impl AuthMiddleware for ClerkAuthMiddleware {
    async fn authenticate(&self, headers: &HeaderMap) -> Result<TenantCtx, AuthError> {
        let token = extract_bearer_token(headers).ok_or(AuthError::PatInvalid)?;

        // Step 1: validate JWT signature via JWKS cache (CTRL-AUTH-001)
        let claims = self.validate_jwt(&token).await
            .map_err(|_| AuthError::PatInvalid)?;

        // Step 2: check revocation via DO (CTRL-CRED-004; ≤ 60s)
        if self.revocation_do.is_revoked(claims.pat_id).await? {
            return Err(AuthError::PatRevoked);
        }

        // Step 3: timing-safe PAT verify via Argon2id (CTRL-AUTH-001)
        self.pat_store.verify_argon2id(&token).await?;

        // Step 4: build TenantCtx from validated claims
        Ok(TenantCtx {
            tenant_id: claims.tenant_id,
            user_id: claims.user_id,
            scopes: parse_scopes(&claims.scopes)?,
            mfa_ts: claims.mfa_ts,
            region_pinned: None,  // S-14 add
            pat_id: claims.pat_id,
            request_id: claims.request_id,
        })
    }
}
```

**Real behavior contract:**
- Same trait + same return type → **drop-in replacement** for stub.
- Adds: JWT validate, JWKS cache 24h, revocation check ≤ 60s, Argon2id timing-safe verify, audit emit.
- Same `AuthError` variants → no error vocabulary divergence.

---

## 7. Migration path stub → real

**Pre-S-03 SEALED** (S-01 + S-02 sprints):
```rust
// In Worker startup
let auth_mw: Arc<dyn AuthMiddleware> = Arc::new(StubAuthMiddleware::from_test_fixtures());
```

**Post-S-03 SEALED**:
```rust
// In Worker startup (real)
let auth_mw: Arc<dyn AuthMiddleware> = Arc::new(
    ClerkAuthMiddleware::new(jwks_cache, revocation_do, pat_store)
);
```

**Feature flag during transition** (S-03 progressive rollout):
```rust
let auth_mw: Arc<dyn AuthMiddleware> = if config.use_real_auth {
    Arc::new(ClerkAuthMiddleware::new(...))
} else {
    Arc::new(StubAuthMiddleware::from_test_fixtures())
};
```

---

## 8. Integration test coverage

Test fixtures shared:

```
tests/fixtures/auth_test_pats.rs  # shared between S-02 + S-03 + S-04 tests
```

Covers:
- Tenant A PAT scope `cache:r` → success on read
- Tenant A PAT scope `cache:w` → success on write
- Tenant B PAT trying access Tenant A blob → `AuthError::PatInvalid` OR `AuthError::ScopeInsufficient` (cross-tenant detected)
- Revoked PAT → `AuthError::PatRevoked`
- Stale MFA (> 30 min) for admin op → `AuthError::MfaStale`
- Missing MFA for admin op → `AuthError::MfaRequired`

Integration test cobre:
1. **S-02 com stub**: full read path; expected behavior.
2. **S-03 transition**: same test runs com real adapter; same assertions pass.
3. **Drift detection**: CI matrix test runs ambos `stub` + `real` adapters; mismatch = CI red.

CI gate (post-S-03):
```yaml
# .github/workflows/auth_drift.yml
- name: Stub adapter tests
  run: cargo test --features=auth-stub --test integration
- name: Real adapter tests
  run: cargo test --features=auth-real --test integration
```

---

## 9. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação interface contract S-02↔S-03 bridge (Lote 9.5b Phase 4 — Opus R3 H-N3-04 fix). |

---

**Frozen para Lote 9.5b. Alterações: ADR + Architect + Security lead sign-off.**
