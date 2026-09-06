#!/usr/bin/env python3
"""Fail-closed contracts for residuals from the frozen D03 Rust bundle."""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

CONTRACTS = (
    ("crates/corelink-container/src/adapter_pat_verifier.rs", ("use futures::FutureExt;",), ("{PatRow, PatRowLookup",)),
    ("crates/corelink-container/src/adapter_pat_gate.rs", ("pub(super) cap: usize,", "///"), ()),
    ("crates/corelink-container/src/adapter_pat.rs", ("use futures::FutureExt;",), ("ARGON2_PERMIT_WAIT, ARGON2_PER_TENANT_PERMITS",)),
    ("crates/corelink-container/src/gc_sweep.rs", (".map_err(PhysicalDeleteError::Backend)?",), ()),
    ("crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs", ("let request_bytes_len = req.bytes.len() as u64;", "fence.commit(lease, request_bytes_len, req.at_unix_ms)"), ()),
    ("crates/corelink-container/src/byte_accounting/b126_m2_test_1_1.rs", ("// In-memory [`ByteStore`]",), ("//! In-memory [`ByteStore`]",)),
    ("crates/corelink-container/src/byte_accounting/b126_m2_test_3_1.rs", ("// Regression net", "CasHandlerError,"), ("//! Regression net",)),
    ("crates/corelink-container/src/byte_accounting/b126_m2_test_4_1.rs", ("// BYOK Wave 3b",), ("//! BYOK Wave 3b",)),
    ("crates/corelink-container/src/routes/pip/tests_support.rs", ("cap_resolver: None,",), ()),
    ("crates/corelink-adapter-host/src/upstream_ssrf.rs", ('resolve("cdn.example.test", *server.address())',), ()),
    ("crates/corelink-container/src/adapter_cache.rs", ("Arc::clone(&map) as Arc<dyn UrlMapStore>",), ()),
    ("crates/corelink-container/src/adapter_pat_tests_1.rs", ("VerifyError::Backend(ref message)",), ()),
)


class VerificationError(RuntimeError):
    pass


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


if __name__ == "__main__":
    try:
        verify()
    except VerificationError as exc:
        raise SystemExit(f"B-282..B-290 BROKEN: {exc}")
    print("B-282..B-290 D03 bundle residuals: PASS")
