#!/usr/bin/env python3
"""Static scope and fail-closed checks for the BYOK ownership writer child."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTROL = ROOT / "crates/corelink-container/src/byok_control_transition.rs"
TESTS = ROOT / "crates/corelink-container/src/byok_control_transition_ownership_tests.rs"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


control = CONTROL.read_text(encoding="utf-8")
tests = TESTS.read_text(encoding="utf-8")

prepare = control.split("pub async fn prepare_activation_with_context(", 1)[1].split(
    "/// Cancel only an unpublished activation.", 1
)[0]
commit = control.split("async fn commit_activation_preparation(", 1)[1].split(
    "async fn activation_request(", 1
)[0]

require(
    "self.prepare_activation_with_context(activation, now_ms, None)" in control,
    "ordinary BYOK callers must preserve the None path",
)
require(
    prepare.index("validate_byok_context(context)?")
    < prepare.index("self.active_config_identity("),
    "supplied context must be scenario-checked before BYOK reads or writes",
)
require(
    "StagingLoadTestResourceClass::ByokArtifact" in control
    and "StagingLoadTestDisposition::Disposable" in control,
    "activation must use the canonical BYOK class and explicit disposable treatment",
)
require(
    "statements.push(byok_ownership_statement(context, &intent_id, now_ms)?)" in commit,
    "ownership registration must be part of the activation's D1 batch",
)
require(
    tests.count("#[tokio::test") >= 2
    and "synthetic_activation_is_registered_atomically_and_replay_fails_closed" in tests
    and "wrong_scenario_and_registration_failure_cannot_commit_activation" in tests,
    "writer-level loopback proof must cover registration, replay, mismatch, and rollback",
)
require(
    "synthetic-wrapped-secret" in tests
    and "assert!(!handle.contains(TENANT))" in tests
    and "assert!(!handle.contains(\"synthetic-wrapped-secret\"))" in tests,
    "opaque handle checks must exclude tenant identity and wrapped secret material",
)

print("issue #2583 BYOK ownership scope and negative-control checks passed")
