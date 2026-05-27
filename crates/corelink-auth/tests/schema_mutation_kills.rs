//! Mutation-kill regression suite — DEBT-008 wave-22 (`corelink-auth-schema`).
//!
//! Pin behaviour for every surviving mutant from the empirical
//! `cargo mutants -p corelink-auth-schema` sweep on 2026-05-16. Test
//! names map 1:1 to the mutant they kill (see comments).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use uuid::Uuid;

use corelink_auth::schema::email_hash::{compute_email_hash, EmailHashKey};
use corelink_auth::schema::pseudonymize::{pseudonymize_account_id, pseudonymize_user_id};
use corelink_auth::schema::sim::{
    AccountRow, AuthSchema, PatRow, RevocationLogRow, RevocationReason, TenantRow, TenantTier,
    UserAccountRow,
};

fn uuid(byte: u8) -> Uuid {
    Uuid::from_bytes([byte; 16])
}

fn account_row(byte: u8) -> AccountRow {
    AccountRow {
        account_id: uuid(byte),
        name: format!("Account {byte}"),
        deleted_at: None,
    }
}

fn tenant_row(byte: u8, account: u8, slug: &str) -> TenantRow {
    TenantRow {
        tenant_id: uuid(byte),
        account_id: uuid(account),
        slug: slug.into(),
        tier: TenantTier::Solo,
        deleted_at: None,
    }
}

fn user_row(byte: u8, clerk: &str, email_hash: [u8; 32]) -> UserAccountRow {
    UserAccountRow {
        user_id: uuid(byte),
        clerk_user_id: clerk.into(),
        email_hash,
        deleted_at: None,
    }
}

fn pat_row(byte: u8, tenant: u8, token_id: &str, token_hash: Vec<u8>) -> PatRow {
    PatRow {
        pat_id: uuid(byte),
        token_id: token_id.into(),
        tenant_id: uuid(tenant),
        issued_to_user: None,
        token_hash,
        scopes: vec!["cas:read".into()],
        revoked_at: None,
        revocation_reason: None,
    }
}

// --------------------------------------------------------------------
// email_hash.rs:151  canonicalise_email -> "" | "xyzzy"
// --------------------------------------------------------------------

/// Kills `email_hash.rs:151 canonicalise_email -> String::new()` and
/// `-> "xyzzy".into()`.
///
/// Canonicalisation is lower-case + trim. We hash three formally
/// different inputs that canonicalise to the SAME value and assert
/// equal output; under the const-return mutants the function would
/// also collapse all inputs to one constant, but our negative
/// assertion (`hash("alice") != hash("bob")`) kills it because both
/// mutants force ALL inputs to the same hash.
#[test]
fn canonicalise_email_normalises_case_and_whitespace() {
    let key = EmailHashKey::from_bytes([0x11; 32]);
    // Same underlying address, three case/whitespace variants.
    let h1 = compute_email_hash(&key, "alice@acme.com");
    let h2 = compute_email_hash(&key, "  ALICE@Acme.COM  ");
    let h3 = compute_email_hash(&key, "Alice@ACME.com");
    assert_eq!(h1, h2, "trim + lowercase must canonicalise");
    assert_eq!(h2, h3, "case canonicalisation");
    // Different addresses must differ — kills the const-return
    // mutants (both force *every* input to one fixed hash).
    let bob = compute_email_hash(&key, "bob@acme.com");
    assert_ne!(
        h1, bob,
        "distinct emails must hash differently (kills const-return canonicalise mutants)"
    );
}

// --------------------------------------------------------------------
// pseudonymize.rs:40  pseudonymize_user_id -> "" | "xyzzy"
// --------------------------------------------------------------------

