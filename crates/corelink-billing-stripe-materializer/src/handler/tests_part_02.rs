#[test]
fn subscription_updated_non_granting_status_does_not_grant_active() {
    // Regression (money-path webhook status gate): a
    // customer.subscription.updated carrying a RECOGNIZED plan but a
    // NON-granting status (past_due / unpaid) MUST NOT (re-)grant the
    // canonical 'active' entitlement. `active` still does.
    for non_granting in ["past_due", "unpaid"] {
        let (handler, d1, audit) = fixture();
        // Tenant starts on a non-active baseline (free), so a granted
        // 'active' write would be an observable upgrade.
        d1.upsert_tier("ten_1", "free", 1_700_000_000_000, "init")
            .unwrap();
        let e = env(
            "evt_su",
            "customer.subscription.updated",
            serde_json::json!({
                "object": {
                    "id": "sub_1",
                    "status": non_granting,
                    "metadata": { "tenant_id": "ten_1" },
                    "plan": { "id": "plan_pro" },
                    "quantity": 5,
                }
            }),
        );
        handler.on_subscription_updated(&e).unwrap();
        // Entitlement gate NOT advanced to the paid 'pro' tier: the
        // materializer skipped the 'active' upsert for the non-granting
        // status (tier stays at the pre-existing baseline).
        assert_eq!(
            d1.tier_for("ten_1"),
            Some("free".to_string()),
            "status={non_granting} must NOT grant the paid tier (subscription_state='active')"
        );
        // No tier-change entitlement audit was emitted (the gate fired
        // before persist_tier_change).
        assert_eq!(
            audit.count_event("corelink.tenant.tier_changed.v1"),
            0,
            "status={non_granting} must not emit a tier_changed entitlement audit"
        );
    }

    // Control: an `active` status with the same recognized plan DOES grant.
    let (handler, d1, audit) = fixture();
    d1.upsert_tier("ten_1", "free", 1_700_000_000_000, "init")
        .unwrap();
    let e = env(
        "evt_su_ok",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_1",
                "status": "active",
                "metadata": { "tenant_id": "ten_1" },
                "plan": { "id": "plan_pro" },
                "quantity": 5,
            }
        }),
    );
    handler.on_subscription_updated(&e).unwrap();
    assert_eq!(d1.tier_for("ten_1"), Some("pro".to_string()));
    assert_eq!(audit.count_event("corelink.tenant.tier_changed.v1"), 1);
}

#[test]
fn subscription_deleted_marks_canceled_and_preserves_historical_tier() {
    // Stripe-cancel tier-race fix: previously named
    // `subscription_deleted_marks_canceled_and_downgrades_to_free` and
    // asserted `tier_for("ten_1") == Some("free")` — that encoded the BUG
    // (the container overwriting `tier_selections.tier` to `'free'` on
    // cancel, racing the signup-worker authority, which deliberately
    // leaves `tier` untouched and only flips `subscription_state`).
    // Renamed + re-asserted: cancel must set `subscription_state` to the
    // access-OFF gate (proven by `mark_subscription_canceled`'s row /
    // `downgrade_tier` being the driven path — see
    // `subscription_deleted_takes_downgrade_path_not_grant_path` below)
    // WITHOUT touching the historical `tier` label — it must stay `pro`.
    let (handler, d1, audit) = fixture();
    d1.upsert_tier("ten_1", "pro", 1_700_000_000_000, "init")
        .unwrap();
    let e = env(
        "evt_sd",
        "customer.subscription.deleted",
        serde_json::json!({
            "object": {
                "id": "sub_1",
                "status": "canceled",
                "metadata": { "tenant_id": "ten_1" },
            }
        }),
    );
    handler.on_subscription_deleted(&e).unwrap();
    assert_eq!(
        d1.tier_for("ten_1"),
        Some("pro".to_string()),
        "cancel must preserve the historical tier label, NOT overwrite it to 'free'"
    );
    assert_eq!(audit.count_event("corelink.tenant.tier_changed.v1"), 1);
    assert_eq!(
        audit.count_event("corelink.billing.subscription_canceled.materialized.v1"),
        1
    );
}

