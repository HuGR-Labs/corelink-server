//! Property tests for the WI-S03-005 schema simulator.
//!
//! Coverage:
//!
//! 1. **UNIQUE constraints** — `pat.token_hash`, `pat.token_id`,
//!    `tenant.slug`, `user_account.email_hash`,
//!    `revocation_log (pat_id, revoked_at)` all reject duplicates and
//!    surface the canonical [`SimError::UniqueViolation`] variant.
//! 2. **Cross-tenant isolation** — for every pair `(tenant_a, tenant_b)`
//!    in a multi-tenant population, `list_pats_for_tenant(tenant_b)`
//!    never returns a PAT whose `tenant_id == tenant_a`. This is the
//!    canonical envelope that RLS enforces in production.
//! 3. **Revocation idempotency** — the `(pat_id, revoked_at)` UNIQUE
//!    pair guarantees WI-S03-004 retries are durable single-row
//!    outcomes.
//! 4. **DSR cascade completeness** — every account erasure removes
//!    every dependent tenant + membership + pat row.
//!
//! Iteration counts:
//!
//! - 10 000 iterations per case for the headline UNIQUE +
//!   cross-tenant invariants, matching the WI's mandate
//!   ("10k iter on UNIQUE constraints + cross-tenant isolation
//!   invariants in queries").
//! - 2 048 iterations for the larger composed cases (multi-tenant DSR
//!   cascade) — proptest cases run ~10× longer there because every
//!   case builds and tears down 4–8 tenants × 4 PATs.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use std::collections::HashSet;

