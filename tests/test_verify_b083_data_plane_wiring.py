from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b083_data_plane_wiring.py"
SPEC = importlib.util.spec_from_file_location("verify_b083_data_plane_wiring", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def _valid_sources() -> tuple[str, str, str, str, str, str, str]:
    builder = "\n".join(
        (
            "crate::byok_orchestrator::make_provider()",
            "D1ByokConfigReader::new",
            "D1ByokSecretReader::new",
            "D1ByokEnvelopeStore::new",
            "handler.with_byok(config, tcs).with_byok_random(mode_b)",
            "return Some(",
        )
    )
    cache = "let cacheable = matches!\nSome(ByokState::Active | ByokState::Partial | ByokState::Shredded)\nguard.remove(tenant)"
    policy = "ByokState::Shredded => ByokEngagement::FailClosed"
    do_start = "\n".join(
        f'ctx.env.{name} ?? ""'
        for name in (
            "CORELINK_BYOK_KMS_ACCESS_KEY_ID",
            "CORELINK_BYOK_KMS_SECRET_ACCESS_KEY",
            "CORELINK_BYOK_KMS_SESSION_TOKEN",
        )
    )
    worker_env = "\n".join(
        f"{name}?: string"
        for name in (
            "CORELINK_BYOK_KMS_ACCESS_KEY_ID",
            "CORELINK_BYOK_KMS_SECRET_ACCESS_KEY",
            "CORELINK_BYOK_KMS_SESSION_TOKEN",
        )
    )
    cargo = 'byok-aws-real = ["corelink-byok/aws"]'
    return builder, builder, cache, policy, do_start, worker_env, cargo


def test_real_provider_feature_is_checked_in_container_manifest() -> None:
    MODULE.verify(*_valid_sources())


def test_missing_real_provider_feature_fails_closed() -> None:
    sources = list(_valid_sources())
    sources[-1] = 'byok-mock = ["corelink-byok/mock"]'
    with pytest.raises(AssertionError, match="real-provider cfg"):
        MODULE.verify(*sources)
