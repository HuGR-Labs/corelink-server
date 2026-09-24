#!/usr/bin/env python3
"""Credentialless mutation contract for issue #2178's unmounted CAS service."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
SERVICE = ROOT / "crates/corelink-container/src/reapi_cas.rs"
INGRESS = ROOT / "crates/corelink-container/src/reapi_ingress.rs"


def violations(service: str, ingress: str) -> list[str]:
    errors: list[str] = []
    required_service = (
        "impl ContentAddressableStorage for CasUnaryService",
        "async fn find_missing_blobs",
        "async fn batch_read_blobs",
        "async fn batch_update_blobs",
        ".ingress\n            .authorize",
        "Access::Read",
        "Access::Write",
        ".cas_read(",
        ".cas_write(",
        "digest_function::Value::Sha256",
        "MAX_BATCH_TOTAL_SIZE_BYTES",
        "MAX_CAS_BLOB_SIZE_BYTES",
        "MAX_FIND_MISSING_BATCH_SIZE",
        "batch_read_dispatch_budget",
        "let dispatch_budget = batch_read_dispatch_budget(&body.digests);",
        "duplicate REAPI digest in batch",
        "Code::NotFound, \"CAS object not found\"",
        "ContentAddressableStorageServer::new(self)",
    )
    for item in required_service:
        if item not in service:
            errors.append(f"CAS service is missing contract element: {item}")
    forbidden = (
        "Router::",
        ".merge(",
        "InMemoryCasHandler",
        "http://",
        "https://",
        "serve(",
    )
    for item in forbidden:
        if item in service:
            errors.append(f"CAS service contains forbidden mount or alternate storage: {item}")
    if "pub mod reapi_cas;" not in (ROOT / "crates/corelink-container/src/lib.rs").read_text():
        errors.append("container module registration is absent")
    if "corelink-reapi = { workspace = true }" not in (
        ROOT / "crates/corelink-container/Cargo.toml"
    ).read_text():
        errors.append("container does not use the repository REAPI proto crate")
    if "pub struct ReapiIngress" not in ingress or "pub async fn authorize" not in ingress:
        errors.append("service is not anchored to the #2177 ingress contract")
    required_ingress = (
        "#[cfg(test)]\n    pub(crate) fn test_only()",
        "#[cfg(test)]\n    #[must_use]\n    pub(crate) fn from_test_components(",
        ".with_max_bytes(max_bytes)",
    )
    for item in required_ingress:
        if item not in ingress:
            errors.append(f"CAS ingress is missing bounded test or read seam: {item}")
    return errors


def self_test() -> list[str]:
    service = SERVICE.read_text(encoding="utf-8")
    ingress = INGRESS.read_text(encoding="utf-8")
    errors = violations(service, ingress)
    mutations = (
        (service.replace(".cas_write(", ".write("), ingress, "decorated write bypass"),
        (service.replace("Access::Write", "Access::Read", 1), ingress, "write authorization bypass"),
        (service.replace("digest_function::Value::Sha256", "digest_function::Value::Sha512", 1), ingress, "digest policy drift"),
        (
            service.replace(
                "let dispatch_budget = batch_read_dispatch_budget(&body.digests);",
                "let dispatch_budget = vec![true; body.digests.len()];",
                1,
            ),
            ingress,
            "batch-read dispatch budget bypass",
        ),
        (service.replace("Code::NotFound, \"CAS object not found\"", "Code::PermissionDenied, \"CAS object not found\""), ingress, "cross-tenant disclosure"),
        (service + "\nRouter::new();", ingress, "public mount"),
        (service, ingress.replace(".with_max_bytes(max_bytes)", "", 1), "bounded read bypass"),
    )
    for mutated_service, mutated_ingress, label in mutations:
        if not violations(mutated_service, mutated_ingress):
            errors.append(f"mutation was not detected: {label}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    errors = self_test() if args.self_test else violations(
        SERVICE.read_text(encoding="utf-8"), INGRESS.read_text(encoding="utf-8")
    )
    result = {"check": "i2178-reapi-cas", "ok": not errors, "errors": errors}
    print(json.dumps(result, sort_keys=True) if args.json else "\n".join(errors or ["#2178 CAS contract: PASS"]))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
