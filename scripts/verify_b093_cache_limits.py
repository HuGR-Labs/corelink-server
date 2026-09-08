#!/usr/bin/env python3
"""Fail-closed B-093 proof for the shared HTTP cache-entry ceiling.

The verifier is intentionally source-local and bounded: it checks the one
canonical constant, every cache HTTP route that can buffer an entry, the REST
bridge declaration, and the published OpenAPI/backlog contract.  Its mutation
checks prove that weakening any one of those edges is rejected.
"""

from __future__ import annotations

import argparse
import re
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIMIT = 64 * 1024 * 1024

# Keep this tied to the primary CAS route chain.  The CAS module is assembled
# from several `include!` units and rustfmt may split the call across lines;
# a literal substring check would report a false regression after formatting.
_CAS_PRIMARY_ROUTE = re.compile(
    r"\.route\s*\(\s*CAS_READ_ROUTE\b.*?"
    r"\.route\s*\(\s*CAS_LIST_ROUTE\b",
    re.S,
)
_CAS_WRITE_GUARD = re.compile(
    r"DefaultBodyLimit\s*::\s*max\s*\(\s*"
    r"corelink_hash::CACHE_ENTRY_MAX_BYTES\s*,?\s*\)",
    re.S,
)
_CAS_WRITE_HANDLER = re.compile(r"\.put\s*\(\s*handle_write\s*\)", re.S)


class VerificationError(RuntimeError):
    pass


def fail(message: str) -> None:
    raise VerificationError(message)


def read(path: Path) -> str:
    if not path.is_file():
        fail(f"missing B-093 proof input: {path.relative_to(ROOT)}")
    return path.read_text(encoding="utf-8")


def source() -> dict[str, str]:
    # The CAS route was intentionally split into bounded source units.  Read
    # the include graph as one logical route so a path-only refactor cannot
    # make the guard silently stop observing the live handlers.
    cas_parts = ("foundation.rs", "single.rs", "batch.rs", "list_delete.rs")
    cas = "\n".join(
        read(ROOT / "crates/corelink-container/src/routes/cas" / part)
        for part in cas_parts
    )
    return {
        "hash": read(ROOT / "crates/corelink-hash/src/lib.rs"),
        "cas": cas,
        "bazel": "\n".join(
            read(ROOT / "crates/corelink-container/src/routes/bazel_v2" / part)
            for part in ("part-00.rs", "part-01.rs")
        ),
        "turbo": "\n".join(
            read(ROOT / "crates/corelink-container/src/routes/turbo_v8" / part)
            for part in ("b126_m2_impl_01.rs", "b126_m2_impl_02.rs")
        ),
        "bridge": read(ROOT / "crates/corelink-bazel-bridge/src/lib.rs"),
        "openapi_yaml": read(ROOT / "openapi/corelink-v1.yaml"),
        "openapi_json": read(ROOT / "openapi/corelink-v1.json"),
        "docs_yaml": read(ROOT / "apps/docs/static/openapi-corelink-v1.yaml"),
        "worker_openapi": read(ROOT / "worker/src/lib/openapi_v1.ts"),
        "backlog": read(ROOT / "BACKLOG.md"),
    }