/// Kills `pseudonymize.rs:40 pseudonymize_user_id -> String::new()` and
/// `-> "xyzzy".into()`.
///
/// The user-id pseudonym is HMAC-SHA256 prefix-tagged with `b"user:"`
/// and truncated to 16 bytes (32 hex chars). Length must be 32 and
/// it must differ from the account-id pseudonym for the same UUID
/// (domain-separation).
#[test]
fn pseudonymize_user_id_is_32_hex_chars_and_domain_separated() {
    let key = [0x22u8; 32];
    let id = uuid(0xAB);
    let u_pseudo = pseudonymize_user_id(&key, &id);
    let a_pseudo = pseudonymize_account_id(&key, &id);
    assert_eq!(u_pseudo.len(), 32, "user pseudonym is 32 hex chars");
    assert!(
        u_pseudo.chars().all(|c| c.is_ascii_hexdigit()),
        "user pseudonym hex"
    );
    // Domain-separation: same UUID under different prefixes must
    // produce different outputs. The mutants collapse to a constant
    // string ("" or "xyzzy") that is identical to its account-pair.
    assert_ne!(
        u_pseudo, a_pseudo,
        "user-vs-account domain separation (kills const-return user-id mutants)"
    );
    // Different keys yield different outputs (kills the empty/"xyzzy"
    // const-return: those are key-invariant).
    let other_key = [0x55u8; 32];
    let u_other = pseudonymize_user_id(&other_key, &id);
    assert_ne!(u_pseudo, u_other);
}

// --------------------------------------------------------------------
// sim.rs:220  insert_account || -> &&,  > -> ==
// --------------------------------------------------------------------

/// Kills `sim.rs:220:32 || with &&` AND `sim.rs:220:50 > with ==` in
/// `AuthSchema::insert_account`.
///
/// Length guard is `name.is_empty() || name.len() > 200`. With `||→&&`
/// the empty-name branch only fires when ALSO over 200 chars — so an
/// empty-name insert succeeds. With `>→==` the over-200 branch only
/// fires at exactly 200 chars — so a 201-char name succeeds.
#[test]
fn insert_account_rejects_empty_and_over_200_name() {
    let mut s = AuthSchema::new();
    // Empty name must error (kills ||→&& mutant).
    let mut empty = account_row(1);
    empty.name = String::new();
    let err = s.insert_account(empty).expect_err("empty name rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("account.name") || msg.contains("check"),
        "expected check_violation on empty name, got {msg}"
    );

    // 201-char name must error (kills >→== mutant; 201 > 200 is true,
    // 201 == 200 is false).
    let mut over = account_row(2);
    over.name = "a".repeat(201);
    let err = s.insert_account(over).expect_err("over-200 rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("account.name") || msg.contains("check"),
        "expected check_violation on 201-char name, got {msg}"
    );

    // 200-char name is the boundary and must SUCCEED (proves the
    // guard is `>` not `>=`; if the mutant had been `>=` we'd reject
    // 200 here, killing it. cargo-mutants does not generate that
    // mutant but we keep the assertion for symmetry).
    let mut ok = account_row(3);
    ok.name = "a".repeat(200);
    s.insert_account(ok).expect("200-char name accepted");
}

// --------------------------------------------------------------------
// sim.rs:233  get_account -> None
// sim.rs:266  get_tenant -> None
// --------------------------------------------------------------------

