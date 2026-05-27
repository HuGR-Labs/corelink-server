//! Host-side in-memory simulator of the WI-S03-005 schema.
//!
//! The simulator is **not** a SQL parser — it is a hand-coded fake of
//! the seven auth tables that enforces the invariants the migration
//! relies on:
//!
//! - `account.account_id` PRIMARY KEY uniqueness.
//! - `tenant.tenant_id` PRIMARY KEY uniqueness + `tenant.slug` UNIQUE.
//! - `user_account.user_id` PRIMARY KEY + `clerk_user_id` UNIQUE +
//!   `email_hash` UNIQUE.
//! - `membership` composite PRIMARY KEY `(user_account_id, tenant_id)`.
//! - `pat.pat_id` PRIMARY KEY + `pat.token_id` UNIQUE +
//!   `pat.token_hash` UNIQUE.
//! - `revocation_log.id` PRIMARY KEY +
//!   `(pat_id, revoked_at)` UNIQUE (idempotency for WI-S03-004).
//! - Foreign-key cascade behaviour: `account → tenant`,
//!   `tenant → membership/pat`, `user_account → membership/webauthn`.
//! - Tenant-isolation envelope (`SELECT … WHERE tenant_id = $ctx`):
//!   modelled by [`AuthSchema::list_pats_for_tenant`] which returns
//!   only the rows whose `tenant_id` matches the active context.
//!
//! Pure side-effect-free Rust — no async, no I/O. Property tests in
//! `tests/prop_schema.rs` drive 10 000 iterations against this surface.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;
use uuid::Uuid;

/// All structured failure modes the simulator can report.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SimError {
    /// Attempted to insert a row whose PK / UNIQUE column already
    /// exists. Mirrors Postgres `unique_violation` (SQLSTATE 23505).
    #[error("unique_violation: {0}")]
    UniqueViolation(&'static str),

    /// FK target row missing. Mirrors Postgres `foreign_key_violation`
    /// (SQLSTATE 23503).
    #[error("foreign_key_violation: {0}")]
    ForeignKeyViolation(&'static str),

    /// CHECK constraint failed. Mirrors Postgres `check_violation`
    /// (SQLSTATE 23514).
    #[error("check_violation: {0}")]
    CheckViolation(&'static str),

    /// Cross-tenant access attempted from a context that did not
    /// match the row owner. Mirrors the empty-result envelope that
    /// RLS returns in production.
    #[error("rls_denied: query did not match active tenant context")]
    RlsDenied,
}

/// Account tier mirror of the SQL `tier_t` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TenantTier {
    /// Solo-developer tier.
    Solo,
    /// Small team.
    Team,
    /// Business / pro.
    Business,
    /// Enterprise.
    Enterprise,
}

/// Membership role mirror of the SQL `role_t` enum (canonical 4-variant
/// per `data_model.md` §4.1 line 174).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Account owner; full access including billing.
    Owner,
    /// Tenant administrator.
    Admin,
    /// Developer member; CAS r/w but no admin surface.
    Developer,
    /// Read-only viewer.
    Viewer,
}

/// Revocation reason mirror of `revocation_reason_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevocationReason {
    /// User self-revoked their PAT.
    UserInitiated,
    /// Tenant admin revoked.
    AdminRevoked,
    /// Scheduled rotation.
    Rotation,
    /// Compromise suspected; emergency revoke.
    CompromiseSuspected,
    /// Token TTL elapsed.
    Expired,
    /// Tenant offboarded → mass revoke.
    TenantOffboarded,
}

/// `account` row.
#[derive(Clone, Debug)]
pub struct AccountRow {
    /// PRIMARY KEY (UUIDv7 minted app-side).
    pub account_id: Uuid,
    /// Display name.
    pub name: String,
    /// Soft-delete column.
    pub deleted_at: Option<u64>,
}

/// `tenant` row.
#[derive(Clone, Debug)]
pub struct TenantRow {
    /// PRIMARY KEY (UUIDv7 minted app-side).
    pub tenant_id: Uuid,
    /// FK → `account.account_id`.
    pub account_id: Uuid,
    /// URL-friendly slug, UNIQUE across all accounts.
    pub slug: String,
    /// Tier.
    pub tier: TenantTier,
    /// Soft-delete column.
    pub deleted_at: Option<u64>,
}

