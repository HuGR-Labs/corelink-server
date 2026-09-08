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


# Keep the source contract on the live include!()-assembled CAS modules. The
# historical monolithic units no longer exist after the source split.
CAS_ROUTE_PARTS = (
    "foundation_core.rs",
    "foundation_state.rs",
    "single_setup.rs",
    "single_handlers.rs",
    "batch_write.rs",
    "batch_read.rs",
    "list_delete.rs",
)


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

    # Keep the three sibling native-CAS batch routes closed as one surface.
    # Checking only the batch-read path let the write and existence routes
    # disappear from the published contract without reopening this gate.
    route_contracts = {
        "CAS_BATCH_ROUTE": ("/v1/cas/{tenant}/batch", "handle_batch_write"),
        "CAS_BATCH_READ_ROUTE": ("/v1/cas/{tenant}/batch-read", "handle_batch_read"),
        "CAS_BATCH_EXISTS_ROUTE": ("/v1/cas/{tenant}/batch-exists", "handle_batch_exists"),
    }
    for constant, (path, handler_name) in route_contracts.items():
        if f'pub const {constant}: &str = "{path}"' not in route:
            gaps.append(f"source-{constant}")
        registration = f"{constant},\n            post({handler_name})"
        if registration not in route:
            gaps.append(f"source-registration-{constant}")

    try:
        batch = _section(route, "async fn handle_batch_read", "async fn handle_batch_exists")
    except ContractError as error:
        return [str(error)]

    required_batch = {
        "bounded-buffer": "Semaphore::new(BATCH_READ_FANOUT)" in batch and "acquire_owned()" in batch,
        "spawned-fanout": "tokio::spawn(async move" in batch and "tasks.push" in batch,
        "terminal-drain": (
            "tasks.abort_and_drain().await" in batch
            and "pending.await" in route
            and "impl Drop for BatchReadTaskGuard" in route
        ),
        "cancellation-lease": (
            "struct BatchReadLease" in route
            and "Arc::new(BatchReadLease" in batch
            and "let lease = Arc::clone(&lease)" in batch
            and "let response_lease = Arc::clone(&lease)" in batch
        ),
        "per-read-ceiling": ".with_max_bytes(BATCH_MAX_BYTES as u64)" in batch,
        "request-ceiling": "body.len() > BATCH_REQUEST_BODY_LIMIT_BYTES" in batch,
        "oversize-413": "if payload.len() + bytes.len() > BATCH_MAX_BYTES" in batch and "return batch_too_large()" in batch,
        "aggregate-ceiling": "payload.len() + bytes.len() > BATCH_MAX_BYTES" in batch,
    }
    gaps.extend(name for name, present in required_batch.items() if not present)
    if "BatchReadTaskGuard" in batch and "BatchReadTaskGuard::with_capacity(BATCH_READ_FANOUT)" not in batch:
        gaps.append("materialized-task-collection")

    try:
        drop_guard = _section(route, "impl Drop for BatchReadTaskGuard", "async fn handle_batch_read")
    except ContractError as error:
        gaps.append("drop-guard")
    else:
        if "for pending in &self.handles" not in drop_guard or "pending.abort();" not in drop_guard:
            gaps.append("drop-abort")

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
        capped = _section(storage, "pub async fn get_capped", "pub async fn head_size")
        capped += _section(storage, "async fn collect_capped_body<S", "pub async fn new(")
    except ContractError as error:
        gaps.append(str(error))
        capped = ""
    if "body.next_chunk()" not in capped:
        gaps.append("bounded-r2-body-loop")
    if "actual_bytes > max_bytes" not in capped:
        gaps.append("bounded-r2-body-ceiling")
    if ".body\n                    .collect()" in capped or ".body.collect()" in capped:
        gaps.append("unbounded-get-capped-collect")
    for token, gap in (
        ("timeout_config(", "r2-timeout-config"),
        (".connect_timeout(", "r2-connect-timeout"),
        (".read_timeout(", "r2-read-timeout"),
        (".operation_attempt_timeout(", "r2-attempt-timeout"),
        (".operation_timeout(", "r2-operation-timeout"),
        ("Self::R2_BODY_IDLE_TIMEOUT", "bounded-r2-body-idle-timeout"),
        ("Self::R2_BODY_TOTAL_TIMEOUT", "bounded-r2-body-total-timeout"),
        ("tokio::time::timeout(idle_timeout, body.next_chunk())", "bounded-r2-body-idle-loop"),
        ("tokio::time::timeout(total_timeout, collect)", "bounded-r2-body-total-loop"),
    ):
        if token not in storage:
            gaps.append(gap)

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

    def openapi_path_block(path: str) -> str:
        marker = f"  {path}:"
        start = openapi.find(marker)
        if start < 0:
            return ""
        end = openapi.find("\n  /", start + len(marker))
        return openapi[start:] if end < 0 else openapi[start:end]

    expected_operations = {
        "/v1/cas/{tenant}/batch": ("casBatchWrite", "batch"),
        "/v1/cas/{tenant}/batch-read": ("casBatchRead", "batch-read"),
        "/v1/cas/{tenant}/batch-exists": ("casBatchExists", "batch-exists"),
    }
    for path, (operation_id, label) in expected_operations.items():
        operation = openapi_path_block(path)
        if not operation:
            gaps.append(f"openapi-{label}-path")
            continue
        for token in ("post:", f"operationId: {operation_id}", '"413"', '"415"', '"429"'):
            if token not in operation:
                gaps.append(f"openapi-{operation_id}-{token.rstrip(':').replace('"', '')}")

    batch_read = openapi_path_block("/v1/cas/{tenant}/batch-read")
    if batch_read:
        for token in ("application/x-ndjson", "application/x-hugit-cas-batch"):
            if token not in batch_read:
                gaps.append(f"openapi-{token}")

    # The shared 413 response is a named component so all three routes cannot
    # drift on caps or error shape independently.
    response_start = openapi.find("    BatchTooLarge:")
    response_end = openapi.find("    Unauthorized:", response_start + 1)
    response_413 = openapi[response_start:response_end] if response_start >= 0 and response_end >= 0 else ""
    schema_start = openapi.find("    BatchTooLargeResponse:")
    schema_end = openapi.find("    BatchUploadResult:", schema_start + 1)
    batch_schema = openapi[schema_start:schema_end] if schema_start >= 0 and schema_end >= 0 else ""
    for token in ("application/json", "BatchTooLargeResponse"):
        if token not in response_413:
            gaps.append(f"openapi-413-{token}")
    for token in (
        "additionalProperties: false",
        "required: [error, limit_objects, limit_bytes]",
        "error: { type: string, const: batch_too_large }",
        "limit_objects:",
        "limit_bytes:",
    ):
        if token not in batch_schema:
            gaps.append(f"openapi-413-{token}")

    for token in ("BatchUploadResult", "BatchExistsResult", "BatchHashNdjsonBody"):
        if token not in openapi:
            gaps.append(f"openapi-schema-{token}")

    for token in ("BATCH_READ_FANOUT", "8 MiB", "413", "buffered", "R2 timeout", "lease"):
        if token not in docs:
            gaps.append(f"okf-doc-{token}")
    return gaps


def assess(root: Path) -> list[str]:
    route = (root / "crates/corelink-container/src/routes/cas.rs").read_text(encoding="utf-8")
    route += "\n" + "\n".join(
        p.read_text(encoding="utf-8") for p in (
            root / "crates/corelink-container/src/routes/cas" / n
            for n in CAS_ROUTE_PARTS
        )
    )
    handler = (root / "crates/corelink-handler-cas/src/request.rs").read_text(encoding="utf-8")
    handler += (root / "crates/corelink-handler-cas/src/handler.rs").read_text(encoding="utf-8")
    storage = (root / "crates/corelink-container/src/storage/r2_s3.rs").read_text(encoding="utf-8")
    storage += "\n" + "\n".join(
        (root / "crates/corelink-container/src/storage/r2_s3_parts" / n).read_text(encoding="utf-8")
        for n in ("client.rs", "client_impl.rs", "cas_core.rs", "cas_ops.rs", "ac_core.rs", "ac_ops.rs")
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
