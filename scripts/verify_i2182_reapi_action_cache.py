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


def violations(service: str, tests: str, proto: str, routes: str) -> list[str]:
    errors: list[str] = []
    required_service = (
        "impl ActionCache for ReapiActionCacheService",
        "impl Capabilities for ReapiCacheCapabilitiesService",
        "Access::Read",
        "Access::Write",
        "admitted.ac_lookup(",
        ".ac_update(",
        "validate_action_result",
        "validate_output_path",
        "REAPI_ACTION_RESULT_MAX_SERIALIZED_BYTES",
        "Code::AlreadyExists",
        "execution_capabilities: None",
        "digest_function::Value::Sha256 as i32",
        "update_enabled: true",
    )
    for item in required_service:
        if item not in service:
            errors.append(f"ActionCache service is missing contract element: {item}")
    if service.count("validate_output_path(&output.path)?;") != 3:
        errors.append("ActionCache must validate every output file, directory, and symlink path")
    if service.count(".ac_lookup(") != 1 or service.count(".ac_update(") != 1:
        errors.append("ActionCache must make exactly one decorated lookup and update call")
    for item in ("Router::", ".merge(", "InMemory", "http://", "https://", "serve("):
        if item in service:
            errors.append(f"ActionCache service contains forbidden mount or alternate storage: {item}")
    required_proto = (
        "service ActionCache",
        "rpc GetActionResult(GetActionResultRequest) returns (ActionResult);",
        "rpc UpdateActionResult(UpdateActionResultRequest) returns (ActionResult);",
        "message ActionResult",
        "repeated OutputFile output_files = 2;",
        "repeated OutputDirectory output_directories = 3;",
        "ExecutedActionMetadata execution_metadata = 9;",
        "repeated OutputSymlink output_file_symlinks = 10",
        "repeated OutputSymlink output_directory_symlinks = 11",
        "repeated OutputSymlink output_symlinks = 12;",
        "repeated string inline_output_files = 5;",
        "DigestFunction.Value digest_function = 6;",
        "ResultsCachePolicy results_cache_policy = 4;",
        "DigestFunction.Value digest_function = 5;",
        "NodeProperties node_properties = 7;",
        "NodeProperties node_properties = 4;",
        "string worker = 1;",
        "google.protobuf.Timestamp queued_timestamp = 2;",
        "google.protobuf.Timestamp worker_start_timestamp = 3;",
        "repeated google.protobuf.Any auxiliary_metadata = 11;",
        "google.protobuf.Duration virtual_execution_duration = 12;",
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
    return errors


def self_test() -> list[str]:
    service = SERVICE.read_text(encoding="utf-8")
    tests = TESTS.read_text(encoding="utf-8")
    proto = PROTO.read_text(encoding="utf-8")
    routes = ROUTES.read_text(encoding="utf-8")
    errors = violations(service, tests, proto, routes)
    mutations = (
        (service.replace("Access::Write", "Access::Read", 1), tests, proto, routes, "write scope bypass"),
        (service.replace("admitted.ac_lookup(", "admitted.cas_read(", 1), tests, proto, routes, "decorated lookup bypass"),
        (service.replace("Code::AlreadyExists", "Code::FailedPrecondition", 1), tests, proto, routes, "immutable conflict remap"),
        (service.replace("validate_output_path(&output.path)?;", "", 1), tests, proto, routes, "output path validation bypass"),
        (service.replace("execution_capabilities: None", "execution_capabilities: Some(Default::default())", 1), tests, proto, routes, "execution capability claim"),
        (service, tests.replace("quota_and_audit_faults_fail_closed_without_disclosure", "", 1), proto, routes, "fault coverage removed"),
    )
    for mutated_service, mutated_tests, mutated_proto, mutated_routes, label in mutations:
        if not violations(mutated_service, mutated_tests, mutated_proto, mutated_routes):
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
    )
    result = {"check": "i2182-reapi-action-cache", "ok": not errors, "errors": errors}
    print(json.dumps(result, sort_keys=True) if args.json else "\n".join(errors or ["#2182 ActionCache contract: PASS"]))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