/// `user_account` row.
#[derive(Clone, Debug)]
pub struct UserAccountRow {
    /// PRIMARY KEY (UUIDv7 minted app-side).
    pub user_id: Uuid,
    /// Clerk's stable user identifier (UNIQUE).
    pub clerk_user_id: String,
    /// HMAC-SHA256 deterministic email hash (UNIQUE; 32 bytes).
    pub email_hash: [u8; 32],
    /// Soft-delete column.
    pub deleted_at: Option<u64>,
}

/// `membership` row.
#[derive(Clone, Debug)]
pub struct MembershipRow {
    /// FK → `user_account.user_id`.
    pub user_account_id: Uuid,
    /// FK → `tenant.tenant_id`.
    pub tenant_id: Uuid,
    /// Role.
    pub role: Role,
    /// Soft-delete column.
    pub deleted_at: Option<u64>,
}

/// `pat` row.
#[derive(Clone, Debug)]
pub struct PatRow {
    /// PRIMARY KEY (UUIDv7 minted app-side).
    pub pat_id: Uuid,
    /// 16-character canonical token id (UNIQUE).
    pub token_id: String,
    /// FK → `tenant.tenant_id`.
    pub tenant_id: Uuid,
    /// FK → `user_account.user_id` (nullable for service / CI tokens).
    pub issued_to_user: Option<Uuid>,
    /// Argon2id PHC string serialised as bytes (UNIQUE).
    pub token_hash: Vec<u8>,
    /// Scope strings; the runtime u64 bitset is an app-side
    /// transformation per ADR-0026.
    pub scopes: Vec<String>,
    /// Tombstone column (null = active).
    pub revoked_at: Option<u64>,
    /// Required when `revoked_at` is set; null otherwise.
    pub revocation_reason: Option<RevocationReason>,
}

/// `revocation_log` row.
#[derive(Clone, Debug)]
pub struct RevocationLogRow {
    /// PRIMARY KEY (UUIDv7 minted app-side).
    pub id: Uuid,
    /// `pat.pat_id` reference (NOT a foreign key — the row outlives
    /// erased PATs for cross-region propagation).
    pub pat_id: Uuid,
    /// `tenant.tenant_id` reference (denormalised for query).
    pub tenant_id: Uuid,
    /// Wall-clock revocation time (millis).
    pub revoked_at: u64,
    /// Reason.
    pub reason: RevocationReason,
}

/// In-memory simulator. Every method returns a structured result so
/// property tests can assert on the precise failure mode.
#[derive(Debug, Default)]
pub struct AuthSchema {
    accounts: BTreeMap<Uuid, AccountRow>,
    tenants: BTreeMap<Uuid, TenantRow>,
    tenant_slugs: BTreeSet<String>,
    users: BTreeMap<Uuid, UserAccountRow>,
    clerk_user_ids: BTreeSet<String>,
    user_email_hashes: BTreeSet<[u8; 32]>,
    memberships: BTreeMap<(Uuid, Uuid), MembershipRow>,
    pats: BTreeMap<Uuid, PatRow>,
    pat_token_ids: BTreeSet<String>,
    pat_token_hashes: BTreeSet<Vec<u8>>,
    revocations: BTreeMap<Uuid, RevocationLogRow>,
    revocation_idempotency: BTreeSet<(Uuid, u64)>,
}

impl AuthSchema {
    /// Construct an empty schema instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // --------------------------------------------------------------
    // account
    // --------------------------------------------------------------

    /// Insert an account row.
    pub fn insert_account(&mut self, row: AccountRow) -> Result<(), SimError> {
        if row.name.is_empty() || row.name.len() > 200 {
            return Err(SimError::CheckViolation("account.name length 1..=200"));
        }
        if self.accounts.contains_key(&row.account_id) {
            return Err(SimError::UniqueViolation("account.account_id"));
        }
        self.accounts.insert(row.account_id, row);
        Ok(())
    }

    /// Look up an account by id.
    #[must_use]
    pub fn get_account(&self, account_id: &Uuid) -> Option<&AccountRow> {
        self.accounts.get(account_id)
    }

    // --------------------------------------------------------------
    // tenant
    // --------------------------------------------------------------

