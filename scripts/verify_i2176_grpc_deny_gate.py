#!/usr/bin/env python3
"""Trusted, source-only admission gate for issue #2176.

This program deliberately lives on the protected base branch.  The
``pull_request_target`` workflow checks out a candidate tree separately and
passes it here; this verifier never imports, executes, or builds candidate
code.  It proves only that gRPC ingress stays closed until a separately
reviewed receipt-producing change replaces this gate.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path


class ContractError(RuntimeError):
    """The candidate no longer satisfies the trusted deny contract."""


INDEX = Path("worker/src/index_fetch.ts")
GATE = Path("worker/src/grpc_transport_gate.ts")
CONTRACT = Path("specs/03_architecture/issue-2176-grpc-transport-contract.md")


def read(root: Path, relative: Path) -> str:
    try:
        return (root / relative).read_text(encoding="utf-8")
    except FileNotFoundError as error:
        raise ContractError(f"missing required file: {relative}") from error


def require(text: str, token: str, location: Path) -> int:
    position = text.find(token)
    if position < 0:
        raise ContractError(f"{location}: missing required token {token!r}")
    return position


def validate(root: Path) -> None:
    index = read(root, INDEX)
    gate = read(root, GATE)
    contract = read(root, CONTRACT)

    import_position = require(
        index,
        'import { rejectUnprovenGrpcTransport } from "./grpc_transport_gate.js";',
        INDEX,
    )
    fetch_position = require(index, "async fetch(request: Request", INDEX)
    gate_call_position = require(
        index,
        "const grpcTransportGate = rejectUnprovenGrpcTransport(request);",
        INDEX,
    )
    return_position = require(
        index,
        "if (grpcTransportGate !== null) return grpcTransportGate;",
        INDEX,
    )
    first_pipeline_step = require(index, "const requestStart = Date.now();", INDEX)
    if not (import_position < fetch_position < gate_call_position < return_position < first_pipeline_step):
        raise ContractError(
            f"{INDEX}: gRPC deny must run before every Fetch/DO pipeline stage"
        )

    for token in (
        'const GRPC_MEDIA_TYPE_PREFIX = "application/grpc";',
        'request.headers.get("content-type")',
        "mediaType.startsWith(GRPC_MEDIA_TYPE_PREFIX)",
        'error: "GRPC_TRANSPORT_UNAVAILABLE"',
        "status: 503",
        '"cache-control": "no-store"',
        '"content-type": "application/json; charset=utf-8"',
    ):
        require(gate, token, GATE)

    for forbidden in (
        "fetch(",
        "authorization",
        "request.text",
        "request.json",
        "request.arrayBuffer",
    ):
        if forbidden in gate:
            raise ContractError(f"{GATE}: deny gate must not inspect or reflect {forbidden!r}")

    for token in (
        "**BLOCKED — no public gRPC claim.**",
        "Worker → Durable Object → Container",
        "raw TCP socket",
        "application/grpc",
        "authorization",
        "binary metadata",
        "grpc-status",
        "trailers",
        "protected-environment runtime receipt",
        "No REST, HTTP/1, gRPC-Web, local proxy, or local cache fallback.",
    ):
        require(contract, token, CONTRACT)


def assert_exact_head(candidate: Path, expected_head: str) -> None:
    actual_head = subprocess.run(
        ["git", "-C", str(candidate), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if actual_head != expected_head:
        raise ContractError(
            f"candidate checkout is {actual_head}, expected pull-request head {expected_head}"
        )


def write_fixture(root: Path) -> None:
    files = {
        INDEX: '''import { rejectUnprovenGrpcTransport } from "./grpc_transport_gate.js";\n\nexport const baseHandler = {\n  async fetch(request: Request) {\n    const grpcTransportGate = rejectUnprovenGrpcTransport(request);\n    if (grpcTransportGate !== null) return grpcTransportGate;\n    const requestStart = Date.now();\n    return new Response(String(requestStart));\n  },\n};\n''',
        GATE: '''const GRPC_MEDIA_TYPE_PREFIX = "application/grpc";\n\nexport function rejectUnprovenGrpcTransport(request: Request): Response | null {\n  const mediaType = request.headers.get("content-type")?.split(";", 1)[0]?.trim().toLowerCase();\n  if (mediaType === undefined || !mediaType.startsWith(GRPC_MEDIA_TYPE_PREFIX)) return null;\n  return new Response(JSON.stringify({ error: "GRPC_TRANSPORT_UNAVAILABLE" }), {\n    status: 503,\n    headers: {\n      "cache-control": "no-store",\n      "content-type": "application/json; charset=utf-8",\n    },\n  });\n}\n''',
        CONTRACT: """# Issue #2176 transport contract\n\n**BLOCKED — no public gRPC claim.**\n\nThe required path is Worker → Durable Object → Container over a raw TCP socket.\nIt must preserve application/grpc, authorization, binary metadata, grpc-status, and trailers.\nA protected-environment runtime receipt is required.\n\nNo REST, HTTP/1, gRPC-Web, local proxy, or local cache fallback.\n""",
    }
    for relative, content in files.items():
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")


def self_test() -> None:
    with tempfile.TemporaryDirectory() as directory:
        fixture = Path(directory)
        write_fixture(fixture)
        validate(fixture)
        mutations = (
            (INDEX, "if (grpcTransportGate !== null) return grpcTransportGate;", ""),
            (GATE, '"cache-control": "no-store"', '"cache-control": "public, max-age=600"'),
            (GATE, "mediaType.startsWith(GRPC_MEDIA_TYPE_PREFIX)", "mediaType === GRPC_MEDIA_TYPE_PREFIX"),
            (CONTRACT, "protected-environment runtime receipt", "unverified deployment claim"),
        )
        for relative, old, new in mutations:
            target = fixture / relative
            original = target.read_text(encoding="utf-8")
            if old not in original:
                raise ContractError(f"self-test fixture lost mutation anchor {old!r}")
            target.write_text(original.replace(old, new, 1), encoding="utf-8")
            try:
                validate(fixture)
            except ContractError:
                pass
            else:
                raise ContractError(f"mutation escaped the deny gate: {relative}: {old!r}")
            target.write_text(original, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--expected-head")
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    try:
        if arguments.self_test:
            self_test()
        elif arguments.candidate is not None and arguments.expected_head is not None:
            assert_exact_head(arguments.candidate, arguments.expected_head)
            validate(arguments.candidate)
        else:
            raise ContractError("pass --self-test or both --candidate and --expected-head")
    except (ContractError, subprocess.CalledProcessError) as error:
        print(f"issue-2176 trusted deny gate: FAIL: {error}", file=sys.stderr)
        return 1
    print("issue-2176 trusted deny gate: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