/// Kills `sim.rs:233 get_account -> None`, `sim.rs:266 get_tenant -> None`,
/// AND `sim.rs:299 lookup_user_by_email_hash -> None`,
/// AND `sim.rs:381 lookup_pat_by_token_id -> None` (4 const-return
/// mutants — pin Some-returns on the happy path).
#[test]
fn lookups_return_some_for_existing_rows() {
    let mut s = AuthSchema::new();
    let acct = account_row(0x10);
    let acct_id = acct.account_id;
    s.insert_account(acct).expect("insert account");
    assert!(
        s.get_account(&acct_id).is_some(),
        "get_account must return Some for inserted row"
    );

    let tnt = tenant_row(0x11, 0x10, "team-a");
    let tnt_id = tnt.tenant_id;
    s.insert_tenant(tnt).expect("insert tenant");
    assert!(
        s.get_tenant(&tnt_id).is_some(),
        "get_tenant must return Some for inserted row"
    );

    // User + email-hash lookup
    let key = EmailHashKey::from_bytes([0x33; 32]);
    let email_hash = compute_email_hash(&key, "lookup@acme.com");
    let user = user_row(0x12, "clerk_xyz", email_hash);
    s.insert_user(user).expect("insert user");
    let found = s
        .lookup_user_by_email_hash(&email_hash)
        .expect("lookup_user_by_email_hash must return Some");
    assert_eq!(found.clerk_user_id, "clerk_xyz");

    // PAT lookup by token_id
    let pat = pat_row(0x13, 0x11, "tokid_0123456789", vec![0xAA; 16]);
    s.insert_pat(pat).expect("insert pat");
    let found_pat = s
        .lookup_pat_by_token_id("tokid_0123456789")
        .expect("lookup_pat_by_token_id must return Some");
    assert_eq!(found_pat.token_id, "tokid_0123456789");
}

/// Kills `sim.rs:299 lookup_user_by_email_hash -> None` for the
/// negative branch + `sim.rs:301 && -> ||` + `sim.rs:301 == -> !=`.
///
/// - `&& -> ||`: filter is `deleted_at.is_none() && email_hash == X`.
///   Replacing `&&` with `||` makes soft-deleted users still match by
///   `is_none()`-false-OR-anything. Insert a user, soft-delete by
///   inserting again with a different ID? We can't soft-delete the
///   sim, but we CAN exercise the `==` filter: a hash that does not
///   match must return None (kills the `!=` mutant which returns the
///   wrong user).
#[test]
fn lookup_user_by_email_hash_filters_exact_match() {
    let mut s = AuthSchema::new();
    let key = EmailHashKey::from_bytes([0x44; 32]);
    let alice = compute_email_hash(&key, "alice@acme.com");
    let bob = compute_email_hash(&key, "bob@acme.com");
    assert_ne!(alice, bob);

    s.insert_user(user_row(0x20, "u_alice", alice))
        .expect("insert alice");
    s.insert_user(user_row(0x21, "u_bob", bob))
        .expect("insert bob");

    // Looking up alice must return alice, NOT bob (kills `== -> !=`
    // which would return bob's row when asked for alice).
    let found = s
        .lookup_user_by_email_hash(&alice)
        .expect("alice found");
    assert_eq!(found.clerk_user_id, "u_alice");
    let found_bob = s.lookup_user_by_email_hash(&bob).expect("bob found");
    assert_eq!(found_bob.clerk_user_id, "u_bob");

    // A random hash that matches no user → None (kills the const-Some
    // path if any).
    let missing = [0xFFu8; 32];
    assert!(
        s.lookup_user_by_email_hash(&missing).is_none(),
        "non-matching hash must return None"
    );
}

// --------------------------------------------------------------------
// sim.rs:275  insert_user || -> &&,  > -> ==
// --------------------------------------------------------------------

/// Kills `sim.rs:275:41 || -> &&` and `sim.rs:275:68 > -> ==`.
///
/// Same shape as `insert_account` but for `user_account.clerk_user_id`.
#[test]
fn insert_user_rejects_empty_and_over_200_clerk_id() {
    let mut s = AuthSchema::new();
    let email = [0x77u8; 32];
    let mut empty = user_row(0x30, "", email);
    empty.clerk_user_id = String::new();
    let err = s.insert_user(empty).expect_err("empty clerk rejected");
    let msg = format!("{err}");
    assert!(msg.contains("clerk_user_id") || msg.contains("check"));

    let mut over = user_row(0x31, &"x".repeat(201), [0x78u8; 32]);
    over.clerk_user_id = "x".repeat(201);
    let err = s.insert_user(over).expect_err("over-200 clerk rejected");
    let msg = format!("{err}");
    assert!(msg.contains("clerk_user_id") || msg.contains("check"));
}