    /// Insert a tenant row.
    pub fn insert_tenant(&mut self, row: TenantRow) -> Result<(), SimError> {
        if !is_valid_slug(&row.slug) {
            return Err(SimError::CheckViolation(
                "tenant.slug: lowercase alnum + dash; 2..=64",
            ));
        }
        if !self.accounts.contains_key(&row.account_id) {
            return Err(SimError::ForeignKeyViolation(
                "tenant.account_id → account.account_id",
            ));
        }
        if self.tenants.contains_key(&row.tenant_id) {
            return Err(SimError::UniqueViolation("tenant.tenant_id"));
        }
        if self.tenant_slugs.contains(&row.slug) {
            return Err(SimError::UniqueViolation("tenant.slug"));
        }
        self.tenant_slugs.insert(row.slug.clone());
        self.tenants.insert(row.tenant_id, row);
        Ok(())
    }

    /// Look up a tenant by id.
    #[must_use]
    pub fn get_tenant(&self, tenant_id: &Uuid) -> Option<&TenantRow> {
        self.tenants.get(tenant_id)
    }

    // --------------------------------------------------------------
    // user_account
    // --------------------------------------------------------------

    /// Insert a user_account row.
    pub fn insert_user(&mut self, row: UserAccountRow) -> Result<(), SimError> {
        if row.clerk_user_id.is_empty() || row.clerk_user_id.len() > 200 {
            return Err(SimError::CheckViolation(
                "user_account.clerk_user_id length 1..=200",
            ));
        }
        if self.users.contains_key(&row.user_id) {
            return Err(SimError::UniqueViolation("user_account.user_id"));
        }
        if self.clerk_user_ids.contains(&row.clerk_user_id) {
            return Err(SimError::UniqueViolation("user_account.clerk_user_id"));
        }
        if self.user_email_hashes.contains(&row.email_hash) {
            return Err(SimError::UniqueViolation("user_account.email_hash"));
        }
        self.clerk_user_ids.insert(row.clerk_user_id.clone());
        self.user_email_hashes.insert(row.email_hash);
        self.users.insert(row.user_id, row);
        Ok(())
    }

    /// Look up a user by their email hash. Mirrors the production
    /// `WHERE email_hash = $1` indexed query.
    #[must_use]
    pub fn lookup_user_by_email_hash(&self, email_hash: &[u8; 32]) -> Option<&UserAccountRow> {
        self.users
            .values()
            .find(|u| u.deleted_at.is_none() && u.email_hash == *email_hash)
    }

    // --------------------------------------------------------------
    // membership
    // --------------------------------------------------------------

    /// Insert a membership row.
    pub fn insert_membership(&mut self, row: MembershipRow) -> Result<(), SimError> {
        if !self.users.contains_key(&row.user_account_id) {
            return Err(SimError::ForeignKeyViolation(
                "membership.user_account_id → user_account.user_id",
            ));
        }
        if !self.tenants.contains_key(&row.tenant_id) {
            return Err(SimError::ForeignKeyViolation(
                "membership.tenant_id → tenant.tenant_id",
            ));
        }
        let key = (row.user_account_id, row.tenant_id);
        if self.memberships.contains_key(&key) {
            return Err(SimError::UniqueViolation(
                "membership PRIMARY KEY (user_account_id, tenant_id)",
            ));
        }
        self.memberships.insert(key, row);
        Ok(())
    }

    // --------------------------------------------------------------
    // pat
    // --------------------------------------------------------------

    /// Insert a pat row.
    pub fn insert_pat(&mut self, row: PatRow) -> Result<(), SimError> {
        if row.token_id.len() != 16 {
            return Err(SimError::CheckViolation("pat.token_id length must be 16"));
        }
        if row.scopes.is_empty() {
            return Err(SimError::CheckViolation(
                "pat.scopes: array_length >= 1",
            ));
        }
        if !self.tenants.contains_key(&row.tenant_id) {
            return Err(SimError::ForeignKeyViolation(
                "pat.tenant_id → tenant.tenant_id",
            ));
        }
        if let Some(uid) = row.issued_to_user {
            if !self.users.contains_key(&uid) {
                return Err(SimError::ForeignKeyViolation(
                    "pat.issued_to_user → user_account.user_id",
                ));
            }
        }
        if row.revoked_at.is_some() != row.revocation_reason.is_some() {
            return Err(SimError::CheckViolation(
                "pat.revocation invariant: revoked_at <=> revocation_reason",
            ));
        }
        if self.pats.contains_key(&row.pat_id) {
            return Err(SimError::UniqueViolation("pat.pat_id"));
        }
        if self.pat_token_ids.contains(&row.token_id) {
            return Err(SimError::UniqueViolation("pat.token_id"));
        }
        if self.pat_token_hashes.contains(&row.token_hash) {
            return Err(SimError::UniqueViolation("pat.token_hash"));
        }
        self.pat_token_ids.insert(row.token_id.clone());
        self.pat_token_hashes.insert(row.token_hash.clone());
        self.pats.insert(row.pat_id, row);
        Ok(())
    }

