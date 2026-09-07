#!/usr/bin/env python3
"""Fail-closed guard for the B-323 GC binary identity repair."""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SERVER_MANIFEST = "crates/corelink-container/Cargo.toml"
GC_MANIFEST = "crates/corelink-gc/Cargo.toml"
PRODUCTION_BIN = "crates/corelink-container/src/bin/gc_sweep.rs"
FIXTURE_TEST = "crates/corelink-gc/tests/gc_sweep_bin.rs"
DOCKERFILE = "Dockerfile"
TARGETS = (SERVER_MANIFEST, GC_MANIFEST, PRODUCTION_BIN, FIXTURE_TEST, DOCKERFILE)


class VerificationError(RuntimeError):
    """The fixture and production GC executable identities overlap or drifted."""


def _read(path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        text = overrides[path]
    else:
        target = ROOT / path
        if target.is_symlink() or not target.is_file():
            raise VerificationError(f"missing/non-regular target: {path}")
        text = target.read_text(encoding="utf-8")
    if not isinstance(text, str) or not text or len(text.encode()) > 300_000:
        raise VerificationError(f"invalid bounded target: {path}")
    return text


def verify(*, overrides: dict[str, str] | None = None) -> None:
    values = {path: _read(path, overrides or {}) for path in TARGETS}
    try:
        server = tomllib.loads(values[SERVER_MANIFEST])
        gc = tomllib.loads(values[GC_MANIFEST])
    except tomllib.TOMLDecodeError as exc:
        raise VerificationError(f"invalid Cargo manifest: {exc}") from exc
    if server.get("package", {}).get("autobins") is not False:
        raise VerificationError("server auto binary discovery must remain disabled")
    bins = {(item.get("name"), item.get("path")) for item in server.get("bin", [])}
    expected = {
        ("corelink-server", "src/main.rs"),
        ("corelink-gc-sweep-production", "src/bin/gc_sweep.rs"),
    }
    if bins != expected:
        raise VerificationError(f"server binary targets drifted: {sorted(bins)!r}")
    gc_bins = {(item.get("name"), item.get("path")) for item in gc.get("bin", [])}
    if ("gc_sweep", "src/bin/gc_sweep.rs") not in gc_bins:
        raise VerificationError("shipped fixture binary identity drifted")
    fixture = values[FIXTURE_TEST]
    fixture_selector = 'Command::new(env!("CARGO_BIN_EXE_gc_sweep"))'
    if fixture.count(fixture_selector) != 1:
        raise VerificationError("fixture test no longer selects the corelink-gc binary")
    if "\n    stderr: String,\n" not in fixture or "String::from_utf8(out.stderr)" not in fixture:
        raise VerificationError("subprocess stderr is no longer retained")
    production = values[PRODUCTION_BIN]
    if "Promotion into the image remains owner-gated." not in production:
        raise VerificationError("production binary promotion boundary drifted")
    docker = values[DOCKERFILE]
    if "cargo build --release --locked -p corelink-gc --bin gc_sweep;" not in docker:
        raise VerificationError("image no longer builds the reviewed fixture binary")
    if "-p corelink-server --bin corelink-gc-sweep-production" in docker:
        raise VerificationError("owner-gated production binary was promoted implicitly")


def self_test() -> None:
    source = {path: _read(path, {}) for path in TARGETS}
    mutations = (
        (SERVER_MANIFEST, source[SERVER_MANIFEST].replace("autobins = false", "autobins = true", 1)),
        (SERVER_MANIFEST, source[SERVER_MANIFEST].replace("corelink-gc-sweep-production", "gc_sweep", 1)),
        (FIXTURE_TEST, source[FIXTURE_TEST].replace('Command::new(env!("CARGO_BIN_EXE_gc_sweep"))', 'Command::new(env!("CARGO_BIN_EXE_corelink_gc_sweep_production"))', 1)),
        (FIXTURE_TEST, source[FIXTURE_TEST].replace("\n    stderr: String,\n", "\n    diagnostic: String,\n", 1)),
        (PRODUCTION_BIN, source[PRODUCTION_BIN].replace("Promotion into the image remains owner-gated.", "Shipped as gc_sweep.", 1)),
        (DOCKERFILE, source[DOCKERFILE].replace("-p corelink-gc --bin gc_sweep;", "-p corelink-server --bin corelink-gc-sweep-production;", 1)),
    )
    for index, (path, mutation) in enumerate(mutations, 1):
        if mutation == source[path]:
            raise VerificationError(f"self-test mutation {index} changed nothing")
        try:
            verify(overrides={path: mutation})
        except VerificationError:
            continue
        raise VerificationError(f"self-test mutation {index} was accepted")


if __name__ == "__main__":
    try:
        verify()
        if "--self-test" in sys.argv[1:]:
            self_test()
    except (OSError, VerificationError) as exc:
        print(f"B-323 GC binary identity: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    print("B-323 GC binary identity: PASS")
