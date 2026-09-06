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
        (verify.CAS_SINGLE, "clippy::too_many_arguments"),
        (verify.CAS_BATCH, "clippy::too_many_arguments"),
        (verify.CAS_ERASE, "max_tenants.clamp(1, DEFAULT_MAX_TENANT_BLOOMS)"),
        (verify.SLI, ".is_some_and(|bucket|"),
        (verify.ADAPTER_CACHE, "type WriteRecord = (String, String, String, bool);"),
        (verify.REVOCATION, "#[cfg(test)]\n#[allow(clippy::expect_used, clippy::indexing_slicing)]"),
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