use corelink_auth::schema::{
    AccountRow, AuthSchema, MembershipRow, PatRow, RevocationLogRow, RevocationReason, Role,
    SimError, TenantRow, TenantTier, UserAccountRow,
};
use proptest::prelude::*;
use uuid::Uuid;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Returns the
/// env-overridden value if set, else `default`. The 100k nightly
/// variant overrides via `PROPTEST_CASES=100_000`.
fn proptest_cases(default: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

// ----------------------------------------------------------------------------
// Strategies.
// ----------------------------------------------------------------------------

fn slug_strategy() -> impl Strategy<Value = String> {
    "[a-z0-9][a-z0-9-]{0,30}[a-z0-9]".prop_map(String::from)
}

fn token_id_strategy() -> impl Strategy<Value = String> {
    "[a-z0-9]{16}".prop_map(String::from)
}

fn email_hash_strategy() -> impl Strategy<Value = [u8; 32]> {
    proptest::array::uniform32(any::<u8>())
}

// ----------------------------------------------------------------------------
// 1. UNIQUE constraints.
// ----------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases(10_000)))]

    #[test]
    fn pat_token_hash_unique(
        slug in slug_strategy(),
        tid_a in token_id_strategy(),
        tid_b in token_id_strategy(),
        hash in proptest::collection::vec(any::<u8>(), 16..=64),
    ) {
        prop_assume!(tid_a != tid_b);
        prop_assume!(slug.len() >= 2);
        let mut s = AuthSchema::new();
        let acc = AccountRow {
            account_id: Uuid::from_u128(1),
            name: "acct".into(),
            deleted_at: None,
        };
        s.insert_account(acc).unwrap();
        let tenant_id = Uuid::from_u128(2);
        s.insert_tenant(TenantRow {
            tenant_id,
            account_id: Uuid::from_u128(1),
            slug: slug.clone(),
            tier: TenantTier::Team,
            deleted_at: None,
        }).unwrap();
        s.insert_pat(PatRow {
            pat_id: Uuid::from_u128(10),
            token_id: tid_a.clone(),
            tenant_id,
            issued_to_user: None,
            token_hash: hash.clone(),
            scopes: vec!["cache:r".into()],
            revoked_at: None,
            revocation_reason: None,
        }).unwrap();
        // Re-using the same `token_hash` under a different `pat_id`
        // and `token_id` MUST surface `UniqueViolation("pat.token_hash")`.
        let err = s.insert_pat(PatRow {
            pat_id: Uuid::from_u128(11),
            token_id: tid_b,
            tenant_id,
            issued_to_user: None,
            token_hash: hash,
            scopes: vec!["cache:r".into()],
            revoked_at: None,
            revocation_reason: None,
        }).unwrap_err();
        prop_assert_eq!(err, SimError::UniqueViolation("pat.token_hash"));
    }

    #[test]
    fn pat_token_id_unique(
        slug in slug_strategy(),
        token_id in token_id_strategy(),
        hash_a in proptest::collection::vec(any::<u8>(), 16..=64),
        hash_b in proptest::collection::vec(any::<u8>(), 16..=64),
    ) {
        prop_assume!(hash_a != hash_b);
        prop_assume!(slug.len() >= 2);
        let mut s = AuthSchema::new();
        s.insert_account(AccountRow {
            account_id: Uuid::from_u128(1),
            name: "acct".into(),
            deleted_at: None,
        }).unwrap();
        let tenant_id = Uuid::from_u128(2);
        s.insert_tenant(TenantRow {
            tenant_id,
            account_id: Uuid::from_u128(1),
            slug,
            tier: TenantTier::Team,
            deleted_at: None,
        }).unwrap();
        s.insert_pat(PatRow {
            pat_id: Uuid::from_u128(10),
            token_id: token_id.clone(),
            tenant_id,
            issued_to_user: None,
            token_hash: hash_a,
            scopes: vec!["cache:r".into()],
            revoked_at: None,
            revocation_reason: None,
        }).unwrap();
        let err = s.insert_pat(PatRow {
            pat_id: Uuid::from_u128(11),
            token_id,
            tenant_id,
            issued_to_user: None,
            token_hash: hash_b,
            scopes: vec!["cache:r".into()],
            revoked_at: None,
            revocation_reason: None,
        }).unwrap_err();
        prop_assert_eq!(err, SimError::UniqueViolation("pat.token_id"));
    }

    #[test]
    fn tenant_slug_unique(slug in slug_strategy()) {
        prop_assume!(slug.len() >= 2);
        let mut s = AuthSchema::new();
        s.insert_account(AccountRow {
            account_id: Uuid::from_u128(1),
            name: "acct".into(),
            deleted_at: None,
        }).unwrap();
        s.insert_tenant(TenantRow {
            tenant_id: Uuid::from_u128(2),
            account_id: Uuid::from_u128(1),
            slug: slug.clone(),
            tier: TenantTier::Team,
            deleted_at: None,
        }).unwrap();
        let err = s.insert_tenant(TenantRow {
            tenant_id: Uuid::from_u128(3),
            account_id: Uuid::from_u128(1),
            slug,
            tier: TenantTier::Team,
            deleted_at: None,
        }).unwrap_err();
        prop_assert_eq!(err, SimError::UniqueViolation("tenant.slug"));
    }

    #[test]
    fn user_email_hash_unique(
        clerk_a in "clerk_user_[a-z0-9]{8,16}",
        clerk_b in "clerk_user_[a-z0-9]{8,16}",
        email_hash in email_hash_strategy(),
    ) {
        prop_assume!(clerk_a != clerk_b);
        let mut s = AuthSchema::new();
        s.insert_user(UserAccountRow {
            user_id: Uuid::from_u128(0xa1),
            clerk_user_id: clerk_a,
            email_hash,
            deleted_at: None,
        }).unwrap();
        let err = s.insert_user(UserAccountRow {
            user_id: Uuid::from_u128(0xa2),
            clerk_user_id: clerk_b,
            email_hash,
            deleted_at: None,
        }).unwrap_err();
        prop_assert_eq!(err, SimError::UniqueViolation("user_account.email_hash"));
    }

    #[test]
    fn revocation_idempotency_pair_unique(
        slug in slug_strategy(),
        token_id in token_id_strategy(),
        revoked_at in 1u64..u64::MAX / 2,
    ) {
        prop_assume!(slug.len() >= 2);
        let mut s = AuthSchema::new();
        s.insert_account(AccountRow {
            account_id: Uuid::from_u128(1),
            name: "acct".into(),
            deleted_at: None,
        }).unwrap();
        let tenant_id = Uuid::from_u128(2);
        s.insert_tenant(TenantRow {
            tenant_id,
            account_id: Uuid::from_u128(1),
            slug,
            tier: TenantTier::Team,
            deleted_at: None,
        }).unwrap();
        let pat_id = Uuid::from_u128(0xab_cd);
        s.insert_pat(PatRow {
            pat_id,
            token_id,
            tenant_id,
            issued_to_user: None,
            token_hash: b"hash-rev".to_vec(),
            scopes: vec!["cache:r".into()],
            revoked_at: None,
            revocation_reason: None,
        }).unwrap();
        s.insert_revocation_log(RevocationLogRow {
            id: Uuid::from_u128(0xa1),
            pat_id,
            tenant_id,
            revoked_at,
            reason: RevocationReason::UserInitiated,
        }).unwrap();
        // Second insert with same (pat_id, revoked_at) MUST be rejected
        // by the UNIQUE pair constraint — that's the WI-S03-004
        // idempotency primitive.
        let err = s.insert_revocation_log(RevocationLogRow {
            id: Uuid::from_u128(0xa2),
            pat_id,
            tenant_id,
            revoked_at,
            reason: RevocationReason::AdminRevoked,
        }).unwrap_err();
        prop_assert_eq!(
            err,
            SimError::UniqueViolation("revocation_log UNIQUE (pat_id, revoked_at)")
        );
    }
}

