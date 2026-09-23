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
IMPORT_ANCHOR = 'import { runScheduled } from "./index_schedule.js";\n'
DENY_IMPORT = 'import { rejectUnprovenGrpcTransport } from "./grpc_transport_gate.js";\n'
FETCH_OPENING = "  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {\n"
FIRST_PIPELINE_STEP = "    const requestStart = Date.now();\n"
DENY_INSERTION = (
    "    const grpcTransportGate = rejectUnprovenGrpcTransport(request);\n"
    "    if (grpcTransportGate !== null) return grpcTransportGate;\n"
)
GATE_SOURCE = '''const GRPC_MEDIA_TYPE_PREFIX = "application/grpc";

/**
 * Refuse every native-gRPC media type at the public Fetch boundary until the
 * separately reviewed socket transport has a protected runtime receipt.
 *
 * The prefix deliberately includes `application/grpc+proto` and gRPC-Web so
 * neither content-type variant becomes a protocol-conversion escape hatch.
 */
export function rejectUnprovenGrpcTransport(request: Request): Response | null {
  const mediaType = request.headers
    .get("content-type")
    ?.split(";", 1)[0]
    ?.trim()
    .toLowerCase();
  if (mediaType === undefined || !mediaType.startsWith(GRPC_MEDIA_TYPE_PREFIX)) return null;

  return new Response(JSON.stringify({ error: "GRPC_TRANSPORT_UNAVAILABLE" }), {
    status: 503,
    headers: {
      "cache-control": "no-store",
      "content-type": "application/json; charset=utf-8",
    },
  });
}
'''


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


def expected_index(trusted_base: Path) -> str:
    base_index = read(trusted_base, INDEX)
    if base_index.count(IMPORT_ANCHOR) != 1:
        raise ContractError(f"{INDEX}: trusted base no longer has the known import anchor")
    with_import = base_index.replace(IMPORT_ANCHOR, IMPORT_ANCHOR + DENY_IMPORT)
    insertion_point = FETCH_OPENING + FIRST_PIPELINE_STEP
    if with_import.count(insertion_point) != 1:
        raise ContractError(
            f"{INDEX}: trusted base no longer has the known fetch entrypoint"
        )
    return with_import.replace(insertion_point, FETCH_OPENING + DENY_INSERTION + FIRST_PIPELINE_STEP)


def validate(candidate: Path, trusted_base: Path) -> None:
    index = read(candidate, INDEX)
    gate = read(candidate, GATE)
    contract = read(candidate, CONTRACT)

    expected = expected_index(trusted_base)
    if index != expected:
        raise ContractError(
            f"{INDEX}: candidate must equal trusted base plus the canonical early deny insertion"
        )

    if gate != GATE_SOURCE:
        raise ContractError(f"{GATE}: candidate must equal the canonical no-inspection deny implementation")

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


def write_fixture(root: Path, trusted_base: Path) -> None:
    base_index = (
        IMPORT_ANCHOR
        + "\n"
        "export const baseHandler = {\n"
        + FETCH_OPENING
        + FIRST_PIPELINE_STEP
        + "    return new Response(String(requestStart));\n  },\n};\n"
    )
    base_target = trusted_base / INDEX
    base_target.parent.mkdir(parents=True, exist_ok=True)
    base_target.write_text(base_index, encoding="utf-8")
    files = {
        INDEX: expected_index(trusted_base),
        GATE: GATE_SOURCE,
        CONTRACT: """# Issue #2176 transport contract\n\n**BLOCKED — no public gRPC claim.**\n\nThe required path is Worker → Durable Object → Container over a raw TCP socket.\nIt must preserve application/grpc, authorization, binary metadata, grpc-status, and trailers.\nA protected-environment runtime receipt is required.\n\nNo REST, HTTP/1, gRPC-Web, local proxy, or local cache fallback.\n""",
    }
    for relative, content in files.items():
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")


def self_test() -> None:
    with tempfile.TemporaryDirectory() as directory:
        fixture = Path(directory)
        trusted_base = fixture / "trusted-base"
        candidate = fixture / "candidate"
        write_fixture(candidate, trusted_base)
        validate(candidate, trusted_base)
        mutations = (
            (INDEX, "if (grpcTransportGate !== null) return grpcTransportGate;", ""),
            (INDEX, "    const requestStart = Date.now();", "    await fetch(\"https://invalid.example\");\n    const requestStart = Date.now();"),
            (GATE, '"cache-control": "no-store"', '"cache-control": "public, max-age=600"'),
            (GATE, "return new Response", "return null;\n  // return new Response"),
            (CONTRACT, "protected-environment runtime receipt", "unverified deployment claim"),
        )
        for relative, old, new in mutations:
            target = candidate / relative
            original = target.read_text(encoding="utf-8")
            if old not in original:
                raise ContractError(f"self-test fixture lost mutation anchor {old!r}")
            target.write_text(original.replace(old, new, 1), encoding="utf-8")
            try:
                validate(candidate, trusted_base)
            except ContractError:
                pass
            else:
                raise ContractError(f"mutation escaped the deny gate: {relative}: {old!r}")
            target.write_text(original, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trusted-base", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--expected-head")
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    try:
        if arguments.self_test:
            self_test()
        elif (
            arguments.trusted_base is not None
            and arguments.candidate is not None
            and arguments.expected_head is not None
        ):
            assert_exact_head(arguments.candidate, arguments.expected_head)
            validate(arguments.candidate, arguments.trusted_base)
        else:
            raise ContractError(
                "pass --self-test or --trusted-base, --candidate, and --expected-head"
            )
    except (ContractError, subprocess.CalledProcessError) as error:
        print(f"issue-2176 trusted deny gate: FAIL: {error}", file=sys.stderr)
        return 1
    print("issue-2176 trusted deny gate: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
