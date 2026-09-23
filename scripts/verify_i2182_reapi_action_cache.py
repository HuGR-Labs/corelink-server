#!/usr/bin/env python3
"""Credentialless mutation contract for issue #2182."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
SERVICE = ROOT / "crates/corelink-container/src/reapi_action_cache.rs"
TESTS = ROOT / "crates/corelink-container/src/reapi_action_cache/tests.rs"
PROTO = ROOT / "crates/corelink-reapi/proto/build/bazel/remote/execution/v2/remote_execution.proto"
ROUTES = ROOT / "crates/corelink-container/src/routes/build.rs"
CONCEPT = ROOT / "docs/knowledge/surfaces/reapi-authenticated-action-cache.md"


def violations(service: str, tests: str, proto: str, routes: str, concept: str) -> list[str]:
    errors: list[str] = []
    required_service = (
        "impl ActionCache for ReapiActionCacheService",
        "impl Capabilities for ReapiCacheCapabilitiesService",
        "Access::Read",
        "Access::Write",
        "admitted.ac_lookup(",
        ".ac_update(",
        "validate_action_result",
        "REAPI_ACTION_RESULT_MAX_SERIALIZED_BYTES",
        "Code::AlreadyExists",
        "execution_capabilities: None",
        "DigestFunction::Sha256 as i32",
        "update_enabled: true",
    )
    for item in required_service:
        if item not in service:
            errors.append(f"ActionCache service is missing contract element: {item}")
    if service.count(".ac_lookup(") != 1 or service.count(".ac_update(") != 1:
        errors.append("ActionCache must make exactly one decorated lookup and update call")
    required_proto = (
        "service ActionCache",
        "rpc GetActionResult(GetActionResultRequest) returns (ActionResult);",
        "rpc UpdateActionResult(UpdateActionResultRequest) returns (ActionResult);",
        "message ActionResult",
        "repeated OutputFile output_files = 2;",
        "repeated OutputDirectory output_directories = 3;",
        "ExecutedActionMetadata execution_metadata = 9;",
        "repeated OutputSymlink output_file_symlinks = 10;",
        "repeated OutputSymlink output_directory_symlinks = 11;",
        "repeated OutputSymlink output_symlinks = 12;",
    )
    for item in required_proto:
        if item not in proto:
            errors.append(f"REAPI proto is missing ActionResult wire field: {item}")
    if "ReapiActionCacheService::new" in routes or "ReapiCacheCapabilitiesService::new" in routes:
        errors.append("ActionCache services must remain unmounted pending #2176 and #2183")
    required_tests = (
        "authenticated_round_trip_uses_decorated_action_cache_once_per_rpc",
        "rejected_authorization_tenant_and_malformed_result_never_touch_action_cache",
        "miss_is_not_found_and_immutable_conflict_is_already_exists",
        "quota_and_audit_faults_fail_closed_without_disclosure",
        "capabilities_are_authenticated_sha256_action_cache_only",
    )
    for item in required_tests:
        if item not in tests:
            errors.append(f"ActionCache behavior test is missing: {item}")
    required_concept = (
        'type: "CacheSurface"',
        "crates/corelink-container/src/reapi_action_cache.rs",
        "crates/corelink-container/src/reapi_action_cache/tests.rs",
        "gRPC remains unmounted",
        "ALREADY_EXISTS",
        "no execution",
    )
    for item in required_concept:
        if item not in concept:
            errors.append(f"ActionCache concept is missing contract element: {item}")
    return errors


def self_test() -> list[str]:
    service = SERVICE.read_text(encoding="utf-8")
    tests = TESTS.read_text(encoding="utf-8")
    proto = PROTO.read_text(encoding="utf-8")
    routes = ROUTES.read_text(encoding="utf-8")
    concept = CONCEPT.read_text(encoding="utf-8")
    errors = violations(service, tests, proto, routes, concept)
    mutations = (
        (service.replace("Access::Write", "Access::Read", 1), tests, proto, routes, concept, "write scope bypass"),
        (service.replace("admitted.ac_lookup(", "admitted.cas_read(", 1), tests, proto, routes, concept, "decorated lookup bypass"),
        (service.replace("Code::AlreadyExists", "Code::FailedPrecondition", 1), tests, proto, routes, concept, "immutable conflict remap"),
        (service.replace("execution_capabilities: None", "execution_capabilities: Some(Default::default())", 1), tests, proto, routes, concept, "execution capability claim"),
        (service, tests.replace("quota_and_audit_faults_fail_closed_without_disclosure", "", 1), proto, routes, concept, "fault coverage removed"),
    )
    for mutated_service, mutated_tests, mutated_proto, mutated_routes, mutated_concept, label in mutations:
        if not violations(mutated_service, mutated_tests, mutated_proto, mutated_routes, mutated_concept):
            errors.append(f"mutation was not detected: {label}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    errors = self_test() if args.self_test else violations(
        SERVICE.read_text(encoding="utf-8"),
        TESTS.read_text(encoding="utf-8"),
        PROTO.read_text(encoding="utf-8"),
        ROUTES.read_text(encoding="utf-8"),
        CONCEPT.read_text(encoding="utf-8"),
    )
    result = {"check": "i2182-reapi-action-cache", "ok": not errors, "errors": errors}
    print(json.dumps(result, sort_keys=True) if args.json else "\n".join(errors or ["#2182 ActionCache contract: PASS"]))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