def assess(files: dict[str, str], expected_status: str = "done") -> None:
    canonical = files["hash"]
    if not re.search(
        r"pub const CACHE_ENTRY_MAX_BYTES: usize = 64 \* 1024 \* 1024;",
        canonical,
    ):
        fail("canonical cache-entry ceiling is not exactly 64 MiB")

    cas = files["cas"]
    if "CAS_READ_MAX_OBJECT_BYTES: u64 = corelink_hash::CACHE_ENTRY_MAX_BYTES as u64" not in cas:
        fail("native CAS read ceiling is not sourced from the canonical limit")
    route_match = _CAS_PRIMARY_ROUTE.search(cas)
    if not route_match:
        fail("native CAS read/write route chain is missing")
    primary_route = route_match.group(0)
    if not _CAS_WRITE_HANDLER.search(primary_route):
        fail("native CAS write handler is not mounted on the primary CAS route")
    if not _CAS_WRITE_GUARD.search(primary_route):
        fail("native CAS write route lost its 64 MiB body guard")
    if "BATCH_REQUEST_BODY_LIMIT_BYTES" not in cas or "DefaultBodyLimit::max" not in cas:
        fail("native CAS batch route lost its bounded request-body guard")
    if "pub const BATCH_MAX_BYTES: usize = 8 * 1024 * 1024;" not in cas:
        fail("native CAS batch byte ceiling is not exactly 8 MiB")
    try:
        batch_write = cas[cas.index("async fn handle_batch_write") : cas.index("async fn handle_batch_read")]
    except ValueError:
        fail("native CAS batch write handler is missing")
    for marker, message in (
        ("total.checked_add(entry.len)", "batch manifest sum is not overflow checked"),
        ("usize::try_from(entry.len)", "batch manifest length conversion is not checked"),
        ("offset.checked_add(entry_len)", "batch payload offset is not overflow checked"),
        ("let Some(slice) = payload.get(offset..end) else", "batch payload slicing is not fail-closed"),
    ):
        if marker not in batch_write:
            fail(message)
    if ".sum()" in batch_write or "unwrap_or(&[])" in batch_write:
        fail("batch framing uses a wrapping sum or fallback slice")

    bazel = files["bazel"]
    if bazel.count("DefaultBodyLimit::max(CACHE_ENTRY_MAX_BYTES)") < 3:
        fail("Bazel REST write routes do not all carry the 64 MiB body guard")
    if "findMissingBlobs" not in bazel:
        fail("Bazel find-missing route disappeared from the assessed surface")

    turbo = files["turbo"]
    if "pub const TURBO_BODY_LIMIT_BYTES: usize = 100 * 1024 * 1024;" not in turbo:
        fail("Turborepo ceiling is not sourced from the current bounded capacity contract")
    if "DefaultBodyLimit::max(TURBO_BODY_LIMIT_BYTES)" not in turbo:
        fail("Turborepo artifact router lost its body guard")

    bridge = files["bridge"]
    if "MAX_BLOB_SIZE_BYTES: u64 = corelink_hash::CACHE_ENTRY_MAX_BYTES as u64" not in bridge:
        fail("Bazel bridge declared size ceiling is not canonical")

    for key, marker in (
        ("openapi_yaml", "maxLength: 67108864"),
        ("docs_yaml", "maxLength: 67108864"),
        ("openapi_json", '"maxLength": 67108864'),
        ("worker_openapi", r'\"maxLength\": 67108864'),
    ):
        if files[key].count(marker) != 10:
            fail(f"{key} does not contain exactly the ten 64 MiB cache boundaries")

    backlog = files["backlog"]
    match = re.search(r"### B-093 .*?(?=\n### B-094 )", backlog, re.S)
    if not match:
        fail("B-093 backlog section is missing")
    section = match.group(0)
    if f"status: {expected_status}" not in section:
        fail(f"B-093 backlog status is not {expected_status!r}")
    if "DD-037" not in section:
        fail("B-093 no longer preserves its DD-037 source mapping")
    if "source-finding: DD-037" not in section:
        fail("B-093 source-finding field is not grounded in DD-037")
    if "source-finding-document: docs/security/2026-06-15-launch-due-diligence-audit.md" not in section:
        fail("B-093 source-finding document is missing")
    if "verify_b093_cache_limits.py" not in section:
        fail("B-093 backlog verification is not wired to the executable guard")


