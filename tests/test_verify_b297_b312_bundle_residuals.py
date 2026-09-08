from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b297_b312_bundle_residuals as verify


def _text(path: str) -> str:
    return (verify.ROOT / path).read_text(encoding="utf-8")


def test_current_bundle_passes_and_population_is_closed() -> None:
    verify.verify()
    assert len(verify.CONTRACTS) == 16
    assert len(verify.CHECKS) == 16


@pytest.mark.parametrize(
    ("path", "needle"),
    (
        (verify.ADVERSARIAL, "AuditCapture, CasStore"),
        (verify.MAIN, "#[cfg(test)]\nuse boot::{build_runners_resolver_from, build_tier_selector_from};"),
        (verify.STORAGE_1, "#[test]"),
        (verify.STORAGE_3, '#[tokio::test(flavor = "multi_thread", worker_threads = 2)]'),
        (verify.STORAGE_3, "#[tokio::test(flavor = \"multi_thread\", worker_threads = 2)]"),
        (verify.FAILOVER, "#[cfg(test)]\n    #[must_use]\n    fn stale_after_ms"),
        (verify.OCI, "#[cfg(test)]\n    fn with_allowlist"),
        (verify.SLI, "#[cfg(test)]\n    fn counters_at"),
        (verify.REVOCATION, '#[must_use = "handle the result to obtain the configured revocation detector"]'),
        (verify.ACCOUNTING, "let committed_len = {"),
        (verify.AUDIT_DRAIN, "clippy::too_many_arguments"),
        (verify.CAS_SINGLE, "(auth, scope, headers): CasRequestAuth"),
        (verify.CAS_BATCH_WRITE, "(auth, scope, headers): CasRequestAuth"),
        (verify.CAS_BATCH_READ, "(auth, scope, headers): CasRequestAuth"),
        (verify.CAS_ERASE, "max_tenants.clamp(1, DEFAULT_MAX_TENANT_BLOOMS)"),
        (verify.SLI, ".is_some_and(|bucket|"),
        (verify.ADAPTER_CACHE, "type WriteRecord = (String, String, String, bool);"),
        (verify.REVOCATION_TESTS, "#[cfg(test)]\nmod tests {\n    #![allow(clippy::expect_used, clippy::indexing_slicing)]"),
        (verify.ORIGIN, "clippy::expect_used"),
        (verify.OCI_TEST, "seeded_hog_budget.is_some_and(|bytes| bytes < OCI_MAX_INFLIGHT_BYTES)"),
        (verify.BILLING, "let is_allow = matches!(outcome.decision, QuotaCasDecision::Allow { .. });"),
        (verify.BILLING, "let is_deny = matches!(outcome.decision, QuotaCasDecision::Deny429 { .. });"),
    ),
)
def test_each_family_rejects_a_targeted_mutation(path: str, needle: str) -> None:
    source = _text(path)
    assert needle in source, f"fixture needle disappeared: {path}: {needle}"
    mutated = source.replace(needle, "", 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: mutated})


def test_unknown_override_is_rejected() -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={"not-a-contract.rs": ""})


def test_missing_targets_fail_closed(tmp_path: Path) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(root=tmp_path)


def test_non_text_override_is_rejected() -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.ADVERSARIAL: None})  # type: ignore[arg-type]


def _reject_mutation(path: str, needle: str, replacement: str) -> None:
    source = _text(path)
    assert needle in source, f"fixture needle disappeared: {path}: {needle}"
    mutated = source.replace(needle, replacement, 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: mutated})


def test_reviewer_reproducers_reject_comment_or_string_bait() -> None:
    _reject_mutation(
        verify.ADVERSARIAL,
        "AuditCapture, CasStore, CmkRotationLedger, ConstantTimeAuthProbe,",
        "AuditCapture, CasStore, CmkRotationLedger, ConstantTimeAuthProbe, ExtraSymbol,",
    )
    _reject_mutation(
        verify.CAS_ERASE,
        "max_tenants: max_tenants.clamp(1, DEFAULT_MAX_TENANT_BLOOMS)",
        "max_tenants: 0 /* max_tenants.clamp(1, DEFAULT_MAX_TENANT_BLOOMS) */",
    )
    _reject_mutation(
        verify.ACCOUNTING,
        "match committed_len",
        "match unrelated /* match committed_len */",
    )
    _reject_mutation(
        verify.SLI,
        ">= MAX_WINDOW_MS",
        "> MAX_WINDOW_MS /* >= MAX_WINDOW_MS */",
    )
    _reject_mutation(
        verify.BILLING,
        "let is_allow = matches!(outcome.decision, QuotaCasDecision::Allow { .. });\n            prop_assert!(is_allow);",
        "let is_allow = true; /* let is_allow = matches!(outcome.decision, QuotaCasDecision::Allow { .. }); prop_assert!(is_allow); */",
    )


