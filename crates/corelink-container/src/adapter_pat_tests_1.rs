/// Fake row source: a `token_id → PatRow` map plus a call counter and
/// an optional forced backend error. Drives the full verification
/// pipeline with real crypto but no network.
#[derive(Default)]
struct FakeLookup {
    rows: HashMap<String, PatRow>,
    calls: AtomicUsize,
    backend_err: Option<String>,
}

impl FakeLookup {
    fn with_row(token_id: &str, row: PatRow) -> Self {
        let mut rows = HashMap::new();
        rows.insert(token_id.to_owned(), row);
        Self {
            rows,
            calls: AtomicUsize::new(0),
            backend_err: None,
        }
    }
    fn backend(err: &str) -> Self {
        Self {
            backend_err: Some(err.to_owned()),
            ..Self::default()
        }
    }
    fn empty() -> Self {
        Self::default()
    }
    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl PatRowLookup for FakeLookup {
    async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(e) = &self.backend_err {
            return Err(e.clone());
        }
        Ok(self.rows.get(token_id).cloned())
    }
}

fn test_key() -> Arc<PatSigningKey> {
    Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
}

/// Mint a real PAT and return `(plaintext, token_id, pat_hash, tenant_string)`.
fn mint_pat(key: &PatSigningKey, tenant: u128, scopes: u64) -> (String, String, String, String) {
    let tenant_id = TenantId(Uuid::from_u128(tenant));
    let (plaintext, pat) = mint(
        PatEnv::Pat,
        tenant_id,
        PrincipalId(Uuid::from_u128(tenant + 1000)),
        PatScopes::from_u64(scopes),
        None,
        key,
        1,
    )
    .unwrap();
    (
        plaintext.into_string(),
        pat.token_id.as_str().to_owned(),
        pat.hash.as_str().to_owned(),
        pat.tenant_id.0.to_string(),
    )
}

fn row(pat_hash: &str, tenant: &str, scope: &str) -> PatRow {
    PatRow {
        tenant_id: tenant.to_owned(),
        pat_hash: pat_hash.to_owned(),
        scope: scope.to_owned(),
        find_only: false,
        runner_job: false,
    }
}

/// [`row`] carrying the 0086 runner-job marker.
fn runner_job_row(pat_hash: &str, tenant: &str, scope: &str) -> PatRow {
    PatRow {
        runner_job: true,
        ..row(pat_hash, tenant, scope)
    }
}

#[tokio::test]
async fn forged_token_rejected_before_d1() {
    let key = test_key();
    let lookup = Arc::new(FakeLookup::empty());
    let verifier = PatVerifier::new(lookup.clone(), key);
    let err = verifier
        .verify("corelink_pat_not-a-real-token")
        .await
        .unwrap_err();
    assert!(matches!(err, VerifyError::InvalidPat));
    assert_eq!(lookup.call_count(), 0, "forged token must not reach D1");
}

#[tokio::test]
async fn wrong_signing_key_rejected_before_d1() {
    let mint_key = test_key();
    let (pt, _tid, hash, tenant) = mint_pat(&mint_key, 7, SCOPE_CACHE_RW);
    let other_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
    let lookup = Arc::new(FakeLookup::with_row(
        "ignored",
        row(&hash, &tenant, "cas:rw"),
    ));
    let verifier = PatVerifier::new(lookup.clone(), other_key);
    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(matches!(err, VerifyError::InvalidPat));
    assert_eq!(lookup.call_count(), 0, "bad HMAC must not reach D1");
}