def mutation_checks(files: dict[str, str]) -> None:
    # Mutate the guard in-place regardless of rustfmt's line wrapping.  This
    # is the behavioral/static tooth for the exact residual that originally
    # escaped: a 10 MiB route override must fail the source proof.
    cas = files["cas"]
    route_match = _CAS_PRIMARY_ROUTE.search(cas)
    primary_route = route_match.group(0) if route_match else ""
    handler = _CAS_WRITE_HANDLER.search(primary_route)
    if not handler:
        fail("mutation fixture for native CAS handler binding did not match the source")
    mutant = dict(files)
    mutant["cas"] = (
        cas[: route_match.start()]
        + primary_route.replace(handler.group(0), ".put(handle_read)", 1)
        + cas[route_match.end() :]
    )
    try:
        assess(mutant)
    except VerificationError:
        pass
    else:
        fail("mutation unexpectedly passed: native CAS handler binding")

    guard = _CAS_WRITE_GUARD.search(primary_route)
    if not guard:
        fail("mutation fixture for native CAS override did not match the source")
    mutant = dict(files)
    replacement = re.sub(
        r"corelink_hash::CACHE_ENTRY_MAX_BYTES",
        "10 * 1024 * 1024",
        guard.group(0),
        count=1,
    )
    route_offset = route_match.start() if route_match else 0
    route_end = route_match.end() if route_match else 0
    mutant["cas"] = (
        cas[:route_offset]
        + primary_route.replace(guard.group(0), replacement, 1)
        + cas[route_end:]
    )
    try:
        assess(mutant)
    except VerificationError:
        pass
    else:
        fail("mutation unexpectedly passed: native CAS override")

    mutants = (
        ("canonical limit", "hash", "64 * 1024 * 1024", "63 * 1024 * 1024"),
        ("native CAS read source", "cas", "CAS_READ_MAX_OBJECT_BYTES: u64 = corelink_hash::CACHE_ENTRY_MAX_BYTES as u64", "CAS_READ_MAX_OBJECT_BYTES: u64 = 63 * 1024 * 1024"),
        ("Bazel override", "bazel", "DefaultBodyLimit::max(CACHE_ENTRY_MAX_BYTES)", "DefaultBodyLimit::max(10 * 1024 * 1024)"),
        ("Turbo source", "turbo", "TURBO_BODY_LIMIT_BYTES: usize = 100 * 1024 * 1024", "TURBO_BODY_LIMIT_BYTES: usize = 99 * 1024 * 1024"),
        ("bridge source", "bridge", "MAX_BLOB_SIZE_BYTES: u64 = corelink_hash::CACHE_ENTRY_MAX_BYTES as u64", "MAX_BLOB_SIZE_BYTES: u64 = 4 * 1024 * 1024 * 1024"),
        ("batch byte limit", "cas", "pub const BATCH_MAX_BYTES: usize = 8 * 1024 * 1024;", "pub const BATCH_MAX_BYTES: usize = 64 * 1024 * 1024;"),
        ("batch sum overflow guard", "cas", "total.checked_add(entry.len)", "total + entry.len"),
        ("batch length conversion guard", "cas", "usize::try_from(entry.len)", "entry.len as usize"),
        ("batch offset overflow guard", "cas", "offset.checked_add(entry_len)", "offset + entry_len"),
        ("batch slice guard", "cas", "let Some(slice) = payload.get(offset..end) else", "let slice = payload.get(offset..end).unwrap_or(&[]);"),
        ("canonical OpenAPI YAML", "openapi_yaml", "maxLength: 67108864", "maxLength: 67108863"),
        ("generated OpenAPI JSON", "openapi_json", '"maxLength": 67108864', '"maxLength": 67108863'),
        ("docs static OpenAPI YAML", "docs_yaml", "maxLength: 67108864", "maxLength: 67108863"),
        ("Worker OpenAPI", "worker_openapi", r'\"maxLength\": 67108864', r'\"maxLength\": 67108863'),
        ("DD-037 mapping", "backlog", "DD-037", "DD-000"),
    )
    for name, key, old, new in mutants:
        if old not in files[key]:
            fail(f"mutation fixture for {name} did not match the source")
        mutant = dict(files)
        replace_count = -1 if name in ("Bazel override", "DD-037 mapping") else 1
        mutant[key] = files[key].replace(old, new, replace_count)
        try:
            assess(mutant)
        except VerificationError:
            continue
        fail(f"mutation unexpectedly passed: {name}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--expect", default="done", choices=("open", "done"))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    files = source()
    assess(files, args.expect)
    if args.self_test:
        mutation_checks(files)
    print(f"B-093 {args.expect}: {LIMIT} byte HTTP cache-entry ceiling; mutations red")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except VerificationError as error:
        raise SystemExit(f"FAIL: {error}")
