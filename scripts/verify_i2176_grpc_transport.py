#!/usr/bin/env python3
"""Credentialless contract and mutation gate for issue #2176.

The gate intentionally proves only the decision that can be established from
source and public-provider evidence. It rejects any claim that the existing
Fetch proxy is a deployed gRPC transport proof.
"""

from __future__ import annotations

import argparse
import shutil
import sys
import tempfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
DECISION = Path("specs/_audits/2026-09-22-issue-2176-grpc-transport-decision.md")
PROXY = Path("worker/src/durable_object_probes.ts")
WORKFLOW = Path(".github/workflows/issue-2176-grpc-transport-contract.yml")
ROUTES = Path("worker/src/route_match.ts")
WORKER_ENTRY = Path("worker/src/index.ts")
DO_ENTRY = Path("worker/src/durable_object.ts")


class ContractError(RuntimeError):
    pass


def read(root: Path, rel: Path) -> str:
    try:
        return (root / rel).read_text(encoding="utf-8")
    except FileNotFoundError as exc:
        raise ContractError(f"missing required file: {rel}") from exc


def require(text: str, token: str, where: Path) -> None:
    if token not in text:
        raise ContractError(f"{where}: missing required token: {token!r}")


def forbid(text: str, token: str, where: Path) -> None:
    if token in text:
        raise ContractError(f"{where}: forbidden active transport shape: {token!r}")


def validate(root: Path) -> None:
    decision = read(root, DECISION)
    proxy = read(root, PROXY)
    workflow = read(root, WORKFLOW)
    routes = read(root, ROUTES)
    worker_entry = read(root, WORKER_ENTRY)
    do_entry = read(root, DO_ENTRY)

    for token in (
        "**BLOCKED — fail closed.**",
        "private beta",
        "Worker → Durable Object → Container",
        "finish with a nonzero-free `grpc-status: 0` trailer.",
        "bidirectional-streaming RPC",
        "authorization",
        "ALPN is HTTP/2",
        "No REST, HTTP/1, gRPC-Web, local proxy, local cache fallback",
        "No public gRPC/REAPI endpoint",
        "provider mutation or secrets",
        "https://developers.cloudflare.com/workers/reference/protocols/",
        "https://blog.cloudflare.com/grpc-workers/",
        "https://developers.cloudflare.com/durable-objects/api/container/",
    ):
        require(decision, token, DECISION)

    # This is the actual, unproven Fetch bridge. The decisive risk is that it
    # reconstructs a request and uses Fetch rather than a byte socket.
    for token in (
        "const CONTAINER_PORT = 50051",
        "new Request(containerUrl",
        "fetcher.fetch(proxied)",
    ):
        require(proxy, token, PROXY)

    for token in (
        "ref: ${{ github.event.pull_request.head.sha || github.sha }}",
        "persist-credentials: false",
        "runs-on: ubuntu-24.04",
        "python3 -S scripts/verify_i2176_grpc_transport.py --self-test",
        "python3 -S -m unittest -v tests/test_i2176_grpc_transport_contract.py",
    ):
        require(workflow, token, WORKFLOW)

    # A public mount or socket implementation would invalidate this blocked
    # decision. A future implementation must replace this decision in a new,
    # receipt-producing PR rather than silently changing the current bridge.
    for text, path, token in (
        (routes, ROUTES, 'routeKind: "grpc"'),
        (routes, ROUTES, 'routeKind: "reapi"'),
        (worker_entry, WORKER_ENTRY, "async connect("),
        (do_entry, DO_ENTRY, "async connect("),
    ):
        forbid(text, token, path)


def copy_contract_tree(destination: Path) -> None:
    for rel in (DECISION, PROXY, WORKFLOW, ROUTES, WORKER_ENTRY, DO_ENTRY):
        target = destination / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(REPO_ROOT / rel, target)


def expect_rejected(root: Path, rel: Path, old: str, new: str) -> None:
    path = root / rel
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise ContractError(f"self-test setup could not mutate {rel}: {old!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")
    try:
        validate(root)
    except ContractError:
        return
    raise ContractError(f"mutation escaped the contract gate: {rel}: {old!r}")


def self_test() -> None:
    validate(REPO_ROOT)
    cases = (
        (DECISION, "private beta", "generally available"),
        (
            DECISION,
            "finish with a nonzero-free `grpc-status: 0` trailer.",
            "finish without a required terminal trailer.",
        ),
        (PROXY, "fetcher.fetch(proxied)", "return proxied"),
        (WORKFLOW, "persist-credentials: false", "persist-credentials: true"),
        (WORKER_ENTRY, "const handler", "async connect(socket) {}\nconst handler"),
    )
    for rel, old, new in cases:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            copy_contract_tree(root)
            expect_rejected(root, rel, old, new)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
        else:
            validate(REPO_ROOT)
    except ContractError as exc:
        print(f"issue-2176 grpc transport contract: FAIL: {exc}", file=sys.stderr)
        return 1
    print("issue-2176 grpc transport contract: PASS (blocked/fail-closed)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
