#!/usr/bin/env python3
"""Fail-closed structural contract for the final D03 Rust repair bundle."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

# Each marker is intentionally load-bearing. The mutation suite removes every
# one in turn, so comment-only or partial repairs cannot satisfy the guard.
CONTRACTS: tuple[tuple[str, tuple[str, ...], tuple[str, ...]], ...] = (
    ("tests/e2e-tenant-isolation/src/fakes/foundation.rs", ("use uuid::Uuid;",), ()),
    ("crates/corelink-container/src/adapter_pat.rs", (
        '#[path = "adapter_pat_crypto.rs"]\nmod adapter_pat_crypto;',
        '#[path = "adapter_pat_gate.rs"]\nmod adapter_pat_gate;',
        '#[path = "adapter_pat_lookup.rs"]\nmod adapter_pat_lookup;',
        '#[path = "adapter_pat_verifier.rs"]\nmod adapter_pat_verifier;',
    ), ()),
    ("crates/corelink-container/src/storage/r2_s3.rs", ("use super::{byok_cas, StorageEnv};",), ()),
    ("crates/corelink-gc/src/physical_delete.rs", (
        "pub enum PurgeStage {", "fn begin_purge(", "fn claim_retry_epoch(",
        "fn mark_r2_deleted(", "fn mark_r2_retry(", "fn finalize_purge(",
    ), ()),
    ("crates/corelink-container/src/gc_sweep/part-03.rs", (
        "SELECT epoch, state, updated_at FROM gc_purge_intent", "fn begin_purge(",
        "fn claim_retry_epoch(", "fn mark_r2_deleted(", "fn mark_r2_retry(",
        "fn finalize_purge(",
    ), ()),
    ("crates/corelink-container/src/routes/dsr/portal/part-00.rs", (
        "super::super::d1util::d1_query_blocking", "super::super::access::run_access",
        "super::super::access::run_portability", "super::super::access::run_rectification",
    ), ()),
    ("crates/corelink-container/src/routes/dsr/portal/part-00-01.rs", (
        "super::super::build_audit_r2_client",
    ), ()),
    ("crates/corelink-container/src/routes/tier_select/part-00-00.rs", (
        "crate::routes::tier_select_store::D1HttpTierSelectStore",
        "crate::routes::tier_select_checkout::StripeCheckoutCreator",
        "crate::routes::tier_select_audit::TierSelectAuditAdapter",
    ), ()),
    ("crates/corelink-container/src/routes/tier_select/part-00-01.rs", (
        "crate::routes::admin::resolve_internal_auth_key",
    ), ()),
    ("tests/e2e-tenant-isolation/src/fakes/extended.rs", (), ("use corelink_audit::",)),
    ("tests/e2e-tenant-isolation/src/fakes/stores.rs", (), ("use corelink_audit::",)),
    ("crates/corelink-container/src/routes/audit_archive.rs", ("let row = Value::Object(row);",), ()),
    ("crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02.rs", (
        "let row = Value::Object(row);", "parse_sealed_epoch_metadata(&row)?",
    ), ()),
    ("crates/corelink-container/src/origin_timing.rs", ("slot.replace(previous);",), ("previous.flatten()",)),
    ("crates/corelink-container/src/sli_aggregate.rs", (
        "let mut result: BTreeMap<Sli, SliCounters> = BTreeMap::new();",
    ), ()),
    ("crates/corelink-container/src/routes/public_pullthrough.rs", ("pub(crate) use implementation::*;",), ()),
    ("crates/corelink-container/src/storage/cas_write_fence.rs", (), ("use std::time::Duration;",)),
)

MODULE_PATHS: tuple[tuple[str, str, str], ...] = (
    ("crates/corelink-audit/src/events.rs", "tests", "tests.rs"),
    ("crates/corelink-billing-stripe-materializer/src/d1.rs", "tests", "tests.rs"),
    ("crates/corelink-byok/src/byok_aws/real.rs", "tests", "tests.rs"),
    ("crates/corelink-byok/src/byok_gcp/real.rs", "tests", "tests.rs"),
    ("crates/corelink-cf-bindings/src/d1_real.rs", "tests", "tests.rs"),
    ("crates/corelink-clerk-cf/src/clerk_health_do.rs", "tests", "tests.rs"),
    ("crates/corelink-enterprise-inquiry/src/ledger.rs", "tests", "tests.rs"),
    ("crates/corelink-gc/src/run.rs", "run_tests", "run_tests.rs"),
    ("crates/corelink-gc/src/sweep_runner.rs", "sweep_runner_tests", "sweep_runner_tests.rs"),
    ("crates/corelink-handler-cas/src/handler.rs", "tests", "tests.rs"),
    ("crates/corelink-ops/src/oncall/events.rs", "tests", "tests.rs"),
    ("crates/corelink-rate-headers/src/circuit.rs", "tests", "tests.rs"),
    ("crates/corelink-ratelimit/src/limiter.rs", "tests", "tests.rs"),
    ("crates/corelink-stripe-real/src/webhook_dispatch.rs", "tests", "tests.rs"),
    ("crates/corelink-telemetry/src/lighthouse.rs", "tests", "tests.rs"),
    ("crates/corelink-telemetry/src/logpush/redaction.rs", "tests", "tests.rs"),
    ("tests/e2e-user-journeys/src/journeys/adapters.rs", "adapters_auth", "adapters_auth.rs"),
)


class VerificationError(RuntimeError):
    """A D03 repair contract is absent or ambiguous."""


def _read(root: Path, path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        return overrides[path]
    try:
        return (root / path).read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"missing input: {path}") from exc


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    overrides = overrides or {}
    for path, required, forbidden in CONTRACTS:
        source = _read(root, path, overrides)
        for marker in required:
            if marker not in source:
                raise VerificationError(f"{path}: missing {marker!r}")
        for marker in forbidden:
            if marker in source:
                raise VerificationError(f"{path}: forbidden {marker!r}")

    manifest_path = "crates/corelink-container/Cargo.toml"
    manifest = _read(root, manifest_path, overrides)
    rand_line = 'rand = "0.8"'
    if manifest.count(rand_line) != 1:
        raise VerificationError("container rand dependency must occur exactly once")
    dev = manifest.find("[dev-dependencies]")
    if dev < 0 or manifest.find(rand_line) > dev:
        raise VerificationError("container rand must be a production dependency")

    for parent, module, sibling in MODULE_PATHS:
        source = _read(root, parent, overrides)
        binding = f'#[path = "{sibling}"]\nmod {module};'
        if source.count(binding) != 1:
            raise VerificationError(f"{parent}: must bind sibling {sibling} exactly once")
        if not (root / parent).with_name(sibling).is_file():
            raise VerificationError(f"{parent}: missing sibling {sibling}")


if __name__ == "__main__":
    try:
        verify()
    except VerificationError as exc:
        raise SystemExit(f"B-270..B-281 BROKEN: {exc}")
    print("B-270..B-281 D03 bundle repairs: PASS")