#[tokio::test]
async fn valid_pat_with_cache_scope_resolves_tenant() {
    let key = test_key();
    let (pt, tid, hash, tenant) = mint_pat(&key, 42, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
    let verifier = PatVerifier::new(lookup.clone(), key);
    let resolved = verifier.verify(&pt).await.unwrap();
    assert_eq!(resolved, tenant);
    assert_eq!(lookup.call_count(), 1);
}

#[tokio::test]
async fn pat_minted_under_prev_key_validates_during_overlap() {
    // Rotation overlap: the verifier's CURRENT key differs from the key
    // the PAT was minted under, but the old key is still in the overlap
    // set — so the PAT must still validate (no instant fleet-wide outage).
    let old_key = test_key();
    let new_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
    let (pt, tid, hash, tenant) = mint_pat(&old_key, 60, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
    // Overlap set = [new (current), old (prev)].
    let verifier =
        PatVerifier::with_key_set(lookup.clone(), vec![(*new_key).clone(), (*old_key).clone()]);
    assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
}

#[tokio::test]
async fn pat_rejected_when_minting_key_not_in_overlap_set() {
    // After the overlap window closes (old key dropped), a PAT minted
    // under the now-retired key must be rejected.
    let old_key = test_key();
    let new_key = Arc::new(PatSigningKey::from_bytes(vec![0x11u8; 32]).unwrap());
    let (pt, tid, hash, tenant) = mint_pat(&old_key, 61, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
    // Overlap set = [new] only — old key retired.
    let verifier = PatVerifier::with_key_set(lookup.clone(), vec![(*new_key).clone()]);
    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(matches!(err, VerifyError::InvalidPat));
    assert_eq!(lookup.call_count(), 0, "bad HMAC must not reach D1");
}

#[tokio::test]
async fn empty_key_set_fails_closed() {
    // No key bound ⇒ nothing verifies (fail-closed).
    let key = test_key();
    let (pt, tid, hash, tenant) = mint_pat(&key, 62, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
    let verifier = PatVerifier::with_key_set(lookup.clone(), vec![]);
    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(matches!(err, VerifyError::InvalidPat));
    assert_eq!(lookup.call_count(), 0, "empty key set rejects pre-D1");
}

#[tokio::test]
async fn admin_scope_grants_cache() {
    let key = test_key();
    let (pt, tid, hash, tenant) = mint_pat(&key, 43, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "admin")));
    let verifier = PatVerifier::new(lookup, key);
    assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
}

#[tokio::test]
async fn unknown_token_id_is_invalid() {
    let key = test_key();
    let (pt, _tid, _hash, _tenant) = mint_pat(&key, 44, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::empty());
    let verifier = PatVerifier::new(lookup.clone(), key);
    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(matches!(err, VerifyError::InvalidPat));
    assert_eq!(lookup.call_count(), 1, "valid HMAC must reach D1");
}

#[tokio::test]
async fn wrong_stored_hash_is_invalid() {
    let key = test_key();
    let (pt, tid, _hash, tenant) = mint_pat(&key, 45, SCOPE_CACHE_RW);
    let (_pt2, _tid2, other_hash, _t2) = mint_pat(&key, 46, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::with_row(
        &tid,
        row(&other_hash, &tenant, "cas:rw"),
    ));
    let verifier = PatVerifier::new(lookup, key);
    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(matches!(err, VerifyError::InvalidPat));
}

#[tokio::test]
async fn corrupt_stored_hash_is_backend_error_not_invalid_pat() {
    let key = test_key();
    let (pt, tid, _hash, tenant) = mint_pat(&key, 48, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::with_row(
        &tid,
        row("not-a-phc-string", &tenant, "cas:rw"),
    ));
    let verifier = PatVerifier::new(lookup, key);
    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(
        matches!(err, VerifyError::Backend(ref message) if message.contains("stored PAT hash invalid")),
        "a corrupt PHC row is infrastructure failure, not a customer credential mismatch: {err:?}"
    );
}

#[tokio::test]
async fn empty_scope_fails_closed() {
    let key = test_key();
    let (pt, tid, hash, tenant) = mint_pat(&key, 47, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "")));
    let verifier = PatVerifier::new(lookup, key);
    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(matches!(err, VerifyError::InvalidPat));
}

#[tokio::test]
async fn read_only_scope_still_resolves() {
    // The verifier gates on "has cache read" (the port carries no op);
    // a cas:r PAT resolves here — per-op write-deny is the route's job.
    let key = test_key();
    let (pt, tid, hash, tenant) = mint_pat(&key, 48, SCOPE_CACHE_R);
    let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:r")));
    let verifier = PatVerifier::new(lookup, key);
    assert_eq!(verifier.verify(&pt).await.unwrap(), tenant);
}

