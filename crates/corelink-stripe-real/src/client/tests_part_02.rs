fn checkout_req() -> CheckoutSessionRequest {
    CheckoutSessionRequest::new(
        corelink_tier_selection::tenant::TenantId::new("tenant_promo"),
        corelink_tier_selection::tier::TierKind::Starter,
        "buyer@example.test",
        "https://app/ok",
        "https://app/cancel",
    )
}

fn form_get<'a>(form: &'a [(&'static str, String)], key: &str) -> Option<&'a str> {
    form.iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.as_str())
}

#[test]
fn checkout_form_default_enables_promo_code_field() {
    // Absent a configured coupon, the hosted page must show the promo-code
    // field so a user can enter a code mapped to a 100%-off coupon.
    let form = build_checkout_form(
        &checkout_req(),
        "price_123",
        &CheckoutPromo::AllowCodes,
        "cus_ABC",
    );
    assert_eq!(
        form_get(&form, "allow_promotion_codes"),
        Some("true"),
        "default checkout must set allow_promotion_codes=true: {form:?}"
    );
    // Mutually exclusive with `discounts` — must NOT co-occur.
    assert!(
        form.iter().all(|(k, _)| !k.starts_with("discounts")),
        "allow_promotion_codes and discounts must never co-occur: {form:?}"
    );
    // The base params are untouched (pricing / mode / metadata preserved).
    assert_eq!(form_get(&form, "mode"), Some("subscription"));
    assert_eq!(form_get(&form, "line_items[0][price]"), Some("price_123"));
    assert_eq!(form_get(&form, "metadata[tenant_id]"), Some("tenant_promo"));
}

#[test]
fn checkout_form_attaches_customer_and_never_sends_customer_email() {
    // The flow pre-creates a Customer (no email) and ATTACHES it so
    // `session.customer` is present at creation (a subscription session
    // created without one leaves it null → `missing customer on checkout
    // session`). The buyer's email is collected by Stripe's hosted page, so
    // the form MUST carry `customer` and MUST NOT carry `customer_email`
    // (Stripe rejects an empty one with `Invalid email address: ` — the
    // money-path outage — and it is redundant with `customer`).
    let form = build_checkout_form(
        &checkout_req(),
        "price_123",
        &CheckoutPromo::AllowCodes,
        "cus_ABC",
    );
    assert_eq!(
        form_get(&form, "customer"),
        Some("cus_ABC"),
        "checkout must attach the pre-created customer: {form:?}"
    );
    assert!(
        form.iter().all(|(k, _)| *k != "customer_email"),
        "customer_email must NEVER be sent (empty is rejected; redundant with customer): {form:?}"
    );
}

#[test]
fn checkout_form_coupon_preapplies_discount_and_omits_allow_codes() {
    // A configured coupon is PRE-APPLIED via discounts[0][coupon] (clean
    // checkout → $0) and MUST NOT be combined with allow_promotion_codes.
    let form = build_checkout_form(
        &checkout_req(),
        "price_123",
        &CheckoutPromo::Coupon("coupon_LAUNCH100".to_string()),
        "cus_ABC",
    );
    assert_eq!(
        form_get(&form, "discounts[0][coupon]"),
        Some("coupon_LAUNCH100"),
        "coupon path must pre-apply discounts[0][coupon]: {form:?}"
    );
    assert!(
        form.iter().all(|(k, _)| *k != "allow_promotion_codes"),
        "coupon path must NOT also set allow_promotion_codes (Stripe rejects both): {form:?}"
    );
    // Base params preserved.
    assert_eq!(form_get(&form, "line_items[0][quantity]"), Some("1"));
}

#[test]
fn promo_from_env_absent_coupon_allows_codes() {
    let _g = EnvGuard::new(&["STRIPE_LAUNCH_COUPON"]);
    env::remove_var("STRIPE_LAUNCH_COUPON");
    assert_eq!(CheckoutPromo::from_env(), CheckoutPromo::AllowCodes);
}

#[test]
fn promo_from_env_empty_coupon_allows_codes() {
    // Empty / whitespace-only env is treated as unset (no phantom coupon).
    let _g = EnvGuard::new(&["STRIPE_LAUNCH_COUPON"]);
    env::set_var("STRIPE_LAUNCH_COUPON", "   ");
    assert_eq!(CheckoutPromo::from_env(), CheckoutPromo::AllowCodes);
}

#[test]
fn promo_from_env_present_coupon_preapplies() {
    let _g = EnvGuard::new(&["STRIPE_LAUNCH_COUPON"]);
    env::set_var("STRIPE_LAUNCH_COUPON", "  coupon_LAUNCH100  ");
    // Trimmed value is used.
    assert_eq!(
        CheckoutPromo::from_env(),
        CheckoutPromo::Coupon("coupon_LAUNCH100".to_string())
    );
}

#[test]
fn checkout_idempotency_is_tenant_scoped_not_tier_scoped() {
    assert_eq!(
        checkout_idempotency_key("tenant_a", corelink_tier_selection::tier::TierKind::Starter),
        "checkout:cache:tenant_a"
    );
    assert_ne!(
        checkout_idempotency_key("tenant_a", corelink_tier_selection::tier::TierKind::Starter),
        checkout_idempotency_key("tenant_b", corelink_tier_selection::tier::TierKind::Starter)
    );
    assert_ne!(
        checkout_idempotency_key("tenant_a", corelink_tier_selection::tier::TierKind::Starter),
        checkout_idempotency_key(
            "tenant_a",
            corelink_tier_selection::tier::TierKind::RunnerStarter
        )
    );
    assert_eq!(
        checkout_idempotency_key(
            "tenant_a",
            corelink_tier_selection::tier::TierKind::RunnerStarter
        ),
        "checkout:runner:tenant_a"
    );
}