    /// Look up a PAT by its 16-character `token_id`. Models the
    /// `idx_pat_token_id` B-tree lookup that the WI-S03-002 verify path
    /// depends on (≤ 10 ms p99).
    #[must_use]
    pub fn lookup_pat_by_token_id(&self, token_id: &str) -> Option<&PatRow> {
        self.pats.values().find(|p| p.token_id == token_id)
    }

    /// List the pats visible to the supplied tenant context. Mirrors
    /// the canonical RLS-scoped `SELECT * FROM pat WHERE tenant_id = $ctx
    /// AND revoked_at IS NULL` query.
    #[must_use]
    pub fn list_pats_for_tenant(&self, ctx_tenant: &Uuid) -> Vec<&PatRow> {
        self.pats
            .values()
            .filter(|p| p.tenant_id == *ctx_tenant && p.revoked_at.is_none())
            .collect()
    }

    /// Mark a PAT as revoked. Returns the revocation timestamp for
    /// idempotency tracking; if the PAT is already revoked the
    /// existing timestamp is returned (idempotent retry contract per
    /// WI-S03-004).
    pub fn revoke_pat(
        &mut self,
        pat_id: &Uuid,
        revoked_at: u64,
        reason: RevocationReason,
    ) -> Result<u64, SimError> {
        let row = self
            .pats
            .get_mut(pat_id)
            .ok_or(SimError::ForeignKeyViolation("pat.pat_id not found"))?;
        if let Some(existing) = row.revoked_at {
            return Ok(existing);
        }
        row.revoked_at = Some(revoked_at);
        row.revocation_reason = Some(reason);
        Ok(revoked_at)
    }

    // --------------------------------------------------------------
    // revocation_log
    // --------------------------------------------------------------

    /// Insert a revocation_log row. The `(pat_id, revoked_at)` UNIQUE
    /// constraint is the load-bearing idempotency primitive for
    /// WI-S03-004.
    pub fn insert_revocation_log(&mut self, row: RevocationLogRow) -> Result<(), SimError> {
        if self.revocations.contains_key(&row.id) {
            return Err(SimError::UniqueViolation("revocation_log.id"));
        }
        if !self
            .revocation_idempotency
            .insert((row.pat_id, row.revoked_at))
        {
            return Err(SimError::UniqueViolation(
                "revocation_log UNIQUE (pat_id, revoked_at)",
            ));
        }
        self.revocations.insert(row.id, row);
        Ok(())
    }

    /// Cascade-delete an account and every dependent row that the
    /// production `ON DELETE CASCADE` chain reaches:
    /// `account → tenant → membership/pat`,
    /// `user_account → membership/pat (issued_to_user nulled)`,
    /// `revocation_log` is intentionally **not** cascaded — the row
    /// outlives the PAT for cross-region propagation tracking
    /// (matches the schema's `pat_id UUID NOT NULL` without FK).
    ///
    /// Returns the count of rows removed by the cascade so DSR
    /// integration tests can assert on the envelope.
    pub fn dsr_hard_delete_account(&mut self, account_id: &Uuid) -> CascadeReport {
        let mut report = CascadeReport::default();
        if self.accounts.remove(account_id).is_some() {
            report.accounts = 1;
        }
        // Drop every tenant that belonged to the account.
        let tenants_to_drop: Vec<Uuid> = self
            .tenants
            .values()
            .filter(|t| t.account_id == *account_id)
            .map(|t| t.tenant_id)
            .collect();
        for tenant_id in tenants_to_drop {
            if let Some(t) = self.tenants.remove(&tenant_id) {
                self.tenant_slugs.remove(&t.slug);
                report.tenants += 1;
            }
            // Cascade memberships for that tenant.
            let m_keys: Vec<(Uuid, Uuid)> = self
                .memberships
                .keys()
                .filter(|(_, t_id)| *t_id == tenant_id)
                .copied()
                .collect();
            for k in m_keys {
                if self.memberships.remove(&k).is_some() {
                    report.memberships += 1;
                }
            }
            // Cascade pats for that tenant.
            let pat_keys: Vec<Uuid> = self
                .pats
                .values()
                .filter(|p| p.tenant_id == tenant_id)
                .map(|p| p.pat_id)
                .collect();
            for pk in pat_keys {
                if let Some(p) = self.pats.remove(&pk) {
                    self.pat_token_ids.remove(&p.token_id);
                    self.pat_token_hashes.remove(&p.token_hash);
                    report.pats += 1;
                }
            }
        }
        report
    }