// ----------------------------------------------------------------------------
// 2. Cross-tenant isolation invariant (10 000 iter).
// ----------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct TenantSeed {
    tenant_id: Uuid,
    slug: String,
    pat_token_id: String,
    pat_token_hash: Vec<u8>,
    pat_id: Uuid,
}

fn tenant_seed_strategy() -> impl Strategy<Value = TenantSeed> {
    (
        any::<u128>().prop_map(Uuid::from_u128),
        slug_strategy(),
        token_id_strategy(),
        proptest::collection::vec(any::<u8>(), 16..=64),
        any::<u128>().prop_map(Uuid::from_u128),
    )
        .prop_map(
            |(tenant_id, slug, pat_token_id, pat_token_hash, pat_id)| TenantSeed {
                tenant_id,
                slug,
                pat_token_id,
                pat_token_hash,
                pat_id,
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases(10_000)))]

    /// The canonical RLS envelope: for any pair of tenants populated
    /// with PATs, querying tenant_b's view never returns rows owned by
    /// tenant_a. Mirrors the production
    /// `SELECT … WHERE tenant_id = current_setting('app.current_tenant')::uuid`
    /// envelope.
    #[test]
    fn list_pats_never_leaks_cross_tenant(
        a in tenant_seed_strategy(),
        b in tenant_seed_strategy(),
    ) {
        prop_assume!(a.tenant_id != b.tenant_id);
        prop_assume!(a.slug != b.slug);
        prop_assume!(a.pat_token_id != b.pat_token_id);
        prop_assume!(a.pat_token_hash != b.pat_token_hash);
        prop_assume!(a.pat_id != b.pat_id);
        prop_assume!(a.slug.len() >= 2 && b.slug.len() >= 2);

        let mut s = AuthSchema::new();
        s.insert_account(AccountRow {
            account_id: Uuid::from_u128(0xacc),
            name: "acct".into(),
            deleted_at: None,
        }).unwrap();
        s.insert_tenant(TenantRow {
            tenant_id: a.tenant_id,
            account_id: Uuid::from_u128(0xacc),
            slug: a.slug.clone(),
            tier: TenantTier::Team,
            deleted_at: None,
        }).unwrap();
        s.insert_tenant(TenantRow {
            tenant_id: b.tenant_id,
            account_id: Uuid::from_u128(0xacc),
            slug: b.slug.clone(),
            tier: TenantTier::Team,
            deleted_at: None,
        }).unwrap();
        s.insert_pat(PatRow {
            pat_id: a.pat_id,
            token_id: a.pat_token_id.clone(),
            tenant_id: a.tenant_id,
            issued_to_user: None,
            token_hash: a.pat_token_hash,
            scopes: vec!["cache:r".into()],
            revoked_at: None,
            revocation_reason: None,
        }).unwrap();
        s.insert_pat(PatRow {
            pat_id: b.pat_id,
            token_id: b.pat_token_id.clone(),
            tenant_id: b.tenant_id,
            issued_to_user: None,
            token_hash: b.pat_token_hash,
            scopes: vec!["cache:r".into()],
            revoked_at: None,
            revocation_reason: None,
        }).unwrap();

        let view_a = s.list_pats_for_tenant(&a.tenant_id);
        let view_b = s.list_pats_for_tenant(&b.tenant_id);
        prop_assert_eq!(view_a.len(), 1);
        prop_assert_eq!(view_b.len(), 1);
        prop_assert!(view_a.iter().all(|p| p.tenant_id == a.tenant_id));
        prop_assert!(view_b.iter().all(|p| p.tenant_id == b.tenant_id));
        prop_assert_eq!(view_a[0].token_id.clone(), a.pat_token_id);
        prop_assert_eq!(view_b[0].token_id.clone(), b.pat_token_id);
    }

    /// An empty / unset RLS context returns no rows. Mirrors
    /// `current_setting('app.current_tenant', true) IS NULL` →
    /// `tenant_id = NULL::uuid` is `false`.
    #[test]
    fn list_pats_for_unknown_tenant_is_empty(
        a in tenant_seed_strategy(),
        unknown in any::<u128>().prop_map(Uuid::from_u128),
    ) {
        prop_assume!(unknown != a.tenant_id);
        prop_assume!(a.slug.len() >= 2);

        let mut s = AuthSchema::new();
        s.insert_account(AccountRow {
            account_id: Uuid::from_u128(0xacc),
            name: "acct".into(),
            deleted_at: None,
        }).unwrap();
        s.insert_tenant(TenantRow {
            tenant_id: a.tenant_id,
            account_id: Uuid::from_u128(0xacc),
            slug: a.slug.clone(),
            tier: TenantTier::Team,
            deleted_at: None,
        }).unwrap();
        s.insert_pat(PatRow {
            pat_id: a.pat_id,
            token_id: a.pat_token_id,
            tenant_id: a.tenant_id,
            issued_to_user: None,
            token_hash: a.pat_token_hash,
            scopes: vec!["cache:r".into()],
            revoked_at: None,
            revocation_reason: None,
        }).unwrap();
        let view = s.list_pats_for_tenant(&unknown);
        prop_assert!(view.is_empty());
    }
}

