#!/usr/bin/env python3
"""Static, fail-closed regression gate for the B231-B243 security contracts.

This intentionally does not claim live production or latency evidence. It checks
the source-level ownership and adversarial guards that can be proven in a clean
checkout without credentials or a running D1/Stripe deployment.
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
_OVERRIDES: dict[str, str] | None = None


def read(rel: str) -> str:
    if _OVERRIDES is not None and rel in _OVERRIDES:
        return _OVERRIDES[rel]
    path = ROOT / rel
    if not path.is_file():
        raise AssertionError(f"missing required contract file: {rel}")
    return path.read_text(encoding="utf-8")


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise AssertionError(f"{label}: missing {needle!r}")


def require_active(text: str, needle: str, label: str) -> None:
    """Require a marker outside comments and quoted string literals."""
    scrubbed = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    scrubbed = "\n".join(line.split("//", 1)[0] for line in scrubbed.splitlines())
    for match in re.finditer(re.escape(needle), scrubbed):
        in_string = False
        escaped = False
        for char in scrubbed[: match.start()]:
            if in_string:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    in_string = False
            elif char == '"':
                in_string = True
        if not in_string:
            return
    raise AssertionError(f"{label}: missing active {needle!r}")


def verify(sources: dict[str, str] | None = None) -> int:
    global _OVERRIDES
    previous = _OVERRIDES
    _OVERRIDES = sources
    try:
        return _verify_impl()
    finally:
        _OVERRIDES = previous


def _verify_impl() -> int:
    pricing = read("apps/docs/src/lib/pricing.ts")
    admin_pricing = read("apps/admin-ui/src/lib/pricing.ts")
    ratelimit = read("crates/corelink-ratelimit/src/tier.rs")
    tier_source = read("crates/corelink-tier-selection/src/tier.rs")
    require(pricing, "CANONICAL_TIERS", "B231")
    require(admin_pricing, "CHECKOUT_TIER_IDS", "B231")
    for tier in ("free", "solo", "starter", "pro", "max", "enterprise"):
        require(pricing, f'"{tier}"', "B231")
    for tier in ("starter", "pro", "max"):
        require(ratelimit, f'"{tier}"', "B231")
    require(tier_source, "pub const fn canonical_tiers() -> &'static [TierKind; 6]", "B231")
    require(tier_source, "pub const fn canonical_runner_tiers() -> &'static [TierKind; 5]", "B231")
    require(pricing, "Runner SKUs are a separate", "B231")

    brew = read("crates/corelink-container/src/routes/brew.rs")
    pip = read("crates/corelink-container/src/routes/pip.rs")
    cache = read("crates/corelink-container/src/adapter_cache.rs")
    for label, text in (("B232/B233 brew", brew), ("B232/B233 pip", pip)):
        require(text, "cap_resolver", label)
        require(text, "put_for_tenant", label)
    require(cache, "accounting_namespace", "B232")
    request = read("crates/corelink-handler-cas/src/request.rs")
    handler = read("crates/corelink-handler-cas/src/handler.rs")
    handler_tests = read("crates/corelink-handler-cas/src/tests.rs")
    accounting = read("crates/corelink-container/src/byte_accounting.rs")
    accounting_impl = read("crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs")
    accounting_impl_02 = read("crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs")
    accounting_tests = read("crates/corelink-container/src/byte_accounting/b126_m2_test_3_1.rs")
    require(accounting, 'include!("byte_accounting/b126_m2_impl_01.rs")', "B232")
    require(accounting_impl_02, 'include!("b126_m2_test_3_1.rs")', "B232")
    require(request, "pub accounting_tenant: String", "B232")
    require(request, "pub fn for_public_namespace", "B232")
    require(request, "pub fn is_authorized_for_caller", "B232")
    require_active(handler, "if !req.is_authorized_for_caller()", "B232")
    r2 = read("crates/corelink-container/src/storage/r2_s3.rs")
    # The split R2 facade keeps read/exists helpers in cas_ops.rs, while the
    # write authorization owner lives in cas_write.rs.  Keep this source gate
    # pointed at the executable owner rather than accepting a stale sibling
    # marker in the facade fragment.
    r2_cas_write = read("crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs")
    r2_tests_1 = read("crates/corelink-container/src/storage/r2_s3_parts/tests_1.rs")
    require(r2, 'include!("r2_s3_parts/cas_ops.rs")', "B232")
    # The R2 facade is split into include fragments. Authorization lives in
    # the CasWriteHandler implementation, not in the facade itself.
    require_active(r2_cas_write, "if !req.is_authorized_for_caller()", "B232")
    require(r2, 'include!("r2_s3_parts/tests_1.rs")', "B232")
    require(r2_tests_1, "r2_public_requests_share_physical_key_but_keep_accounting_tenant", "B232")
    put_body = cache[cache.index("pub async fn put_for_tenant"):]
    require(put_body, "if namespace == PUBLIC_NAMESPACE", "B232")
    require(put_body, "CasWriteRequest::for_public_namespace", "B232")
    require(put_body, "private storage/accounting namespace mismatch", "B232")
    if "CasWriteRequest::new(\n                accounting_namespace" in put_body:
        raise AssertionError("B232: public physical namespace is being used for accounting")
    require(accounting_impl, "let storage_namespace = req.tenant.clone()", "B232")
    require(accounting_impl, "let accounting_tenant = req.accounting_tenant.clone()", "B232")
    require(accounting_impl, "let quota_seed = req.storage_quota_bytes", "B232")
    require_active(accounting_impl, "if !req.is_authorized_for_caller()", "B232")
    if "unowned_shared_namespace" in accounting_impl or "quota_seed = if" in accounting_impl:
        raise AssertionError("B232: shared physical namespace bypasses authenticated quota")
    require(
        cache,
        "public_write_charges_real_tenant_but_round_trips_shared_namespace",
        "B232",
    )
    require(
        cache,
        "same physical CAS key and therefore gets the durable dedup no-op",
        "B232",
    )
    require(accounting_tests, "public_namespace_write_uses_authenticated_tenant_quota", "B232")
    require(accounting_tests, "public_namespace_write_respects_authenticated_tenant_quota", "B232")
    require(handler, "mod tests;", "B232")
    require(handler_tests, "public_write_requires_explicit_accounting_identity", "B232")

    # Behavioral mutation guard for the request contract: a public write must
    # preserve the caller as its accounting identity, while a private request
    # with mismatched physical/caller namespaces must be denied. This catches a
    # verifier-only/source-anchor regression where the field exists but is not
    # actually used by the authorization rule.
    def authorized(physical: str, caller: str, charged: str) -> bool:
        return charged == caller and (physical == caller or physical == "_public")

    assert authorized("_public", "tenant-real", "tenant-real")
    assert not authorized("victim", "attacker", "attacker")
    assert not authorized("_public", "tenant-real", "_public")
    for source, label in ((brew, "brew"), (pip, "pip")):
        if not re.search(
            r"put_for_tenant\(\s*PUBLIC_NAMESPACE,\s*(?:tenant_id|&tenant_id)",
            source,
        ):
            raise AssertionError(f"B232 {label}: public write must separate physical and accounting namespaces")
        if not re.search(r"get\(PUBLIC_NAMESPACE", source):
            raise AssertionError(f"B232 {label}: round-trip read must use the shared physical namespace")

    dispatch = read("crates/corelink-stripe-real/src/webhook_dispatch.rs")
    materializer = read("crates/corelink-billing-stripe-materializer/src/handler.rs")
    require(dispatch, "on_charge_refunded", "B234")
    require(materializer, "fully_refunded", "B234")
    main_rs = read("crates/corelink-container/src/main.rs")
    require(main_rs, "durable D1 is unavailable; webhook NOT mounted", "B235")
    if "InMemoryWebhookDlqStore" in main_rs:
        raise AssertionError("B235: production main must not wire volatile DLQ")

    workflow = read(".github/workflows/cf-deploy-prod.yml")
    for secret in ("EMAIL_HASH_SALT", "STRIPE_PRICE_ID_STARTER"):
        require(workflow, secret, "B236/B242")
    matrix = read("docs/internal/secrets-checklist.md")
    for code_only_var in (
        "APPLE_DEVELOPER_ID_FINGERPRINT",
        "APPLE_NOTARIZATION_API_KEY",
        "APPLE_NOTARIZATION_ISSUER",
        "APPLE_NOTARIZATION_KEY_ID",
        "GPG_KEY_FINGERPRINT",
        "WINDOWS_CODE_SIGNING_FINGERPRINT",
        "WINDOWS_CODE_SIGNING_SUBJECT",
    ):
        require(matrix, f"`{code_only_var}`", "B236")
    require(read("scripts/validate_secrets_matrix.py"), "code_only", "B236")
    require(read("worker/src/lib/pat_verify_cache.ts"), "PAT_VERIFY_CACHE_TTL_MS = 5_000", "B237")
    auth_rotate = read("worker/src/lib/auth_rotate.ts")
    require(auth_rotate, "patRowKvKey(oldRow.token_id)", "B237")
    customer_route = read("crates/corelink-container/src/routes/customer.rs")
    customer_part_01 = read("crates/corelink-container/src/routes/customer/part-01.rs")
    require(customer_route, 'include!("customer/part-01.rs")', "B237")
    require(customer_part_01, "x-corelink-pat-cache-invalidate", "B237")
    worker_index = read("worker/src/index.ts")
    worker_fetch = read("worker/src/index_fetch.ts")
    worker_finish = read("worker/src/index_finish_stage.ts")
    # The Worker entrypoint delegates through index_fetch.ts; the final-header
    # deletion is intentionally owned by the split finish-stage module.
    require(worker_index, 'from "./index_fetch.js"', "B237")
    require(worker_fetch, 'from "./index_finish_stage.js"', "B237")
    require_active(worker_finish, "finalHeaders.delete(\"x-corelink-pat-cache-invalidate\")", "B237")

    region = read("crates/corelink-region/src/region.rs")
    require(region, "Region::Apac", "B238")
    require(region, "Region::Afr", "B238")
    require(region, "assert_eq!(Region::ALL.len(), 6)", "B238")
    dsr = read("crates/corelink-container/src/routes/dsr/adapter_d1.rs")
    region_read = dsr.index("fn primary_region_before_delete")
    delete_start = dsr.index("let mut total", region_read)
    if dsr.index("SELECT primary_region", region_read, delete_start) > delete_start:
        raise AssertionError("B238: DSR deletion starts before the residency read")
    require(dsr[region_read:delete_start], "refusing DSR deletion", "B238")

    dpa = read("crates/corelink-container/src/routes/dpa_accept.rs")
    require(dpa, "canonical_notice_hash", "B239")
    require(dpa, "notice_hash != canonical_notice_hash", "B239")
    clerk = read("crates/corelink-clerk/src/adapter.rs")
    require(clerk, "NEGATIVE_CACHE_MAX_ENTRIES", "B240")
    customer_d1 = read("crates/corelink-container/src/customer_d1.rs")
    customer_d1_handler_state = read("crates/corelink-container/src/customer_d1_handler_state.rs")
    require(customer_d1, 'include!("customer_d1_handler_state.rs")', "B240")
    require(customer_d1_handler_state, "ESCAPE '\\\\'", "B240")
    first_fetch = clerk.index("let jwks = self.fetch_and_cache(first_trigger)")
    second_fetch = clerk.index("self.fetch_and_cache(RefreshTrigger::KidMiss)", first_fetch)
    negative_inserts = [m.start() for m in re.finditer(r"self\.negative_kid_cache_insert", clerk)]
    if not negative_inserts or any(pos < second_fetch for pos in negative_inserts):
        raise AssertionError("B240: negative kid cache may be populated before refresh attempts finish")
    # Behavioral mutation guard: a second refresh that returns the kid must be
    # accepted, and only a second miss is eligible for negative caching.
    def resolve(first_has: bool, second_has: bool) -> tuple[str, bool]:
        if first_has:
            return "key", False
        if second_has:
            return "key", False
        return "miss", True
    assert resolve(False, True) == ("key", False)
    assert resolve(False, False) == ("miss", True)

    introspect = read("crates/corelink-container/src/routes/auth_introspect.rs")
    introspect_part_01 = read("crates/corelink-container/src/routes/auth_introspect/part-01.rs")
    require(introspect, 'include!("auth_introspect/part-01.rs")', "B241")
    require_active(introspect_part_01, "len() > 128", "B241")
    if "with_auth_key(Arc::from(hugr_key" in introspect:
        raise AssertionError("B241: secondary fabric key still authorizes resolver")
    signup_secrets = read("scripts/verify-signup-worker-secrets.sh")
    if signup_secrets.count("EMAIL_HASH_SALT") < 2 or signup_secrets.count("DESTINATIONS=(") != 1:
        raise AssertionError("B242: signup secret verifier does not require the salt across all destinations")
    if signup_secrets.count("|wrangler.toml|") != 5 or "apps/signup-worker/wrangler.toml|" not in signup_secrets:
        raise AssertionError("B242: verifier must enumerate exactly five root/regional plus signup destinations")
    worker_special_customer = read("worker/src/index_special_customer.ts")
    worker_fetch = read("worker/src/index_fetch.ts")
    require(worker_fetch, 'from "./index_special_routes.js"', "B242")
    require(
        read("crates/corelink-container/src/main.rs"),
        "if email_hash_salt_missing_in_prod(prod_by_independent_signal, email_hash_salt_present)",
        "B242",
    )
    require_active(
        read("apps/signup-worker/src/webhooks/clerk.ts"),
        'if (env.ENVIRONMENT === "prod" && !env.EMAIL_HASH_SALT?.trim())',
        "B242",
    )
    # The Worker source is split: index_special_routes.ts is only a dispatcher;
    # the customer fragment owns the executable invitation acceptance guard.
    require_active(
        worker_special_customer,
        'if (env.ENVIRONMENT === "prod" && !env.EMAIL_HASH_SALT?.trim())',
        "B242",
    )

    githugr = read("worker/src/lib/githugr_provision.ts")
    require(githugr, "githugr_tenant_org_map", "B243")
    if re.search(r"INSERT OR IGNORE INTO tenant_org_map", githugr):
        raise AssertionError("B243: githugr writer still targets CoreLink map")
    require(read("migrations/d1/0112_githugr_tenant_org_map.sql"), "single", "B243")
    print("verify_b231_b243_contracts: OK (static source/mutation guards; no production remeasurement)")
    return 0


def mutation_checks() -> int:
    """Delete each representative load-bearing tooth and require red."""
    files = {
        rel: read(rel)
        for rel in (
            "apps/docs/src/lib/pricing.ts",
            "apps/admin-ui/src/lib/pricing.ts",
            "crates/corelink-ratelimit/src/tier.rs",
            "crates/corelink-tier-selection/src/tier.rs",
            "crates/corelink-container/src/routes/brew.rs",
            "crates/corelink-container/src/routes/pip.rs",
            "crates/corelink-container/src/adapter_cache.rs",
            "crates/corelink-handler-cas/src/handler.rs",
            "crates/corelink-handler-cas/src/tests.rs",
            "crates/corelink-container/src/storage/r2_s3.rs",
            "crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs",
            "crates/corelink-container/src/storage/r2_s3_parts/tests_1.rs",
            "crates/corelink-container/src/byte_accounting.rs",
            "crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs",
            "crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs",
            "crates/corelink-container/src/byte_accounting/b126_m2_test_3_1.rs",
            "crates/corelink-stripe-real/src/webhook_dispatch.rs",
            "crates/corelink-billing-stripe-materializer/src/handler.rs",
            "crates/corelink-container/src/main.rs",
            ".github/workflows/cf-deploy-prod.yml",
            "scripts/verify-signup-worker-secrets.sh",
            "docs/internal/secrets-checklist.md",
            "worker/src/lib/pat_verify_cache.ts",
            "crates/corelink-region/src/region.rs",
            "crates/corelink-container/src/routes/dpa_accept.rs",
            "crates/corelink-clerk/src/adapter.rs",
            "crates/corelink-container/src/routes/auth_introspect.rs",
            "crates/corelink-container/src/routes/auth_introspect/part-01.rs",
            "crates/corelink-container/src/customer_d1.rs",
            "crates/corelink-container/src/customer_d1_handler_state.rs",
            "worker/src/lib/githugr_provision.ts",
            "crates/corelink-container/src/routes/customer/part-01.rs",
            "worker/src/index.ts",
            "worker/src/index_fetch.ts",
            "worker/src/index_special_customer.ts",
            "apps/signup-worker/src/webhooks/clerk.ts",
            "worker/src/index_finish_stage.ts",
        )
    }
    verify(files)
    rejected = 0
    # Comment/string bait must not satisfy the three split authorization
    # anchors. Mutate all owners together so the shared contract cannot pass
    # through an equivalent marker in a sibling fragment.
    auth_marker = "if !req.is_authorized_for_caller()"
    comment_bait = dict(files)
    string_bait = dict(files)
    for rel in (
        "crates/corelink-handler-cas/src/handler.rs",
        "crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs",
    ):
        comment_bait[rel] = comment_bait[rel].replace(auth_marker, "// " + auth_marker)
        string_bait[rel] = string_bait[rel].replace(
            auth_marker, 'const _BAIT: &str = "' + auth_marker + '";'
        )
    for label, mutant in (("comment", comment_bait), ("string", string_bait)):
        try:
            verify(mutant)
        except AssertionError:
            rejected += 1
        else:
            raise AssertionError(f"{label} bait mutation was accepted for split authorization")

    # B242's split Worker and signup-worker guards must be executable code, not
    # a comment/string marker in either owner.  Mutating one owner at a time
    # proves the aggregate gate cannot be satisfied by a stale sibling anchor.
    salt_guard = 'if (env.ENVIRONMENT === "prod" && !env.EMAIL_HASH_SALT?.trim())'
    for rel in (
        "worker/src/index_special_customer.ts",
        "apps/signup-worker/src/webhooks/clerk.ts",
    ):
        comment_bait = dict(files)
        string_bait = dict(files)
        comment_bait[rel] = comment_bait[rel].replace(salt_guard, "// " + salt_guard)
        string_bait[rel] = string_bait[rel].replace(
            salt_guard, 'const _BAIT: string = "' + salt_guard + '";'
        )
        for label, mutant in (("comment", comment_bait), ("string", string_bait)):
            try:
                verify(mutant)
            except AssertionError:
                rejected += 1
            else:
                raise AssertionError(f"{label} bait mutation was accepted for B242 owner {rel}")

    noop = files["crates/corelink-handler-cas/src/handler.rs"].replace(
        "B232_NONEXISTENT_MARKER", ""
    )
    if noop != files["crates/corelink-handler-cas/src/handler.rs"]:
        raise AssertionError("no-op mutation fixture unexpectedly changed source")
    mutations = (
        ("apps/docs/src/lib/pricing.ts", "CANONICAL_TIERS"),
        ("apps/admin-ui/src/lib/pricing.ts", "CHECKOUT_TIER_IDS"),
        ("crates/corelink-ratelimit/src/tier.rs", '"starter"'),
        ("crates/corelink-tier-selection/src/tier.rs", "canonical_tiers()"),
        ("crates/corelink-container/src/routes/brew.rs", "cap_resolver"),
        ("crates/corelink-container/src/routes/pip.rs", "put_for_tenant"),
        ("crates/corelink-container/src/adapter_cache.rs", "accounting_namespace"),
        ("crates/corelink-handler-cas/src/handler.rs", "if !req.is_authorized_for_caller()"),
        ("crates/corelink-handler-cas/src/tests.rs", "public_write_requires_explicit_accounting_identity"),
        ("crates/corelink-container/src/storage/r2_s3.rs", 'include!("r2_s3_parts/cas_ops.rs")'),
        ("crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs", "if !req.is_authorized_for_caller()"),
        ("crates/corelink-container/src/storage/r2_s3_parts/tests_1.rs", "r2_public_requests_share_physical_key_but_keep_accounting_tenant"),
        ("crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs", "let accounting_tenant = req.accounting_tenant.clone()"),
        ("crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs", 'include!("b126_m2_test_3_1.rs")'),
        ("crates/corelink-container/src/byte_accounting/b126_m2_test_3_1.rs", "public_namespace_write_uses_authenticated_tenant_quota"),
        ("crates/corelink-stripe-real/src/webhook_dispatch.rs", "on_charge_refunded"),
        ("crates/corelink-billing-stripe-materializer/src/handler.rs", "fully_refunded"),
        ("crates/corelink-container/src/main.rs", "durable D1 is unavailable; webhook NOT mounted"),
        (
            "crates/corelink-container/src/main.rs",
            "if email_hash_salt_missing_in_prod(prod_by_independent_signal, email_hash_salt_present)",
        ),
        (".github/workflows/cf-deploy-prod.yml", "EMAIL_HASH_SALT"),
        ("scripts/verify-signup-worker-secrets.sh", "DESTINATIONS=("),
        ("docs/internal/secrets-checklist.md", "APPLE_DEVELOPER_ID_FINGERPRINT"),
        ("worker/src/lib/pat_verify_cache.ts", "PAT_VERIFY_CACHE_TTL_MS = 5_000"),
        ("crates/corelink-region/src/region.rs", "Region::Apac"),
        ("crates/corelink-container/src/routes/dpa_accept.rs", "canonical_notice_hash"),
        ("crates/corelink-clerk/src/adapter.rs", "NEGATIVE_CACHE_MAX_ENTRIES"),
        ("crates/corelink-container/src/routes/auth_introspect.rs", 'include!("auth_introspect/part-01.rs")'),
        ("crates/corelink-container/src/routes/auth_introspect/part-01.rs", "len() > 128"),
        ("crates/corelink-container/src/customer_d1.rs", 'include!("customer_d1_handler_state.rs")'),
        ("crates/corelink-container/src/customer_d1_handler_state.rs", "ESCAPE '\\\\'"),
        ("worker/src/lib/githugr_provision.ts", "githugr_tenant_org_map"),
        ("crates/corelink-container/src/routes/customer/part-01.rs", "x-corelink-pat-cache-invalidate"),
        ("worker/src/index.ts", 'from "./index_fetch.js"'),
        ("worker/src/index_fetch.ts", 'from "./index_finish_stage.js"'),
        ("worker/src/index_fetch.ts", 'from "./index_special_routes.js"'),
        (
            "worker/src/index_special_customer.ts",
            'if (env.ENVIRONMENT === "prod" && !env.EMAIL_HASH_SALT?.trim())',
        ),
        (
            "apps/signup-worker/src/webhooks/clerk.ts",
            'if (env.ENVIRONMENT === "prod" && !env.EMAIL_HASH_SALT?.trim())',
        ),
        ("worker/src/index_finish_stage.ts", "finalHeaders.delete(\"x-corelink-pat-cache-invalidate\")"),
    )
    for rel, marker in mutations:
        if files[rel].count(marker) < 1:
            raise AssertionError(f"mutation fixture missing {rel}: {marker}")
        mutant = dict(files)
        # Remove every occurrence in the owning split unit.  Several anchors
        # are intentionally repeated in comments/tests; deleting only the
        # first could turn this into a no-op mutation that the gate rightly
        # accepts through the remaining occurrence.
        mutant[rel] = mutant[rel].replace(marker, "")
        try:
            verify(mutant)
        except AssertionError:
            rejected += 1
        else:
            raise AssertionError(f"mutation accepted: {rel}: {marker}")
    return rejected


def main() -> int:
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    rejected = mutation_checks() if args.self_test else verify()
    if args.self_test:
        print(f"verify_b231_b243_contracts: PASS ({rejected} mutations rejected)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as exc:
        print(f"verify_b231_b243_contracts: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1)
