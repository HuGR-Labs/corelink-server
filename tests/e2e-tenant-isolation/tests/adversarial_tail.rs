use e2e_tenant_isolation::{
    AuditCapture, AuditChain, AuditQueryEngine, DenyKind, DsrIntake, HierarchicalQuotaStore,
    KvReplicatedPatStore, MultipartBroker, RegionRouter, StripeWebhookLedger, TenantCtx,
};

/// Scenario 18 — Audit chain leaf forge: Tenant A constructs a forged
/// leaf claiming it belongs to Tenant B's chain. Chain verify under
/// B's root MUST reject (no two chains share a root; membership
/// check is constant-time over the claimed tenant's chain).
///
/// STRIDE: `STRIDE-corelink-audit-chain.md` non-forgeable row,
/// INV-AUDIT-CHAIN-NON-FORGEABLE.
#[test]
fn s18_audit_chain_leaf_forge_rejected() {
    let audit = AuditCapture::new();
    let chain = AuditChain::new(audit.clone());
    let a = TenantCtx::tenant_a().unwrap();
    let b = TenantCtx::tenant_b().unwrap();

    chain.append(a.tenant_id(), b"leaf_a_1".to_vec()).unwrap();
    chain.append(b.tenant_id(), b"leaf_b_1".to_vec()).unwrap();

    // Sanity: own-chain verify ok.
    chain
        .verify(a.tenant_id(), a.tenant_id(), b"leaf_a_1")
        .unwrap();
    chain
        .verify(b.tenant_id(), b.tenant_id(), b"leaf_b_1")
        .unwrap();

    // Adversarial: A constructs a forged leaf claiming B's chain.
    let forged = b"leaf_a_FORGED_AS_B".to_vec();
    let r = chain.verify(a.tenant_id(), b.tenant_id(), &forged);
    assert!(matches!(
        r,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::AuditChainForge
        ))
    ));
    assert_eq!(audit.count(), 1);
    let last = audit.last_deny().unwrap();
    assert_eq!(last.requester, a.tenant_id());
    assert_eq!(last.resource_owner, b.tenant_id());

    // Adversarial: A presents A's *own* leaf bytes against B's chain —
    // still rejected (leaf bytes don't belong to B's chain).
    let r2 = chain.verify(a.tenant_id(), b.tenant_id(), b"leaf_a_1");
    assert!(matches!(
        r2,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::AuditChainForge
        ))
    ));
    assert_eq!(audit.count(), 2);
}

/// Scenario 19 — Cross-region replay: a request originally crafted
/// for the BR region is replayed against the US region with the same
/// tenant id. The region router MUST consult the tenant's residency
/// pin and reject when `received_region != home_region`.
///
/// STRIDE: `STRIDE-corelink-residency.md` cross-region replay row,
/// INV-RESIDENCY-REGION-PINNED, FM-RESIDENCY-001.
#[test]
fn s19_cross_region_replay_residency_enforced() {
    let audit = AuditCapture::new();
    let router = RegionRouter::new(audit.clone());
    let a = TenantCtx::tenant_a().unwrap();

    router.pin(a.tenant_id(), "br-sao").unwrap();

    // Legit: request received in br-sao.
    router.route(a.tenant_id(), "br-sao").unwrap();
    assert_eq!(audit.count(), 0);

    // Adversarial: same tenant id, replayed against us-east.
    let r = router.route(a.tenant_id(), "us-east");
    assert!(matches!(
        r,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::RegionResidencyViolation
        ))
    ));
    assert_eq!(audit.count(), 1);
    let deny = audit.last_deny().unwrap();
    assert_eq!(deny.kind, DenyKind::RegionResidencyViolation);
}

/// Scenario 20 — Cross-tenant DSR submission: Tenant A submits a DSR
/// for Tenant B's principal email. The DSR intake MUST require
/// auth-context match (requester tenant == target tenant).
///
/// STRIDE: `STRIDE-corelink-dsr.md` §2.1,
/// INV-DSR-TENANT-CONTEXT-MATCH.
#[test]
fn s20_dsr_cross_tenant_submission_rejected() {
    let audit = AuditCapture::new();
    let dsr = DsrIntake::new(audit.clone());
    let a = TenantCtx::tenant_a().unwrap();
    let b = TenantCtx::tenant_b().unwrap();

    // Same-tenant DSR is allowed.
    dsr.submit(a.tenant_id(), a.tenant_id(), "alice@example.com")
        .unwrap();
    assert_eq!(audit.count(), 0);

    // Cross-tenant DSR: A submits for B's email → rejected.
    let r = dsr.submit(a.tenant_id(), b.tenant_id(), "bob@example.com");
    assert!(matches!(
        r,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::DsrAuthContextMismatch
        ))
    ));
    assert_eq!(audit.count(), 1);
    let deny = audit.last_deny().unwrap();
    assert_eq!(deny.requester, a.tenant_id());
    assert_eq!(deny.resource_owner, b.tenant_id());
}

