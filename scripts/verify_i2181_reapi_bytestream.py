#!/usr/bin/env python3
"""Credentialless mutation contract for issue #2181."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
SERVICE = ROOT / "crates/corelink-container/src/reapi_bytestream.rs"
TESTS = ROOT / "crates/corelink-container/src/reapi_bytestream/tests.rs"
ROUTES = ROOT / "crates/corelink-container/src/routes/build.rs"
CONCEPT = ROOT / "docs/knowledge/surfaces/reapi-authenticated-bytestream.md"


def violations(service: str, tests: str, routes: str, concept: str) -> list[str]:
    errors: list[str] = []
    required_service = (
        "impl ByteStream for ReapiByteStreamService",
        "REAPI_BYTESTREAM_MAX_BUFFERED_BYTES",
        "REAPI_BYTESTREAM_CHUNK_BYTES",
        "REAPI_BYTESTREAM_CONCURRENCY_LIMIT",
        "validate_blob_resource_name",
        "Access::Read",
        "Access::Write",
        "admitted.cas_read(",
        "admitted\n            .cas_write(",
        "finish_write",
        "stream.next().await.transpose()?.is_some()",
        "drop(first);",
        "write_offset",
        "try_acquire_owned()",
        "Status::unimplemented(",
    )
    for item in required_service:
        if item not in service:
            errors.append(f"ByteStream service is missing contract element: {item}")
    if service.count(".cas_write(") != 1:
        errors.append("ByteStream must have exactly one decorated CAS write call")
    if "router = router.merge(reapi_bytestream" in routes or "ReapiByteStreamService::new" in routes:
        errors.append("ByteStream service must remain unmounted pending #2176")
    required_tests = (
        "valid_write_calls_decorated_handler_once_and_read_honors_range",
        "missing_invalid_read_only_and_tenant_mismatch_never_reach_cas",
        "malformed_offsets_incomplete_and_hash_or_size_mismatch_fail_before_persistence",
        "quota_and_audit_failures_fail_closed",
        "read_ranges_and_write_size_ceiling_fail_before_cas",
        "terminal_write_requires_eof_and_rejects_delayed_replay_before_persistence",
        "unsupported_resume_never_invents_upload_state",
    )
    for item in required_tests:
        if item not in tests:
            errors.append(f"ByteStream behavior test is missing: {item}")
    required_concept = (
        'type: "CacheSurface"',
        "crates/corelink-container/src/reapi_bytestream.rs",
        "crates/corelink-container/src/reapi_bytestream/tests.rs",
        "gRPC remains unmounted",
        "exactly once",
    )
    for item in required_concept:
        if item not in concept:
            errors.append(f"ByteStream concept is missing contract element: {item}")
    return errors


def self_test() -> list[str]:
    service = SERVICE.read_text(encoding="utf-8")
    tests = TESTS.read_text(encoding="utf-8")
    routes = ROUTES.read_text(encoding="utf-8")
    concept = CONCEPT.read_text(encoding="utf-8")
    errors = violations(service, tests, routes, concept)
    mutations = (
        (service.replace("Access::Write", "Access::Read", 1), tests, routes, concept, "write scope bypass"),
        (service.replace("admitted.cas_read(", "admitted.cas_lookup(", 1), tests, routes, concept, "decorated read bypass"),
        (service.replace("Status::unimplemented(", "Ok(Response::new(QueryWriteStatusResponse { committed_size: 0, complete: false }))", 1), tests, routes, concept, "invented resume state"),
        (service.replace("stream.next().await.transpose()?.is_some()", "false", 1), tests, routes, concept, "missing terminal EOF drain"),
        (service.replace("drop(first);", "", 1), tests, routes, concept, "unbudgeted initial request frame"),
        (service, tests, routes + "\nReapiByteStreamService::new", concept, "public mount"),
        (service, tests.replace("quota_and_audit_failures_fail_closed", "", 1), routes, concept, "failed audit coverage"),
    )
    for mutated_service, mutated_tests, mutated_routes, mutated_concept, label in mutations:
        if not violations(mutated_service, mutated_tests, mutated_routes, mutated_concept):
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
        ROUTES.read_text(encoding="utf-8"),
        CONCEPT.read_text(encoding="utf-8"),
    )
    result = {"check": "i2181-reapi-bytestream", "ok": not errors, "errors": errors}
    print(json.dumps(result, sort_keys=True) if args.json else "\n".join(errors or ["#2181 ByteStream contract: PASS"]))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
