#!/usr/bin/env python3
"""Fail-closed guard for the B-322 durable D1 mount repair."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = "crates/corelink-container/src/main.rs"


class VerificationError(RuntimeError):
    """The reviewed Stripe mount no longer binds D1 without a panic."""


def _read(overrides: dict[str, str]) -> str:
    if TARGET in overrides:
        text = overrides[TARGET]
    else:
        path = ROOT / TARGET
        if path.is_symlink() or not path.is_file():
            raise VerificationError(f"missing/non-regular target: {TARGET}")
        text = path.read_text(encoding="utf-8")
    if not isinstance(text, str) or not text or len(text.encode()) > 300_000:
        raise VerificationError("invalid bounded target")
    return text


def verify(*, overrides: dict[str, str] | None = None) -> None:
    source = _read(overrides or {})
    gate = re.compile(
        r'if let\s*\(Ok\(secret\),\s*Some\(client\)\)\s*=\s*'
        r'\(std::env::var\("STRIPE_WEBHOOK_SECRET"\),\s*d1_client\.as_ref\(\)\)'
    )
    if len(gate.findall(source)) != 1:
        raise VerificationError("durable mount must bind secret and D1 client exactly once")
    if 'd1_client.as_ref().expect("guarded by durable D1 mount")' in source:
        raise VerificationError("guarded expect regression")
    if source.count("D1WebhookDlqStore::new(Arc::clone(client))") != 1:
        raise VerificationError("durable DLQ no longer consumes the matched client")
    if re.search(r"#\s*!?\[\s*allow\s*\([^]]*expect_used", source):
        raise VerificationError("targeted expect lint suppression is forbidden")


def self_test() -> None:
    source = _read({})
    mutations = (
        source.replace("Some(client)", "Some(_)", 1),
        source.replace(
            "D1WebhookDlqStore::new(Arc::clone(client))",
            "D1WebhookDlqStore::new(Arc::clone(fallback_client))",
            1,
        ),
        source.replace(
            'info!("billing: DURABLE D1 webhook-DLQ store wired (migrations 0045+0094)");',
            'let client = d1_client.as_ref().expect("guarded by durable D1 mount");',
            1,
        ),
        "#[allow(clippy::expect_used)]\n" + source,
    )
    for index, mutation in enumerate(mutations, 1):
        if mutation == source:
            raise VerificationError(f"self-test mutation {index} changed nothing")
        try:
            verify(overrides={TARGET: mutation})
        except VerificationError:
            continue
        raise VerificationError(f"self-test mutation {index} was accepted")


if __name__ == "__main__":
    try:
        verify()
        if "--self-test" in sys.argv[1:]:
            self_test()
    except (OSError, VerificationError) as exc:
        print(f"B-322 durable D1 mount: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    print("B-322 durable D1 mount: PASS")