/// Scenario 21 — Parent/child quota inheritance: two child tenants
/// under the same parent. Exhausting child_A MUST NOT affect
/// child_B's quota window (sibling isolation).
///
/// STRIDE: quota hierarchy row,
/// INV-QUOTA-SIBLING-NON-INHERITED.
#[test]
fn s21_quota_inheritance_siblings_isolated() {
    let audit = AuditCapture::new();
    let quota = HierarchicalQuotaStore::new(audit.clone());
    let parent = TenantCtx::tenant_a().unwrap().tenant_id();
    let child_a = uuid::Uuid::from_u128(0xCCCC_AAAA);
    let child_b = uuid::Uuid::from_u128(0xCCCC_BBBB);

    quota.register_child(child_a, parent, 3).unwrap();
    quota.register_child(child_b, parent, 3).unwrap();

    // child_a exhausts.
    for _ in 0..3 {
        quota.try_consume(child_a).unwrap();
    }
    let r = quota.try_consume(child_a);
    assert!(matches!(
        r,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::QuotaInheritanceLeak
        ))
    ));
    assert_eq!(audit.count(), 1);
    // child_b is unaffected.
    assert_eq!(quota.used(child_b), 0);
    for _ in 0..3 {
        quota.try_consume(child_b).unwrap();
    }
    // Only one deny — child_a's.
    assert_eq!(audit.count(), 1);
}

/// Scenario 22 — R2 multipart upload forge: Tenant A creates a
/// multipart upload (bound to A's prefix). Tenant B forges the
/// `upload_id` and attempts to upload parts. The broker MUST reject
/// by tenant prefix at part-upload time.
///
/// STRIDE: `STRIDE-corelink-cas.md` multipart row,
/// INV-MULTIPART-UPLOAD-TENANT-BOUND, FM-CAS-007.
#[test]
fn s22_multipart_upload_cross_tenant_forge_rejected() {
    let audit = AuditCapture::new();
    let broker = MultipartBroker::new(audit.clone());
    let a = TenantCtx::tenant_a().unwrap();
    let b = TenantCtx::tenant_b().unwrap();

    broker
        .create(a.tenant_id(), a.prefix(), "upload-xyz")
        .unwrap();
    // Owner uploads part — ok.
    broker
        .upload_part(a.tenant_id(), a.prefix(), "upload-xyz", b"part1".to_vec())
        .unwrap();
    assert_eq!(audit.count(), 0);

    // Tenant B forges the upload_id and tries to inject a part.
    let r = broker.upload_part(b.tenant_id(), b.prefix(), "upload-xyz", b"evil".to_vec());
    assert!(matches!(
        r,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::MultipartUploadForge
        ))
    ));
    assert_eq!(audit.count(), 1);
    let deny = audit.last_deny().unwrap();
    assert_eq!(deny.requester, b.tenant_id());
    assert_eq!(deny.resource_owner, a.tenant_id());

    // Tenant B also tries to abort A's upload — same rejection.
    let r2 = broker.abort(b.tenant_id(), "upload-xyz");
    assert!(matches!(
        r2,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::MultipartUploadForge
        ))
    ));
    assert_eq!(audit.count(), 2);
}

/// Scenario 23 — Stripe webhook cross-account spoofing: an attacker
/// crafts a webhook with a Stripe `event_id` already consumed by
/// another customer account (tenant). The ledger MUST reject the
/// cross-tenant replay (extends Scenario 11 with a distinct
/// adversarial framing: forged `event_id` re-use across accounts).
///
/// STRIDE: `STRIDE-corelink-billing.md` webhook-spoof row,
/// INV-STRIPE-WEBHOOK-IDEMPOTENT-CROSS-TENANT.
#[test]
fn s23_stripe_webhook_cross_account_spoof_rejected() {
    let audit = AuditCapture::new();
    let ledger = StripeWebhookLedger::new(audit.clone());
    let a = TenantCtx::tenant_a().unwrap();
    let b = TenantCtx::tenant_b().unwrap();

    // Tenant A's webhook is consumed.
    ledger.process(a.tenant_id(), "evt_spoof_001").unwrap();

    // Tenant B spoofs the same event_id — rejected.
    let r = ledger.process(b.tenant_id(), "evt_spoof_001");
    assert!(matches!(
        r,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::StripeReplay
        ))
    ));
    assert_eq!(audit.count(), 1);

    // Try a SECOND spoof attempt with a DIFFERENT event_id but B
    // re-uses A's id pattern — independently rejected as a new
    // cross-tenant replay.
    ledger.process(a.tenant_id(), "evt_spoof_002").unwrap();
    let r2 = ledger.process(b.tenant_id(), "evt_spoof_002");
    assert!(matches!(
        r2,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::StripeReplay
        ))
    ));
    assert_eq!(audit.count(), 2);
    let deny = audit.last_deny().unwrap();
    assert_eq!(deny.requester, b.tenant_id());
    assert_eq!(deny.resource_owner, a.tenant_id());
}