def test_reviewer_reproducers_reject_incomplete_semantics() -> None:
    _reject_mutation(
        verify.CAPACITY,
        "let read_peak_bytes = CAS_READ_SINGLE_PEAK_BYTES + CAS_READ_BATCH_PEAK_BYTES;",
        "let read_peak_bytes = 0; /* let read_peak_bytes = CAS_READ_SINGLE_PEAK_BYTES + CAS_READ_BATCH_PEAK_BYTES; */",
    )
    _reject_mutation(
        verify.OCI_TEST,
        "tenant_inflight.insert(hog_key.clone(), OCI_MAX_INFLIGHT_BYTES_PER_TENANT);",
        "tenant_inflight.insert(hog_key.clone(), Some(0).unwrap_or(0)); /* tenant_inflight.insert(hog_key.clone(), OCI_MAX_INFLIGHT_BYTES_PER_TENANT); */",
    )
    _reject_mutation(
        verify.AUDIT_DRAIN,
        "#[allow(\n    dead_code,\n    clippy::too_many_arguments,",
        "#[allow(\n    clippy::all,\n    dead_code,\n    clippy::too_many_arguments,",
    )
    _reject_mutation(
        verify.CAS_SINGLE,
        "(auth, scope, headers): CasRequestAuth,",
        "auth: crate::auth_tenant::AuthTenant,\n    scope: crate::scope::CacheScope,\n    headers: axum::http::HeaderMap,",
    )


def test_reviewer_reproducers_require_predicate_consumed_by_assert() -> None:
    capacity_needle = """assert!(budget_fits(
            CONTAINER_MEMORY_BYTES,
            declared_bytes,
            reserve_bytes,
            CONTAINER_VCPU_MILLICORES,
        ));"""
    _reject_mutation(
        verify.CAPACITY,
        capacity_needle,
        """let ignored = budget_fits(
            CONTAINER_MEMORY_BYTES,
            declared_bytes,
            reserve_bytes,
            CONTAINER_VCPU_MILLICORES,
        );
        assert!(true); /* assert!(budget_fits(...)) */""",
    )
    oci_needle = """assert!(
        seeded_hog_budget.is_some_and(|bytes| bytes < OCI_MAX_INFLIGHT_BYTES),
        "per-tenant budget must remain below the global in-flight byte budget"
    );"""
    _reject_mutation(
        verify.OCI_TEST,
        oci_needle,
        """let ignored = seeded_hog_budget.is_some_and(|bytes| bytes < OCI_MAX_INFLIGHT_BYTES);
    assert!(true); /* assert!(seeded_hog_budget.is_some_and(...)) */""",
    )


def test_reviewer_reproducers_reject_ignored_or_wrong_cfg_tests() -> None:
    _reject_mutation(
        verify.STORAGE_1,
        "    #[test]\n    fn physical_cas_bucket_must_match_serving_region",
        "    #[ignore]\n    #[test]\n    fn physical_cas_bucket_must_match_serving_region",
    )
    _reject_mutation(
        verify.STORAGE_3,
        '    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]\n    async fn byok_mode_b_read_fails_closed_when_kms_down',
        '    #[ignore]\n    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]\n    async fn byok_mode_b_read_fails_closed_when_kms_down',
    )
    _reject_mutation(
        verify.STORAGE_3,
        '    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]\n    async fn byok_mode_b_read_fails_closed_when_kms_down',
        '    #[tokio::test(flavor = "current_thread", worker_threads = 2)]\n    async fn byok_mode_b_read_fails_closed_when_kms_down',
    )
    _reject_mutation(
        verify.ORIGIN,
        "#[cfg(test)]\n#[allow(\n    clippy::expect_used,\n    reason = \"tests are allowed to use these primitives\"\n)]\nmod tests_b279_bridge",
        "#[cfg(not(test))] /* #[cfg(test)] */\n#[allow(\n    clippy::expect_used,\n    reason = \"tests are allowed to use these primitives\"\n)]\nmod tests_b279_bridge",
    )
    _reject_mutation(
        verify.MAIN,
        "use boot::{\n    build_runners_resolver,",
        "#[cfg(not(test))]\nuse boot::{\n    build_runners_resolver,",
    )


def test_reviewer_reproducers_reject_stacked_cfg_attributes() -> None:
    """Every test-only seam must reject contradictory/stacked cfg attrs."""
    _reject_mutation(
        verify.FAILOVER,
        "    #[cfg(test)]\n    #[must_use]\n    fn stale_after_ms",
        "    #[cfg(not(test))]\n    #[cfg(test)]\n    #[must_use]\n    fn stale_after_ms",
    )
    _reject_mutation(
        verify.OCI,
        "    #[cfg(test)]\n    fn with_allowlist",
        "    #[cfg(not(test))]\n    #[cfg(test)]\n    fn with_allowlist",
    )
    _reject_mutation(
        verify.SLI,
        "    #[cfg(test)]\n    fn counters_at",
        "    #[cfg(not(test))]\n    #[cfg(test)]\n    fn counters_at",
    )
    _reject_mutation(
        verify.STORAGE_1,
        "    #[test]\n    fn physical_cas_bucket_must_match_serving_region",
        "    #[cfg(not(test))]\n    #[cfg(test)]\n    fn physical_cas_bucket_must_match_serving_region",
    )
    _reject_mutation(
        verify.STORAGE_3,
        '    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]\n    async fn byok_mode_b_read_fails_closed_when_kms_down',
        '    #[cfg(not(test))]\n    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]\n    async fn byok_mode_b_read_fails_closed_when_kms_down',
    )