// ----------------------------------------------------------------------------
// 3. DSR cascade completeness (smaller iteration count; heavier cases).
// ----------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases(2_048)))]

    /// For every populated account, `dsr_hard_delete_account`:
    ///   - Removes the account row.
    ///   - Removes every tenant whose `account_id` matches.
    ///   - Removes every membership row that referenced any of those
    ///     tenants.
    ///   - Removes every PAT whose `tenant_id` matched any of those
    ///     tenants.
    ///   - Leaves audit-chain pseudonymisation outside this contract
    ///     (the simulator does not model the audit chain — that lives
    ///     in S-09).
    #[test]
    fn dsr_cascade_purges_all_dependents(
        seeds in proptest::collection::vec(tenant_seed_strategy(), 1..=4),
    ) {
        prop_assume!(seeds.iter().map(|s| s.slug.clone()).collect::<HashSet<_>>().len() == seeds.len());
        prop_assume!(seeds.iter().map(|s| s.tenant_id).collect::<HashSet<_>>().len() == seeds.len());
        prop_assume!(seeds.iter().map(|s| s.pat_token_id.clone()).collect::<HashSet<_>>().len() == seeds.len());
        prop_assume!(seeds.iter().map(|s| s.pat_token_hash.clone()).collect::<HashSet<_>>().len() == seeds.len());
        prop_assume!(seeds.iter().map(|s| s.pat_id).collect::<HashSet<_>>().len() == seeds.len());
        prop_assume!(seeds.iter().all(|s| s.slug.len() >= 2));

        let mut s = AuthSchema::new();
        let account_id = Uuid::from_u128(0xacc);
        s.insert_account(AccountRow {
            account_id,
            name: "acct".into(),
            deleted_at: None,
        }).unwrap();
        let user_id = Uuid::from_u128(0x000a_11ce);
        s.insert_user(UserAccountRow {
            user_id,
            clerk_user_id: "clerk_user_alice".into(),
            email_hash: [0xab; 32],
            deleted_at: None,
        }).unwrap();
        for seed in &seeds {
            s.insert_tenant(TenantRow {
                tenant_id: seed.tenant_id,
                account_id,
                slug: seed.slug.clone(),
                tier: TenantTier::Team,
                deleted_at: None,
            }).unwrap();
            s.insert_membership(MembershipRow {
                user_account_id: user_id,
                tenant_id: seed.tenant_id,
                role: Role::Owner,
                deleted_at: None,
            }).unwrap();
            s.insert_pat(PatRow {
                pat_id: seed.pat_id,
                token_id: seed.pat_token_id.clone(),
                tenant_id: seed.tenant_id,
                issued_to_user: Some(user_id),
                token_hash: seed.pat_token_hash.clone(),
                scopes: vec!["cache:r".into()],
                revoked_at: None,
                revocation_reason: None,
            }).unwrap();
        }
        let report = s.dsr_hard_delete_account(&account_id);
        prop_assert_eq!(report.accounts, 1);
        prop_assert_eq!(report.tenants, seeds.len());
        prop_assert_eq!(report.memberships, seeds.len());
        prop_assert_eq!(report.pats, seeds.len());
        prop_assert_eq!(s.tenant_count(), 0);
        prop_assert_eq!(s.pat_count(), 0);
    }
}