// --------------------------------------------------------------------
// sim.rs:381  lookup_pat_by_token_id  == -> !=
// --------------------------------------------------------------------

/// Kills `sim.rs:381:48 == -> !=`.
///
/// Two PATs with distinct token_ids; lookup by one must return
/// THAT one, not the other.
#[test]
fn lookup_pat_by_token_id_matches_exactly() {
    let mut s = AuthSchema::new();
    s.insert_account(account_row(0x40)).expect("acct");
    s.insert_tenant(tenant_row(0x41, 0x40, "team-z")).expect("tnt");
    s.insert_pat(pat_row(0x42, 0x41, "tok_aaaa00000000", vec![0xA1; 16]))
        .expect("pat a");
    s.insert_pat(pat_row(0x43, 0x41, "tok_bbbb00000000", vec![0xB2; 16]))
        .expect("pat b");

    let a = s
        .lookup_pat_by_token_id("tok_aaaa00000000")
        .expect("found a");
    assert_eq!(a.token_id, "tok_aaaa00000000");
    let b = s
        .lookup_pat_by_token_id("tok_bbbb00000000")
        .expect("found b");
    assert_eq!(b.token_id, "tok_bbbb00000000");

    assert!(
        s.lookup_pat_by_token_id("tok_zzzzzzzzzzzz").is_none(),
        "unknown token_id must be None"
    );
}

// --------------------------------------------------------------------
// sim.rs:405  revoke_pat -> Ok(0) | Ok(1)
// --------------------------------------------------------------------

/// Kills `sim.rs:405 revoke_pat -> Ok(0)` and `-> Ok(1)`.
///
/// `revoke_pat` returns the supplied `revoked_at` timestamp on first
/// call, the existing timestamp on idempotent retry, and a structured
/// error if the PAT does not exist. Use a non-{0,1} timestamp so both
/// const-return mutants surface.
#[test]
fn revoke_pat_returns_supplied_timestamp_and_is_idempotent() {
    let mut s = AuthSchema::new();
    s.insert_account(account_row(0x50)).expect("acct");
    s.insert_tenant(tenant_row(0x51, 0x50, "team-y")).expect("tnt");
    let pat = pat_row(0x52, 0x51, "tok_x00000000000", vec![0xC3; 16]);
    let pat_id = pat.pat_id;
    s.insert_pat(pat).expect("pat");

    // First revoke returns the supplied timestamp (12345) → kills
    // Ok(0) and Ok(1) const-returns.
    let ts1 = s
        .revoke_pat(&pat_id, 12_345, RevocationReason::UserInitiated)
        .expect("revoke ok");
    assert_eq!(ts1, 12_345);

    // Idempotent retry with a different timestamp returns the FIRST
    // timestamp (12345), not the new one — pins the
    // `if let Some(existing) = row.revoked_at` early-return.
    let ts2 = s
        .revoke_pat(&pat_id, 99_999, RevocationReason::AdminRevoked)
        .expect("idempotent");
    assert_eq!(
        ts2, 12_345,
        "idempotent revoke must return the FIRST timestamp, not the retry's"
    );

    // Unknown pat returns Err — never returns Ok(0)/Ok(1).
    let unknown = uuid(0x99);
    let err = s
        .revoke_pat(&unknown, 1, RevocationReason::Rotation)
        .expect_err("unknown pat must error");
    assert!(format!("{err}").contains("pat.pat_id"));
}

// --------------------------------------------------------------------
// sim.rs:504  tenant_count -> 0
// sim.rs:510  pat_count -> 0
// sim.rs:516  revocation_count -> 1
// --------------------------------------------------------------------