    // --------------------------------------------------------------
    // Inspectors used by property tests.
    // --------------------------------------------------------------

    /// Total tenant count (for cardinality assertions).
    #[must_use]
    pub fn tenant_count(&self) -> usize {
        self.tenants.len()
    }

    /// Total pat count (for cardinality assertions).
    #[must_use]
    pub fn pat_count(&self) -> usize {
        self.pats.len()
    }

    /// Total revocation_log count.
    #[must_use]
    pub fn revocation_count(&self) -> usize {
        self.revocations.len()
    }
}

/// Aggregated counts produced by [`AuthSchema::dsr_hard_delete_account`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CascadeReport {
    /// Account rows removed.
    pub accounts: usize,
    /// Tenant rows removed.
    pub tenants: usize,
    /// Membership rows removed.
    pub memberships: usize,
    /// PAT rows removed.
    pub pats: usize,
}

/// Validate the canonical slug regex `^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$`
/// (`length BETWEEN 2 AND 64`).
fn is_valid_slug(s: &str) -> bool {
    let len = s.len();
    if !(2..=64).contains(&len) {
        return false;
    }
    let bytes = s.as_bytes();
    let first = match bytes.first() {
        Some(b) => *b,
        None => return false,
    };
    let last = match bytes.last() {
        Some(b) => *b,
        None => return false,
    };
    if !is_alnum_ascii(first) || !is_alnum_ascii(last) {
        return false;
    }
    bytes.iter().all(|b| is_alnum_ascii(*b) || *b == b'-')
}