#[tokio::test]
async fn find_only_pat_is_rejected_on_the_adapter_plane() {
    // ADR-0071: a find-only PAT stores the CHECK-safe base `read-only` but is
    // narrowed to find-missing at the Worker header. The adapter verifier
    // authorizes package-manager reads (OCI has NO header gate) directly from
    // the D1 scope, so `requires_cache_read("read-only")` would otherwise grant
    // e.g. `docker pull`. The `find_only` marker MUST fail-CLOSE here even
    // though the base scope reads. (Same possession proof as a normal PAT.)
    let key = test_key();
    let (pt, tid, hash, tenant) = mint_pat(&key, 52, SCOPE_CACHE_R);
    let find_only_row = PatRow {
        tenant_id: tenant.clone(),
        pat_hash: hash,
        scope: "read-only".to_owned(),
        find_only: true,
        runner_job: false,
    };
    let lookup = Arc::new(FakeLookup::with_row(&tid, find_only_row));
    let verifier = PatVerifier::new(lookup, key);
    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(
        matches!(err, VerifyError::InvalidPat),
        "a find-only PAT must be rejected on every adapter surface; got {err:?}"
    );
}

#[tokio::test]
async fn revoked_row_is_invalid_pat() {
    // Soft revocation (migration 0063) is enforced INSIDE the lookup
    // SQL (`AND revoked_at_ms IS NULL`), so a revoked row surfaces to
    // the pipeline exactly like an absent one: `lookup → None`. Model
    // that here (the FakeLookup map simply does not contain the
    // revoked row) and assert the uniform InvalidPat — a revoked PAT
    // must be indistinguishable from an unknown one on the wire.
    let key = test_key();
    let (pt, _tid, _hash, _tenant) = mint_pat(&key, 50, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::empty());
    let verifier = PatVerifier::new(lookup.clone(), key);
    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(matches!(err, VerifyError::InvalidPat));
    assert_eq!(lookup.call_count(), 1, "revocation is decided at D1");
}

#[test]
fn lookup_sql_filters_revoked_and_expired_rows() {
    // The production D1 query must carry BOTH SQL-side liveness
    // filters — losing either one silently re-admits dead tokens.
    assert!(
        PAT_LOOKUP_SQL.contains("AND revoked_at_ms IS NULL"),
        "lookup SQL must exclude soft-revoked rows (migration 0063)"
    );
    assert!(
        PAT_LOOKUP_SQL.contains("expires_ms = 0 OR expires_ms >"),
        "lookup SQL must keep the expiry filter"
    );
    assert!(PAT_LOOKUP_SQL.contains("WHERE token_id = ?1"));
}

#[tokio::test]
async fn backend_error_is_not_invalid_pat() {
    let key = test_key();
    let (pt, _tid, _hash, _tenant) = mint_pat(&key, 49, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::backend("d1 unreachable"));
    let verifier = PatVerifier::new(lookup, key);
    let err = verifier.verify(&pt).await.unwrap_err();
    match err {
        VerifyError::Backend(m) => assert!(m.contains("d1 unreachable")),
        other => panic!("expected Backend, got {other:?}"),
    }
}

/// Finding #2: the Argon2id concurrency gate bounds work — when every
/// permit is held, a hot-path verify (valid HMAC + real D1 row, so it
/// reaches the Argon2id stage) must give up after the bounded wait and
/// surface `Backend("…overloaded…")` rather than piling on another
/// 64-MiB Argon2id allocation. We model "all permits held" by building a
/// verifier with ZERO permits, so the acquire can never succeed and the
/// timeout path is exercised deterministically.
#[tokio::test]
async fn exhausted_argon2_permits_yields_backend_overloaded() {
    let key = test_key();
    let (pt, tid, hash, tenant) = mint_pat(&key, 70, SCOPE_CACHE_RW);
    // A live row so the pipeline gets PAST the HMAC fast-reject and the D1
    // lookup, all the way to the permit acquire that guards Argon2id.
    let lookup = Arc::new(FakeLookup::with_row(&tid, row(&hash, &tenant, "cas:rw")));
    let verifier = PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 0);
    let err = verifier.verify(&pt).await.unwrap_err();
    match err {
        VerifyError::Backend(m) => {
            assert!(m.contains("overloaded"), "expected overloaded, got {m}")
        }
        other => panic!("expected Backend(overloaded), got {other:?}"),
    }
    // The D1 row WAS consulted (we are past the fast-reject) — the gate
    // sits AFTER the lookup, on the expensive stage only.
    assert_eq!(lookup.call_count(), 1);
}