/// Kills `sim.rs:504 tenant_count -> 0`, `sim.rs:510 pat_count -> 0`,
/// `sim.rs:516 revocation_count -> 1`.
///
/// Counts must reflect the BTreeMap len after inserts. We use distinct
/// non-{0,1} cardinalities so each const-return is killed.
#[test]
fn counts_reflect_inserted_rows() {
    let mut s = AuthSchema::new();
    assert_eq!(s.tenant_count(), 0);
    assert_eq!(s.pat_count(), 0);
    assert_eq!(s.revocation_count(), 0);

    s.insert_account(account_row(0x60)).expect("acct");
    // 3 tenants (kills Ok(0)).
    s.insert_tenant(tenant_row(0x61, 0x60, "team-aa")).expect("t1");
    s.insert_tenant(tenant_row(0x62, 0x60, "team-bb")).expect("t2");
    s.insert_tenant(tenant_row(0x63, 0x60, "team-cc")).expect("t3");
    assert_eq!(s.tenant_count(), 3, "kills tenant_count -> 0");

    // 2 pats (kills pat_count -> 0).
    s.insert_pat(pat_row(0x64, 0x61, "tok_p1_aaaaaaaaa", vec![0xD1; 16]))
        .expect("p1");
    s.insert_pat(pat_row(0x65, 0x62, "tok_p2_aaaaaaaaa", vec![0xD2; 16]))
        .expect("p2");
    assert_eq!(s.pat_count(), 2, "kills pat_count -> 0");

    // 0 revocations initially → kills revocation_count -> 1.
    assert_eq!(s.revocation_count(), 0, "kills revocation_count -> 1");

    // After inserting a row → 1 (still kills -> 0 if it existed; and
    // a second insert → 2 to cement non-constness).
    s.insert_revocation_log(RevocationLogRow {
        id: uuid(0x66),
        pat_id: uuid(0x64),
        tenant_id: uuid(0x61),
        revoked_at: 1,
        reason: RevocationReason::UserInitiated,
    })
    .expect("rev 1");
    assert_eq!(s.revocation_count(), 1);
    s.insert_revocation_log(RevocationLogRow {
        id: uuid(0x67),
        pat_id: uuid(0x65),
        tenant_id: uuid(0x62),
        revoked_at: 2,
        reason: RevocationReason::AdminRevoked,
    })
    .expect("rev 2");
    assert_eq!(s.revocation_count(), 2);
}

// --------------------------------------------------------------------
// sim.rs:549  is_valid_slug  || -> &&
// --------------------------------------------------------------------

/// Kills `sim.rs:549:31 || -> &&` in `is_valid_slug`.
///
/// `is_valid_slug` rejects when `!is_alnum_ascii(first) ||
/// !is_alnum_ascii(last)`. Replacing `||` with `&&` requires BOTH
/// endpoints to be non-alnum before rejecting — so a slug starting
/// with `-` (non-alnum first, alnum last) is accepted by the mutant.
/// Drive the rejection through `insert_tenant`, which surfaces a
/// `CheckViolation("tenant.slug: …")`.
#[test]
fn is_valid_slug_rejects_leading_or_trailing_dash() {
    let mut s = AuthSchema::new();
    s.insert_account(account_row(0x70)).expect("acct");

    // Leading-dash slug: alnum-last, non-alnum-first. Kills ||→&&
    // (the && mutant accepts this).
    let bad_leading = tenant_row(0x71, 0x70, "-leadingdash");
    let err = s.insert_tenant(bad_leading).expect_err("leading - rejected");
    assert!(format!("{err}").contains("tenant.slug"));

    // Trailing-dash slug: alnum-first, non-alnum-last. Kills ||→&&
    // (the && mutant accepts this too).
    let bad_trailing = tenant_row(0x72, 0x70, "trailingdash-");
    let err = s
        .insert_tenant(bad_trailing)
        .expect_err("trailing - rejected");
    assert!(format!("{err}").contains("tenant.slug"));

    // Sanity: middle dash is valid (proves the `b'-'` branch in the
    // body allows interior dashes).
    let good = tenant_row(0x73, 0x70, "middle-dash");
    s.insert_tenant(good).expect("middle dash accepted");
}
