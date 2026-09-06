#!/usr/bin/env python3
"""Mutation checks for the B-102/B-106 bounded serving-path guard."""

from __future__ import annotations

import importlib.util
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_serving_path", ROOT / "scripts/verify_b102_b106_serving_path.py"
)
assert SPEC and SPEC.loader
verify = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verify)


def expect_reject(path: str, marker: str, label: str, replacement: str = "MUTATED") -> None:
    original = (ROOT / path).read_text(encoding="utf-8")
    mutated = original.replace(marker, replacement, 1)
    assert mutated != original, f"mutation marker absent: {path}: {marker}"
    if path == verify.CACHE:
        cache = mutated
        auth = (ROOT / verify.AUTH).read_text(encoding="utf-8")
        quota = (ROOT / verify.QUOTA).read_text(encoding="utf-8")
    elif path == verify.AUTH:
        cache = (ROOT / verify.CACHE).read_text(encoding="utf-8")
        auth = mutated
        quota = (ROOT / verify.QUOTA).read_text(encoding="utf-8")
    else:
        cache = (ROOT / verify.CACHE).read_text(encoding="utf-8")
        auth = (ROOT / verify.AUTH).read_text(encoding="utf-8")
        quota = mutated

    original_read = verify.read
    try:
        verify.read = lambda candidate: {verify.CACHE: cache, verify.AUTH: auth, verify.QUOTA: quota}[candidate]
        verify.verify()
    except verify.VerificationError:
        return
    finally:
        verify.read = original_read
    raise AssertionError(f"mutation was accepted: {path}: {label}")


def expect_comment_wrapped(path: str, marker: str, label: str) -> None:
    expect_reject(path, marker, label, f"/* {marker} */")


def main() -> int:
    verify.verify()
    expect_reject(verify.CACHE, "const inflight = new Map", "single-flight map removed")
    expect_reject(verify.CACHE, "opts.waitUntil(putPromise);", "KV write-behind detached")
    expect_reject(
        verify.CACHE,
        "const KV_PAT_ROW_TTL_S = 30;",
        "KV revocation TTL weakened",
        "const KV_PAT_ROW_TTL_S = 61;",
    )
    expect_reject(verify.AUTH, 'env.CONFIG_DB.withSession("first-unconstrained")', "replica routing removed")
    expect_reject(verify.AUTH, "verifyPatRowCached(readSession, parsed.tokenId, {", "cache call removed")
    expect_reject(verify.QUOTA, "meter && !isStorageMutating && !quotaTier.d1Error", "mutating async meter guard removed")
    expect_comment_wrapped(verify.CACHE, "opts.waitUntil(putPromise);", "KV write-behind comment-wrapped")
    expect_comment_wrapped(verify.AUTH, 'env.CONFIG_DB.withSession("first-unconstrained")', "replica session comment-wrapped")
    expect_comment_wrapped(verify.AUTH, "verifyPatRowCached(readSession, parsed.tokenId, {", "cache call comment-wrapped")
    expect_comment_wrapped(verify.QUOTA, "meter && !isStorageMutating && !quotaTier.d1Error", "mutating predicate comment-wrapped")
    print("B102-B106 serving-path verifier mutations: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
