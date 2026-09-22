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
                   "repeated OutputFile output_files = 2",
                   "repeated OutputDirectory output_directories = 3",
                   "int32 exit_code = 4", "bytes stdout_raw = 5",
                   "Digest stdout_digest = 6", "bytes stderr_raw = 7",
                   "Digest stderr_digest = 8",
                   "ExecutedActionMetadata execution_metadata = 9",
                   "repeated OutputSymlink output_file_symlinks = 10",
                   "repeated OutputSymlink output_directory_symlinks = 11",
                   "repeated OutputSymlink output_symlinks = 12"):
        require(proto, marker, "ActionCache wire contract")
    for marker in ("ContentAddressableStorage", "ByteStreamService", "ActionCacheService",
                   "CapabilitiesService(Ingress)", "fn validate_action_result",
                   "result.encoded_len() > MAX_BLOB_BYTES"):
        require(bridge, marker, "shared gRPC service")
    for marker in (".verify_capability(", "require_instance", "DigestAlgo::Sha256",
                   "cache:write scope required", "if write && !can_write",
                   "must use Bearer", "MAX_BLOB_BYTES", "map_cas_error", "audit unavailable",
                   "self.0.charge(&tenant).await?", "if function == 1"):
        require(bridge, marker, "authenticated tenant-scoped ingress guard")
    if bridge.count("self.0.charge(&tenant).await?;") != 6:
        raise AssertionError("quota gate must cover each cache operation")
    for method in ("batch_update_blobs", "batch_read_blobs", "find_missing_blobs",
                   "get_action_result", "update_action_result", "read", "write",
                   "get_capabilities", "query_write_status"):
        try:
            body = bridge.split(f"async fn {method}", 1)[1].split("\n    async fn", 1)[0]
        except IndexError as error:
            raise AssertionError(f"missing RPC method {method}") from error
        require(body, ".authenticate(&request", f"{method} PAT authorization")
        require(body, "require_instance", f"{method} tenant isolation")
    for marker in ("CasService::from_deps", "ByteStreamService::from_deps",
                   "ActionCacheService::from_deps", "CapabilitiesService::from_deps",
                   "Routes::new"):
        require(main, marker, "HTTP/2 tonic mount")
    for marker in ("engine_address = https://corelink-api.humangr.com",
                   "action_cache_address = https://corelink-api.humangr.com",
                   "cas_address = https://corelink-api.humangr.com",
                   "instance_name = replace-with-pat-tenant-id",
                   "Authorization: Bearer $CORELINK_PAT"):
        require(buck, marker, "Buck2 REAPI setting")
    if "[remote_cache]" in buck or "bazel/cache" in buck:
        raise AssertionError("REST remote-cache fallback is present")
    for marker in ("remote_enabled = False", "remote_cache_enabled = True",
                   "ExecutionPlatformInfo"):
        require(platform, marker, "cache-only execution platform policy")
    for marker in ("execution_platforms = prelude//platforms:default root//platforms:corelink-cache",
                   "target_platform_detector_spec = target:root//...->prelude//platforms:default"):
        require(buck, marker, "preserved default platform plus cache platform")


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
        ("bridge", ".verify_capability(", ".verify_capability_removed("),
        ("bridge", "if write && !can_write", "if false && !can_write"),
        ("bridge", "Ingress::require_instance(&body.instance_name, &tenant)?;", "Ok(())?;"),
        ("bridge", "if function == 1", "if function != 1"),
        ("bridge", "self.0.charge(&tenant).await?;", "Ok(());"),
        ("proto", "ExecutedActionMetadata execution_metadata = 9", "metadata_removed"),
        ("main", "ByteStreamService::from_deps", "ByteStreamService::from_deps_removed"),
        ("bridge", "CapabilitiesService(Ingress)", "CapabilitiesService"),
        ("buck", "cas_address = https://corelink-api.humangr.com", "cas_address = https://example.invalid"),
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
