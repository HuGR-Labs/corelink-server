#!/usr/bin/env python3
"""Credentialless source and mutation gate for issue #2067."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise AssertionError(f"missing {label}: {needle!r}")


def verify(files: dict[str, str]) -> None:
    proto = files["proto"]
    bridge = files["bridge"]
    main = files["main"]
    buck = files["buck"]
    platform = files["platform"]
    for marker in ("service ActionCache", "message ActionResult",
                   "ExecutedActionMetadata execution_metadata = 9"):
        require(proto, marker, "ActionCache wire contract")
    for marker in ("ContentAddressableStorage", "ByteStreamService", "ActionCacheService"):
        require(bridge, marker, "shared gRPC service")
    for marker in ("verify_capability", "require_instance", "DigestAlgo::Sha256",
                   "cache:write scope required", "MAX_BLOB_BYTES", "map_cas_error"):
        require(bridge, marker, "authenticated tenant-scoped ingress guard")
    for marker in ("CasService::from_deps", "ByteStreamService::from_deps",
                   "ActionCacheService::from_deps", "Routes::new"):
        require(main, marker, "HTTP/2 tonic mount")
    for marker in ("engine_address =", "action_cache_address =", "cas_address =",
                   "instance_name =", "Authorization: Bearer $CORELINK_PAT"):
        require(buck, marker, "Buck2 REAPI setting")
    if "[remote_cache]" in buck or "bazel/cache" in buck:
        raise AssertionError("REST remote-cache fallback is present")
    for marker in ("remote_enabled = False", "remote_cache_enabled = True",
                   "ExecutionPlatformInfo"):
        require(platform, marker, "cache-only execution platform policy")


def read_files() -> dict[str, str]:
    return {
        "proto": (ROOT / "crates/corelink-reapi/proto/build/bazel/remote/execution/v2/remote_execution.proto").read_text(),
        "bridge": (ROOT / "crates/corelink-container/src/grpc_reapi.rs").read_text(),
        "main": (ROOT / "crates/corelink-container/src/main.rs").read_text(),
        "buck": (ROOT / "examples/buck2-starter/.buckconfig").read_text(),
        "platform": (ROOT / "examples/buck2-starter/platforms/defs.bzl").read_text(),
    }


def mutations_fail(files: dict[str, str]) -> None:
    mutations = (
        ("bridge", "verify_capability", "verify_capability_removed"),
        ("bridge", "DigestAlgo::Sha256", "DigestAlgo::Blake3"),
        ("proto", "ExecutedActionMetadata execution_metadata = 9", "metadata_removed"),
        ("main", "ByteStreamService::from_deps", "ByteStreamService::from_deps_removed"),
        ("buck", "cas_address =", "rest_address ="),
        ("platform", "remote_enabled = False", "remote_enabled = True"),
    )
    for file_name, old, new in mutations:
        mutated = dict(files)
        mutated[file_name] = mutated[file_name].replace(old, new, 1)
        try:
            verify(mutated)
        except AssertionError:
            continue
        raise AssertionError(f"mutation survived: {file_name}: {old!r}")


if __name__ == "__main__":
    current = read_files()
    verify(current)
    mutations_fail(current)
    print("issue-2067 REAPI bridge contract and mutations: PASS")
