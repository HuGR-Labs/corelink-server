#!/usr/bin/env python3
"""Fail-closed source contract for the B-078 bounded CAS batch reader.

This is intentionally a small structural guard: the route is synchronous at
the handler boundary, so the memory property must be visible in the source
without requiring a live R2 service. The focused Rust tests cover framing,
ordering, derived fanout and terminal task draining; this guard protects the
bounded scheduler and pre-collection cap from a future regression.
"""

from __future__ import annotations

import argparse
from pathlib import Path


class ContractError(RuntimeError):
    pass


def _section(source: str, start: str, end: str) -> str:
    begin = source.find(start)
    if begin < 0:
        raise ContractError(f"missing {start}")
    finish = source.find(end, begin + len(start))
    if finish < 0:
        raise ContractError(f"missing section terminator {end}")
    return source[begin:finish]


def assess_source(route: str, handler: str, storage: str, openapi: str, docs: str) -> list[str]:
    gaps: list[str] = []
    try:
        batch = _section(route, "async fn handle_batch_read", "async fn handle_batch_exists")
    except ContractError as error:
        return [str(error)]

    required_batch = {
        "bounded-buffer": "Semaphore::new(BATCH_READ_FANOUT)" in batch and "acquire_owned()" in batch,
        "spawned-fanout": "tokio::spawn(async move" in batch and "handles.push" in batch,
        "terminal-drain": "abort_and_drain" in batch and "pending.await" in batch,
        "per-read-ceiling": ".with_max_bytes(BATCH_MAX_BYTES as u64)" in batch,
        "request-ceiling": "body.len() > BATCH_REQUEST_BODY_LIMIT_BYTES" in batch,
        "oversize-413": "if payload.len() + bytes.len() > BATCH_MAX_BYTES" in batch and "return batch_too_large()" in batch,
        "aggregate-ceiling": "payload.len() + bytes.len() > BATCH_MAX_BYTES" in batch,
    }
    gaps.extend(name for name, present in required_batch.items() if not present)
    if "let mut handles: Vec<tokio::task::JoinHandle<PerHash>>" in batch and "Vec::with_capacity(BATCH_READ_FANOUT)" not in batch:
        gaps.append("materialized-task-collection")

    if "pub max_bytes: Option<u64>" not in handler:
        gaps.append("request-max-bytes-field")
    if "let actual_bytes = bytes.len() as u64" not in handler or "actual_bytes > limit" not in handler:
        gaps.append("in-memory-pre-collect-check")

    try:
        read = _section(storage, "impl CasReadHandler for R2CasHandler", "impl CasListHandler")
    except ContractError as error:
        gaps.append(str(error))
        read = ""
    if read.count("self.get_capped_for_read(&key, max_bytes)") != 2 or "get_capped(key, max_bytes)" not in storage:
        gaps.append("r2-pre-collection-cap-on-both-paths")
    if "self.client.get(&key)" in read:
        gaps.append("unbounded-r2-read")

    try:
        too_large = _section(route, "fn batch_too_large", "// One manifest line")
    except ContractError as error:
        gaps.append(str(error))
        too_large = ""
    for token in ('Json(serde_json::json!', '"error": "batch_too_large"',
                  '"limit_objects": BATCH_MAX_OBJECTS',
                  '"limit_bytes": BATCH_MAX_BYTES'):
        if token not in too_large:
            gaps.append(f"runtime-413-{token}")

    if "/v1/cas/{tenant}/batch-read:" not in openapi:
        gaps.append("openapi-batch-read-path")
    else:
        operation = openapi[openapi.find("/v1/cas/{tenant}/batch-read:") :]
        operation = operation[: operation.find("\n  /v1/", 1)] if "\n  /v1/" in operation else operation
        for token in ('"413"', "batch_too_large", "application/x-ndjson"):
            if token not in operation:
                gaps.append(f"openapi-{token}")
        response_413 = operation[operation.find('"413":') :]
        for token in (
            "application/json",
            "additionalProperties: false",
            "required: [error, limit_objects, limit_bytes]",
            "error: { type: string, const: batch_too_large }",
            "limit_objects:",
            "limit_bytes:",
        ):
            if token not in response_413:
                gaps.append(f"openapi-413-{token}")

    for token in ("BATCH_READ_FANOUT", "8 MiB", "413", "buffered"):
        if token not in docs:
            gaps.append(f"okf-doc-{token}")
    return gaps


def assess(root: Path) -> list[str]:
    route = (root / "crates/corelink-container/src/routes/cas.rs").read_text(encoding="utf-8")
    route += "\n" + "\n".join(
        p.read_text(encoding="utf-8") for p in (
            root / "crates/corelink-container/src/routes/cas" / n
            for n in (
                "foundation_core.rs",
                "foundation_state.rs",
                "single_setup.rs",
                "single_handlers.rs",
                "batch_write.rs",
                "batch_read.rs",
                "list_delete.rs",
            )
        )
    )
    handler = (root / "crates/corelink-handler-cas/src/request.rs").read_text(encoding="utf-8")
    handler += (root / "crates/corelink-handler-cas/src/handler.rs").read_text(encoding="utf-8")
    storage = (root / "crates/corelink-container/src/storage/r2_s3.rs").read_text(encoding="utf-8")
    storage += "\n" + "\n".join(
        (root / "crates/corelink-container/src/storage/r2_s3_parts" / n).read_text(encoding="utf-8")
        for n in ("client.rs", "cas_core.rs", "cas_ops.rs", "ac_core.rs", "ac_ops.rs")
    )
    openapi = (root / "openapi/corelink-v1.yaml").read_text(encoding="utf-8")
    docs = (root / "docs/knowledge/surfaces/native-cas.md").read_text(encoding="utf-8")
    return assess_source(route, handler, storage, openapi, docs)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    try:
        gaps = assess(args.root)
    except (OSError, UnicodeError) as error:
        print(f"B-078 verifier could not read inputs: {error}")
        return 2
    if gaps:
        print("B-078 OPEN: " + ", ".join(gaps))
        return 1
    print("B-078 DONE: bounded batch-read contract and published truth are present")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