/// The permit gate must NOT consume a permit for a forged token: the HMAC
/// fast-reject fires first, so even with zero permits a forged token is
/// still a plain `InvalidPat` (an attacker without the key cannot drive the
/// verifier into the overloaded path).
#[tokio::test]
async fn forged_token_does_not_touch_argon2_permits() {
    let key = test_key();
    let lookup = Arc::new(FakeLookup::empty());
    let verifier = PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 0);
    let err = verifier
        .verify("corelink_pat_not-a-real-token")
        .await
        .unwrap_err();
    assert!(
        matches!(err, VerifyError::InvalidPat),
        "forged token must fast-reject before the permit gate"
    );
    assert_eq!(lookup.call_count(), 0);
}

// ------------------------------------------------------------------
// INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM — the load shed is symmetric
// across D1 row existence.
//
// The shed used to be asymmetric: the row-FOUND arm shed with
// `Backend` (⇒ 503) and the row-NOT-FOUND dummy-burn arm shed with
// `InvalidPat` (⇒ 401). An attacker who can saturate the (small,
// slow) Argon2id pool could therefore read the status code as a
// ROW-EXISTENCE oracle — strictly more than the latency signal the
// dummy burn exists to hide, because the burn is skipped on a shed
// anyway. These tests pin BOTH halves of the contract:
//
//   under saturation  ⇒ every outcome is Backend("…overloaded…")
//   outside saturation ⇒ every rejection is InvalidPat
//
// The idiom is #1022's: build the verifier with exactly ONE permit,
// HOLD it, and let the acquire time out deterministically. A control
// arm is mandatory — without it the test would pass just as happily
// if the permit had never actually been held.
// ------------------------------------------------------------------

/// THE NEW CASE. A saturated pool + an unknown/absent D1 row must shed as
/// `Backend`, not `InvalidPat`. This is the arm that used to leak row
/// existence through the status code.
#[tokio::test]
async fn saturated_pool_sheds_an_unknown_row_as_backend() {
    let key = test_key();
    // A genuinely-minted PAT (so it clears the HMAC fast-reject) whose
    // token_id has NO row — the dummy-burn arm.
    let (pt, _tid, _hash, _tenant) = mint_pat(&key, 90, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::empty());
    let verifier = PatVerifier::with_key_set_and_permits(lookup.clone(), vec![(*key).clone()], 1);

    // Drain the one and only permit and keep it drained.
    let _held = Arc::clone(&verifier.argon2_permits)
        .acquire_owned()
        .await
        .unwrap();

    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(
        matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
        "a shed on the row-NOT-FOUND arm must be Backend(overloaded), got {err:?}"
    );
    assert_eq!(lookup.call_count(), 1, "the shed sits AFTER the D1 lookup");
}

/// The per-tenant sub-cap arm of the same shed. The dummy burn is routed
/// through ONE shared synthetic bucket; saturating THAT bucket (with global
/// headroom to spare) must also shed as `Backend`, never `InvalidPat`.
#[tokio::test]
async fn saturated_dummy_burn_bucket_sheds_an_unknown_row_as_backend() {
    let key = test_key();
    let (pt, _tid, _hash, _tenant) = mint_pat(&key, 91, SCOPE_CACHE_RW);
    let lookup = Arc::new(FakeLookup::empty());
    // Global 8 (ample) so a rejection can ONLY come from the per-tenant
    // tier; synthetic bucket sub-cap 1 so holding one permit saturates it.
    let verifier = PatVerifier::with_key_set_and_permits_per_tenant(
        lookup.clone(),
        vec![(*key).clone()],
        8,
        1,
    );
    let bucket = verifier
        .per_tenant_semaphore(UNKNOWN_TOKEN_BUCKET)
        .expect("synthetic bucket semaphore");
    let _held = Arc::clone(&bucket)
        .try_acquire_owned()
        .expect("bucket permit");
    assert_eq!(bucket.available_permits(), 0, "burn bucket saturated");

    let err = verifier.verify(&pt).await.unwrap_err();
    assert!(
        matches!(err, VerifyError::Backend(ref m) if m.contains("overloaded")),
        "a sub-cap shed on the row-NOT-FOUND arm must be Backend(overloaded), got {err:?}"
    );
}

/// The row-FOUND half of the same contract — existing behaviour, pinned
/// here in the one-permit idiom so a future refactor cannot quietly
/// re-asymmetrise the pair by changing only this side.
include!("adapter_pat_tests_1/part-01.rs");