/// Scenario 24 — KV propagation under partition: Tenant A's PAT is
/// revoked in the home region during a network partition between
/// home and the remote replica. Reads on the remote side MUST
/// fail-CLOSED (cannot prove the token is live) — no stale-allow.
///
/// STRIDE: `STRIDE-corelink-kv-replication.md` partition row,
/// INV-KV-REPLICATION-FAIL-CLOSED, FM-AUTH-013.
#[test]
fn s24_kv_partition_pat_revoke_fail_closed() {
    let audit = AuditCapture::new();
    let store = KvReplicatedPatStore::new(audit.clone());
    let a = TenantCtx::tenant_a().unwrap();

    store.mint("pat_P", a.tenant_id()).unwrap();
    // Pre-partition, pre-revoke: ok in remote.
    store
        .authorize_remote("pat_P", a.tenant_id(), false)
        .unwrap();
    assert_eq!(audit.count(), 0);

    // Home revokes during partition (remote does not yet know).
    store.revoke_home("pat_P").unwrap();

    // Read on remote DURING partition: cannot consult home, no local
    // replication yet → fail-CLOSED.
    let r = store.authorize_remote("pat_P", a.tenant_id(), true);
    assert!(matches!(
        r,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::KvReplicationLag
        ))
    ));
    assert_eq!(audit.count(), 1);

    // Read on remote AFTER partition heals (no partition flag), home
    // still has revoke → also rejected (home authoritative).
    let r2 = store.authorize_remote("pat_P", a.tenant_id(), false);
    assert!(matches!(
        r2,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::KvReplicationLag
        ))
    ));
    assert_eq!(audit.count(), 2);

    // After replication, local replica also knows: still rejected.
    store.replicate_revoke("pat_P").unwrap();
    let r3 = store.authorize_remote("pat_P", a.tenant_id(), false);
    assert!(matches!(
        r3,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::KvReplicationLag
        ))
    ));
    assert_eq!(audit.count(), 3);
}

/// Scenario 25 — Audit query injection: an attacker submits a crafted
/// filter parameter (`tenant_id=B`) while authenticated as A. The
/// query layer MUST enforce tenant scoping pre-filter — a
/// caller-supplied `tenant_id` that disagrees with the JWT-bound
/// tenant is an injection attempt and is rejected.
///
/// STRIDE: `STRIDE-corelink-audit-chain.md` query-injection row,
/// INV-AUDIT-QUERY-TENANT-SCOPED.
#[test]
fn s25_audit_query_injection_rejected() {
    let audit = AuditCapture::new();
    let q = AuditQueryEngine::new(audit.clone());
    let a = TenantCtx::tenant_a().unwrap();
    let b = TenantCtx::tenant_b().unwrap();

    q.seed(a.tenant_id(), "row_A_1").unwrap();
    q.seed(a.tenant_id(), "row_A_2").unwrap();
    q.seed(b.tenant_id(), "row_B_1").unwrap();

    // No filter: tenant A sees only A's rows.
    let rows = q.query(a.tenant_id(), None).unwrap();
    assert_eq!(rows, vec!["row_A_1".to_string(), "row_A_2".to_string()]);

    // Matching filter: also ok (A asks for tenant=A).
    let rows2 = q.query(a.tenant_id(), Some(a.tenant_id())).unwrap();
    assert_eq!(rows2.len(), 2);
    assert_eq!(audit.count(), 0);

    // Adversarial: A authenticates but supplies filter=B → rejected.
    let r = q.query(a.tenant_id(), Some(b.tenant_id()));
    assert!(matches!(
        r,
        Err(e2e_tenant_isolation::fakes::FakeError::Deny(
            DenyKind::AuditQueryInjection
        ))
    ));
    assert_eq!(audit.count(), 1);
    let deny = audit.last_deny().unwrap();
    assert_eq!(deny.requester, a.tenant_id());
    assert_eq!(deny.resource_owner, b.tenant_id());
}
