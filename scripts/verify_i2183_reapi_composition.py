#!/usr/bin/env python3
"""Credentialless composition and unmounted-state contract for #2183."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
COMPOSITION = ROOT / "crates/corelink-container/src/reapi_composition.rs"
MAIN = ROOT / "crates/corelink-container/src/main.rs"


def violations(composition: str, main: str) -> list[str]:
    errors: list[str] = []
    required = (
        "pub fn build_unmounted_cache_only_router(ingress: Option<ReapiIngress>) -> Option<Router>",
        "let ingress = ingress?;",
        "CasUnaryService::new(ingress.clone())",
        "ByteStreamServer::new(ReapiByteStreamService::new(ingress.clone()))",
        "ActionCacheServer::new(ReapiActionCacheService::new(ingress.clone()))",
        "CapabilitiesServer::new(",
        "ReapiCacheCapabilitiesService::new(ingress)",
        "REAPI_MAX_DECODING_MESSAGE_BYTES",
        "Some(server)",
        "does not bind a socket",
    )
    for item in required:
        if item not in composition:
            errors.append(f"cache-only composition is missing: {item}")
    if composition.count(".add_service(") != 4:
        errors.append("composition must register exactly four cache-only services")
    if composition.count(".max_decoding_message_size(REAPI_MAX_DECODING_MESSAGE_BYTES)") != 4:
        errors.append("every cache-only service must use the bounded decode ceiling")
    if composition.count(".max_encoding_message_size(REAPI_MAX_DECODING_MESSAGE_BYTES)") != 4:
        errors.append("every cache-only service must use the bounded encode ceiling")
    forbidden = (
        "ExecutionServer",
        "ExecutionService",
        "serve(",
        "serve_with_incoming",
        "TcpListener",
        "http://",
        "https://",
        "localhost",
    )
    for item in forbidden:
        if item in composition:
            errors.append(f"composition contains a forbidden mount or cache alternative: {item}")
    if "axum::serve(listener, app)" not in main:
        errors.append("the existing Axum listener serve call is missing")
    if "build_unmounted_cache_only_router" in main:
        errors.append("the unmounted composition is wired into the public binary")
    return errors


def self_test() -> list[str]:
    composition = COMPOSITION.read_text(encoding="utf-8")
    main = MAIN.read_text(encoding="utf-8")
    errors = violations(composition, main)
    mutations = (
        (composition.replace("ByteStreamServer::new", "RemovedByteServer::new", 1), main, "missing ByteStream"),
        (composition.replace("CapabilitiesServer::new", "RemovedCapabilities::new", 1), main, "missing Capabilities"),
        (composition.replace("let ingress = ingress?;", "let ingress = ingress.unwrap_or_else(|| panic!());", 1), main, "fail-open absent ingress"),
        (composition.replace(".max_decoding_message_size(REAPI_MAX_DECODING_MESSAGE_BYTES)", "", 1), main, "unbounded request decode"),
        (composition.replace(".max_encoding_message_size(REAPI_MAX_DECODING_MESSAGE_BYTES)", "", 1), main, "unbounded response encode"),
        (composition + "\nExecutionServer::new(service);", main, "execution service"),
        (composition + "\nServer::builder().serve(addr);", main, "public listener"),
        (composition, main.replace("axum::serve(listener, app)", "tokio::spawn(serve_reapi(listener, app))", 1), "lost Axum listener"),
        (composition, main + "\nbuild_unmounted_cache_only_router(dependencies);", "public binary wiring"),
    )
    for mutated_composition, mutated_main, label in mutations:
        if not violations(mutated_composition, mutated_main):
            errors.append(f"mutation was not detected: {label}")
    return errors


def main_cli() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    errors = self_test() if args.self_test else violations(
        COMPOSITION.read_text(encoding="utf-8"), MAIN.read_text(encoding="utf-8")
    )
    result = {"check": "i2183-reapi-composition", "ok": not errors, "errors": errors}
    print(json.dumps(result, sort_keys=True) if args.json else "\n".join(errors or ["#2183 composition contract: PASS"]))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main_cli())