/// A [`BillingD1Writer`] decorator that records WHICH tier-write method the
/// handler invoked (grant `upsert_tier` vs cancel `downgrade_tier`). Every
/// other method + all state forwards to the wrapped [`InMemoryBillingD1`]
/// so the existing fixture semantics are preserved.
#[derive(Debug)]
struct TierPathRecordingD1 {
    inner: Arc<InMemoryBillingD1>,
    upsert_calls: std::sync::Mutex<Vec<String>>,
    downgrade_calls: std::sync::Mutex<Vec<String>>,
}

impl TierPathRecordingD1 {
    fn new(inner: Arc<InMemoryBillingD1>) -> Self {
        Self {
            inner,
            upsert_calls: std::sync::Mutex::new(Vec::new()),
            downgrade_calls: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl BillingD1Writer for TierPathRecordingD1 {
    fn upsert_customer(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.upsert_customer(row)
    }
    fn upsert_subscription(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.upsert_subscription(row)
    }
    fn mark_subscription_canceled(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.mark_subscription_canceled(row)
    }
    fn upsert_invoice(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.upsert_invoice(row)
    }
    fn insert_dispute(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.insert_dispute(row)
    }
    fn insert_refund(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.inner.insert_refund(row)
    }
    fn try_record_event(
        &self,
        stripe_event_id: &str,
        canonical_event_type: &str,
        now_ms: u64,
        outcome: crate::d1::WebhookOutcome,
    ) -> Result<bool, BillingD1Error> {
        self.inner
            .try_record_event(stripe_event_id, canonical_event_type, now_ms, outcome)
    }
    fn read_tier(&self, tenant_id: &str) -> Result<Option<String>, BillingD1Error> {
        self.inner.read_tier(tenant_id)
    }
    fn upsert_tier(
        &self,
        tenant_id: &str,
        tier_wire: &str,
        now_ms: i64,
        correlation_id: &str,
    ) -> Result<(), BillingD1Error> {
        self.upsert_calls
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("rec mutex: {e}")))?
            .push(tier_wire.to_string());
        self.inner
            .upsert_tier(tenant_id, tier_wire, now_ms, correlation_id)
    }
    fn downgrade_tier(
        &self,
        tenant_id: &str,
        tier_wire: &str,
        now_ms: i64,
        correlation_id: &str,
    ) -> Result<(), BillingD1Error> {
        self.downgrade_calls
            .lock()
            .map_err(|e| BillingD1Error::Transient(format!("rec mutex: {e}")))?
            .push(tier_wire.to_string());
        self.inner
            .downgrade_tier(tenant_id, tier_wire, now_ms, correlation_id)
    }
    fn upsert_runners_entitlement(
        &self,
        tenant_id: &str,
        max_concurrency: u32,
        max_vcpu_h: u32,
        now_ms: i64,
    ) -> Result<(), BillingD1Error> {
        self.inner
            .upsert_runners_entitlement(tenant_id, max_concurrency, max_vcpu_h, now_ms)
    }
    fn delete_runners_entitlement(&self, tenant_id: &str) -> Result<(), BillingD1Error> {
        self.inner.delete_runners_entitlement(tenant_id)
    }
}

#[test]
fn subscription_deleted_takes_downgrade_path_not_grant_path() {
    // CAA-360 MEDIUM: a `customer.subscription.deleted` must drive the
    // DOWNGRADE path (which writes subscription_state='inactive'), NOT the
    // grant `upsert_tier` path (which hard-codes 'active' → contradictory
    // active-free row). We seed a starting tier via the inner mirror
    // directly (so that write is NOT counted against the recorder) then
    // assert the handler used downgrade_tier — never upsert_tier.
    let inner = Arc::new(InMemoryBillingD1::new());
    inner
        .upsert_tier("ten_1", "pro", 1_700_000_000_000, "init")
        .unwrap();
    let rec = Arc::new(TierPathRecordingD1::new(inner));
    let audit = Arc::new(InMemoryBillingAuditEmitter::new());
    let sel = Arc::new(InMemoryTierSelector::with_mapping(&[(
        "plan_pro",
        TierKind::Pro,
    )]));
    let handler = D1SubscriptionStateHandler::new(rec.clone(), audit.clone(), sel).with_clock(
        Arc::new(InMemoryFakeMatClock::at_unix_ms(1_700_000_000_000)),
    );

    let e = env(
        "evt_sd2",
        "customer.subscription.deleted",
        serde_json::json!({
            "object": {
                "id": "sub_1",
                "status": "canceled",
                "metadata": { "tenant_id": "ten_1" },
            }
        }),
    );
    handler.on_subscription_deleted(&e).unwrap();

    // The cancel path drove downgrade_tier exactly once (→ free), and
    // NEVER the grant path (upsert_tier) — no 'active'-writing statement
    // was reached.
    let downgrades = rec.downgrade_calls.lock().unwrap().clone();
    let upserts = rec.upsert_calls.lock().unwrap().clone();
    assert_eq!(
        downgrades,
        vec!["free".to_string()],
        "cancel must drive downgrade_tier('free') exactly once"
    );
    assert!(
        upserts.is_empty(),
        "cancel must NOT drive the grant upsert_tier path: {upserts:?}"
    );
    // The handler still DRIVES downgrade_tier with tier_wire="free" (the
    // call-site argument, asserted above), but the tier column itself must
    // stay 'pro' — the underlying writer (mirroring SQL_DOWNGRADE_TIER)
    // preserves the existing row's tier rather than overwriting it.
    assert_eq!(rec.inner.tier_for("ten_1").as_deref(), Some("pro"));
    assert_eq!(audit.count_event("corelink.tenant.tier_changed.v1"), 1);
}

#[test]
fn invoice_paid_writes_audit_and_d1() {
    let (handler, d1, audit) = fixture();
    let e = env(
        "evt_ip",
        "invoice.paid",
        serde_json::json!({
            "object": {
                "id": "in_1",
                "metadata": { "tenant_id": "ten_1" },
            }
        }),
    );
    handler.on_invoice_paid(&e).unwrap();
    assert_eq!(d1.count_table("stripe_invoices"), 1);
    assert_eq!(
        audit.count_event("corelink.billing.invoice.materialized.v1"),
        1
    );
}

#[test]
fn dispute_created_emits_sev1_audit() {
    let (handler, _d1, audit) = fixture();
    let e = env(
        "evt_dc",
        "charge.dispute.created",
        serde_json::json!({
            "object": {
                "id": "dp_1",
                "metadata": { "tenant_id": "ten_1" },
            }
        }),
    );
    handler.on_charge_dispute_created(&e).unwrap();
    let rows = audit.snapshot();
    assert!(rows.iter().any(|r| r.severity == AuditSeverity::Sev1));
}

#[test]
fn audit_failure_aborts_d1_write_fail_closed() {
    let (handler, d1, audit) = fixture();
    audit.arm_failure(BillingAuditError::Transient("audit down".into()));
    let e = env(
        "evt_ip",
        "invoice.paid",
        serde_json::json!({
            "object": {
                "id": "in_1",
                "metadata": { "tenant_id": "ten_1" },
            }
        }),
    );
    let err = handler.on_invoice_paid(&e).unwrap_err();
    assert!(matches!(err, MaterializerError::Transient(_)));
    // No D1 row written.
    assert_eq!(d1.count_table("stripe_invoices"), 0);
}

#[test]
fn missing_tenant_id_surfaces_invalid_payload() {
    let (handler, _d1, _audit) = fixture();
    let e = env(
        "evt_x",
        "invoice.paid",
        serde_json::json!({ "object": { "id": "in_1" } }),
    );
    let err = handler.on_invoice_paid(&e).unwrap_err();
    assert!(matches!(err, MaterializerError::InvalidPayload(_)));
}

#[test]
fn unknown_plan_in_subscription_update_surfaces_invalid_payload() {
    let (handler, d1, _audit) = fixture();
    d1.upsert_tier("ten_1", "starter", 1_700_000_000_000, "init")
        .unwrap();
    let e = env(
        "evt_su",
        "customer.subscription.updated",
        serde_json::json!({
            "object": {
                "id": "sub_1",
                "status": "active",
                "metadata": { "tenant_id": "ten_1" },
                "plan": { "id": "plan_DOESNOTEXIST" },
            }
        }),
    );
    let err = handler.on_subscription_updated(&e).unwrap_err();
    assert!(matches!(err, MaterializerError::InvalidPayload(_)));
}