def test_reviewer_reproducers_reject_conflicting_boot_import_cfg() -> None:
    _reject_mutation(
        verify.MAIN,
        "#[cfg(test)]\nuse boot::{build_runners_resolver_from, build_tier_selector_from};",
        "#[cfg(not(test))]\n#[cfg(test)]\nuse boot::{build_runners_resolver_from, build_tier_selector_from};",
    )


def test_reviewer_reproducers_require_exact_clamp_expression() -> None:
    _reject_mutation(
        verify.CAS_ERASE,
        "max_tenants: max_tenants.clamp(1, DEFAULT_MAX_TENANT_BLOOMS),",
        "max_tenants: max_tenants.clamp(1, DEFAULT_MAX_TENANT_BLOOMS).max(1),",
    )


def test_reviewer_reproducers_require_direct_live_capacity_assertions() -> None:
    deployed = """assert!(budget_fits(
            CONTAINER_MEMORY_BYTES,
            declared_bytes,
            reserve_bytes,
            CONTAINER_VCPU_MILLICORES,
        ));"""
    _reject_mutation(
        verify.CAPACITY,
        deployed,
        "if true {\n            " + deployed.replace("\n", "\n            ") + "\n        }",
    )
    oci = """assert!(
        seeded_hog_budget.is_some_and(|bytes| bytes < OCI_MAX_INFLIGHT_BYTES),
        "per-tenant budget must remain below the global in-flight byte budget"
    );"""
    _reject_mutation(
        verify.OCI_TEST,
        oci,
        "if false {\n        " + oci.replace("\n", "\n        ") + "\n    }",
    )


def test_reviewer_reproducers_bind_billing_assertions_to_their_branches() -> None:
    _reject_mutation(verify.BILLING, "if should_allow {", "if !should_allow {")
    allow = """let is_allow = matches!(outcome.decision, QuotaCasDecision::Allow { .. });
            prop_assert!(is_allow);"""
    _reject_mutation(
        verify.BILLING,
        allow,
        "if true {\n                " + allow.replace("\n", "\n                ") + "\n            }",
    )


def test_reviewer_reproducers_reject_renamed_or_non_test_declarations() -> None:
    _reject_mutation(
        verify.REVOCATION,
        '#[must_use = "handle the result to obtain the configured revocation detector"]',
        '#[must_use = "a reason, but not the canonical one"]',
    )
    _reject_mutation(
        verify.FAILOVER,
        "#[cfg(test)]\n    #[must_use]\n    fn stale_after_ms",
        "#[cfg(not(test))] /* #[cfg(test)] */\n    #[must_use]\n    fn stale_after_ms",
    )
    _reject_mutation(
        verify.OCI,
        "#[cfg(test)]\n    fn with_allowlist",
        "#[cfg(not(test))] /* #[cfg(test)] */\n    fn with_allowlist",
    )
    _reject_mutation(
        verify.REVOCATION,
        "pub fn detector_for_client",
        "pub fn renamed_detector_for_client /* pub fn detector_for_client */",
    )
    _reject_mutation(
        verify.ADAPTER_CACHE,
        "type WriteRecord = (String, String, String, bool);",
        "type RenamedWriteRecord = (String, String, String, bool); /* type WriteRecord = (String, String, String, bool); */",
    )


def test_reviewer_reproducers_reject_removed_revocation_test_include() -> None:
    _reject_mutation(
        verify.REVOCATION,
        'include!("byok_revocation_runtime/part-01.rs");',
        '/* include!("byok_revocation_runtime/part-01.rs"); */',
    )


def test_reviewer_reproducers_reject_registry_population_or_identity_mutation(monkeypatch: pytest.MonkeyPatch) -> None:
    families = dict(verify.FAMILY_PATHS)
    families["B-297 adversarial imports"] = (verify.MAIN,)
    monkeypatch.setattr(verify, "FAMILY_PATHS", families)
    with pytest.raises(verify.VerificationError):
        verify.verify()

    monkeypatch.undo()
    checks = list(verify.CHECKS)
    checks[0], checks[1] = checks[1], checks[0]
    monkeypatch.setattr(verify, "CHECKS", tuple(checks))
    with pytest.raises(verify.VerificationError):
        verify.verify()