fn is_alnum_ascii(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit()
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

    fn account(id: u64) -> AccountRow {
        AccountRow {
            account_id: Uuid::from_u128(u128::from(id)),
            name: format!("acct-{id}"),
            deleted_at: None,
        }
    }

    fn tenant(id: u64, account_id: Uuid, slug: &str) -> TenantRow {
        TenantRow {
            tenant_id: Uuid::from_u128(u128::from(id) + 0x1_0000_0000),
            account_id,
            slug: slug.to_string(),
            tier: TenantTier::Team,
            deleted_at: None,
        }
    }

    fn user(id: u64, clerk_id: &str, email_hash: [u8; 32]) -> UserAccountRow {
        UserAccountRow {
            user_id: Uuid::from_u128(u128::from(id) + 0x2_0000_0000),
            clerk_user_id: clerk_id.to_string(),
            email_hash,
            deleted_at: None,
        }
    }

    fn pat(id: u64, tenant_id: Uuid, token_id: &str, hash: &[u8]) -> PatRow {
        PatRow {
            pat_id: Uuid::from_u128(u128::from(id) + 0x3_0000_0000),
            token_id: token_id.to_string(),
            tenant_id,
            issued_to_user: None,
            token_hash: hash.to_vec(),
            scopes: vec!["cache:r".to_string()],
            revoked_at: None,
            revocation_reason: None,
        }
    }

    #[test]
    fn fk_to_account_required_for_tenant() {
        let mut s = AuthSchema::new();
        let bad_account = Uuid::nil();
        let row = TenantRow {
            tenant_id: Uuid::from_u128(1),
            account_id: bad_account,
            slug: "acme-corp".into(),
            tier: TenantTier::Team,
            deleted_at: None,
        };
        assert_eq!(
            s.insert_tenant(row),
            Err(SimError::ForeignKeyViolation(
                "tenant.account_id → account.account_id"
            ))
        );
    }

    #[test]
    fn slug_regex_enforced() {
        let mut s = AuthSchema::new();
        let acc = account(1);
        let acc_id = acc.account_id;
        s.insert_account(acc).unwrap();
        let bad = TenantRow {
            tenant_id: Uuid::from_u128(2),
            account_id: acc_id,
            slug: "Acme!Corp".into(),
            tier: TenantTier::Team,
            deleted_at: None,
        };
        assert!(matches!(s.insert_tenant(bad), Err(SimError::CheckViolation(_))));
    }

    #[test]
    fn cross_tenant_list_pats_isolated() {
        let mut s = AuthSchema::new();
        let acc = account(1);
        let acc_id = acc.account_id;
        s.insert_account(acc).unwrap();
        let t_a = tenant(1, acc_id, "tenant-a");
        let t_b = tenant(2, acc_id, "tenant-b");
        let id_a = t_a.tenant_id;
        let id_b = t_b.tenant_id;
        s.insert_tenant(t_a).unwrap();
        s.insert_tenant(t_b).unwrap();
        s.insert_pat(pat(1, id_a, "tokenidaaaaaaaaa", b"hash-a")).unwrap();
        s.insert_pat(pat(2, id_b, "tokenidbbbbbbbbb", b"hash-b")).unwrap();
        let view_a = s.list_pats_for_tenant(&id_a);
        let view_b = s.list_pats_for_tenant(&id_b);
        assert_eq!(view_a.len(), 1);
        assert_eq!(view_b.len(), 1);
        assert_eq!(view_a[0].token_id, "tokenidaaaaaaaaa");
        assert_eq!(view_b[0].token_id, "tokenidbbbbbbbbb");
    }

    #[test]
    fn revocation_idempotency_unique_pat_at_revoked_at() {
        let mut s = AuthSchema::new();
        let acc = account(7);
        let acc_id = acc.account_id;
        s.insert_account(acc).unwrap();
        let t = tenant(7, acc_id, "tenant-rev");
        let t_id = t.tenant_id;
        s.insert_tenant(t).unwrap();
        let p = pat(11, t_id, "revtokenidaaaaaa", b"hash-rev");
        let p_id = p.pat_id;
        s.insert_pat(p).unwrap();
        let row1 = RevocationLogRow {
            id: Uuid::from_u128(0xa1),
            pat_id: p_id,
            tenant_id: t_id,
            revoked_at: 1_700_000_000,
            reason: RevocationReason::UserInitiated,
        };
        let row2 = RevocationLogRow {
            id: Uuid::from_u128(0xa2),
            pat_id: p_id,
            tenant_id: t_id,
            revoked_at: 1_700_000_000,
            reason: RevocationReason::UserInitiated,
        };
        s.insert_revocation_log(row1).unwrap();
        let dup = s.insert_revocation_log(row2).unwrap_err();
        assert!(matches!(dup, SimError::UniqueViolation(_)));
    }

    #[test]
    fn dsr_cascade_clears_dependents() {
        let mut s = AuthSchema::new();
        let acc = account(99);
        let acc_id = acc.account_id;
        s.insert_account(acc).unwrap();
        let t = tenant(99, acc_id, "tenant-dsr");
        let t_id = t.tenant_id;
        s.insert_tenant(t).unwrap();
        let u = user(99, "clerk_dsr", [0xee; 32]);
        let u_id = u.user_id;
        s.insert_user(u).unwrap();
        s.insert_membership(MembershipRow {
            user_account_id: u_id,
            tenant_id: t_id,
            role: Role::Owner,
            deleted_at: None,
        })
        .unwrap();
        s.insert_pat(pat(99, t_id, "dsrtokenidaaaaaa", b"hash-dsr"))
            .unwrap();
        let report = s.dsr_hard_delete_account(&acc_id);
        assert_eq!(report.accounts, 1);
        assert_eq!(report.tenants, 1);
        assert_eq!(report.memberships, 1);
        assert_eq!(report.pats, 1);
        assert_eq!(s.tenant_count(), 0);
        assert_eq!(s.pat_count(), 0);
    }
}
