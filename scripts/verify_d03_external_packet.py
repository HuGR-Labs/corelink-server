#!/usr/bin/env python3
"""Semantic owner/runtime guards for D03 B-216 and B-229.

B-216 checks parsed Wrangler queue topology and the active TypeScript Worker
queue method. B-229 checks parsed deploy workflow structure and the active
shell ``REQUIRED`` array. Comments, string decoys, and unrelated YAML steps do
not satisfy these checks. Production evidence remains external, so a valid
repository contract reports ``OWNER_BLOCKED`` until owners attach evidence.

B-170 remains governed by its existing B-087 owner packet. B-226 is
deliberately excluded: its engineering lane is separate from this owner-only
packet and must not be represented as closed here.
"""

from __future__ import annotations

import argparse
import json
import re
import stat
import sys
import tomllib
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]


class PacketError(ValueError):
    """A packet or executable B-216/B-229 contract is absent or weakened."""


def _read(root: Path, relative: str, lane: str) -> str:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        raise PacketError(f"{lane}: missing or non-regular artifact: {relative}")
    try:
        if not stat.S_ISREG(path.stat(follow_symlinks=False).st_mode):
            raise PacketError(f"{lane}: artifact is not a regular file: {relative}")
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise PacketError(f"{lane}: cannot read {relative}: {exc}") from exc


def _require(text: str, markers: tuple[str, ...], lane: str, source: str) -> None:
    folded = " ".join(text.split())
    missing = [marker for marker in markers if marker not in text and " ".join(marker.split()) not in folded]
    if missing:
        raise PacketError(f"{lane}: {source} missing required markers: {', '.join(missing)}")


def _evidence_status(root: Path, paths: tuple[str, ...]) -> dict[str, object]:
    """Report bundle readiness without treating file presence as truth."""
    missing: list[str] = []
    empty: list[str] = []
    for relative in paths:
        path = root / relative
        if path.is_symlink() or not path.is_file():
            missing.append(relative)
            continue
        try:
            if not path.read_text(encoding="utf-8").strip():
                empty.append(relative)
        except (OSError, UnicodeError):
            missing.append(relative)
    blocked = missing + empty
    return {"status": "OWNER_BLOCKED" if blocked else "READY_FOR_BUNDLE", "missing": missing, "empty": empty}


def _strip_ts_comments(text: str) -> str:
    """Blank TS comments while preserving literals and source positions."""
    out = list(text)
    quote: str | None = None
    escaped = False
    i = 0
    while i < len(text):
        c = text[i]
        n = text[i + 1] if i + 1 < len(text) else ""
        if quote:
            if escaped:
                escaped = False
            elif c == "\\":
                escaped = True
            elif c == quote:
                quote = None
            i += 1
            continue
        if c in "'\"`":
            quote = c
            i += 1
            continue
        if c == "/" and n == "/":
            out[i] = out[i + 1] = " "
            i += 2
            while i < len(text) and text[i] != "\n":
                out[i] = " "
                i += 1
            continue
        if c == "/" and n == "*":
            out[i] = out[i + 1] = " "
            i += 2
            while i < len(text):
                if text[i] == "*" and i + 1 < len(text) and text[i + 1] == "/":
                    out[i] = out[i + 1] = " "
                    i += 2
                    break
                if text[i] != "\n":
                    out[i] = " "
                i += 1
            continue
        i += 1
    return "".join(out)


def _mask_ts_literals(text: str) -> str:
    """Blank string/template contents so syntax markers cannot be string bait."""
    out = list(text)
    quote: str | None = None
    escaped = False
    for i, c in enumerate(text):
        if quote:
            if c != "\n":
                out[i] = " "
            if escaped:
                escaped = False
            elif c == "\\":
                escaped = True
            elif c == quote:
                quote = None
            continue
        if c in "'\"`":
            quote = c
            out[i] = " "
    return "".join(out)


def _balanced_body(code: str, signature: str, lane: str) -> tuple[str, int]:
    start = code.find(signature)
    if start < 0:
        raise PacketError(f"{lane}: active Worker signature missing: {signature}")
    brace = code.find("{", start + len(signature))
    if brace < 0:
        raise PacketError(f"{lane}: active Worker body missing: {signature}")
    depth = 0
    for index in range(brace, len(code)):
        if code[index] == "{":
            depth += 1
        elif code[index] == "}":
            depth -= 1
            if depth == 0:
                return code[start : index + 1], start
    raise PacketError(f"{lane}: unterminated Worker body: {signature}")


def _check_active_worker_wiring(root: Path, lane: str) -> None:
    source = _read(root, "apps/signup-worker/src/index.ts", lane)
    without_comments = _strip_ts_comments(source)
    code = _mask_ts_literals(without_comments)
    queue_code, queue_start = _balanced_body(code, "async queue(", lane)
    queue_source = without_comments[queue_start : queue_start + len(queue_code)]
    _require(queue_code, ("await handleErasureDlqBatch(", "await handleErasureQueueBatch("), lane, "active queue callsites")
    if not re.search(r"if\s*\(\s*batch\.queue\s*===\s*", queue_code):
        raise PacketError(f"{lane}: active queue dispatch condition is missing")
    if not re.search(r'if\s*\(\s*batch\.queue\s*===\s*"corelink-dsr-erasure-dlq"\s*\)', queue_source):
        raise PacketError(f"{lane}: DLQ queue name is not active in dispatch condition")
    prefix = code[:queue_start]
    if not re.search(r"import\s*\{[^}]*\bhandleErasureDlqBatch\b", prefix, re.S):
        raise PacketError(f"{lane}: DLQ handler import is not active")


def _check_b216_wrangler(root: Path, lane: str) -> None:
    text = _read(root, "apps/signup-worker/wrangler.toml", lane)
    try:
        config = tomllib.loads(text)
    except tomllib.TOMLDecodeError as exc:
        raise PacketError(f"{lane}: malformed Wrangler TOML: {exc}") from exc
    queues = config.get("queues")
    if not isinstance(queues, dict):
        raise PacketError(f"{lane}: queues table missing")
    producers = queues.get("producers")
    if not isinstance(producers, list) or not any(
        isinstance(item, dict) and item.get("binding") == "DSR_QUEUE" and item.get("queue") == "corelink-dsr-erasure"
        for item in producers
    ):
        raise PacketError(f"{lane}: active DSR_QUEUE producer binding missing")
    consumers = queues.get("consumers")
    if not isinstance(consumers, list):
        raise PacketError(f"{lane}: queue consumers table missing")
    main = [item for item in consumers if isinstance(item, dict) and item.get("queue") == "corelink-dsr-erasure"]
    dlq = [item for item in consumers if isinstance(item, dict) and item.get("queue") == "corelink-dsr-erasure-dlq"]
    if len(main) != 1 or main[0].get("dead_letter_queue") != "corelink-dsr-erasure-dlq":
        raise PacketError(f"{lane}: main DSR consumer lacks the active DLQ binding")
    if len(dlq) != 1:
        raise PacketError(f"{lane}: active DLQ consumer binding missing or duplicated")


def check_b216(root: Path) -> dict[str, object]:
    lane = "B-216"
    packet = _read(root, "docs/internal/b215-b230-runtime-owner-actions.md", lane)
    _require(packet, ("deployed signup-worker revision", "corelink-dsr-erasure-dlq", "on-call destination", "redacted delivery receipt", "controlled exhausted-message observation", "bounded requeue", "final operator disposition"), lane, "runtime owner packet")
    _check_b216_wrangler(root, lane)
    _check_active_worker_wiring(root, lane)
    return _evidence_status(root, ("reports/owner-actions/b216-deployed-signup-worker.json", "reports/owner-actions/b216-alert-delivery.json", "reports/owner-actions/b216-exhausted-observation.md"))


def _required_secret_array(script: str, lane: str) -> None:
    match = re.search(r"(?ms)^REQUIRED=\(\s*\n(?P<body>.*?)^\)\s*$", script)
    if not match:
        raise PacketError(f"{lane}: active REQUIRED array missing")
    body = match.group("body")
    if not re.search(r"(?m)^\s*CLERK_WEBHOOK_SECRET\s*(?:#.*)?$", body):
        raise PacketError(f"{lane}: CLERK_WEBHOOK_SECRET is absent from active REQUIRED array")


def _check_b229_workflow(root: Path, lane: str) -> None:
    workflow_text = _read(root, ".github/workflows/signup-worker-deploy.yml", lane)
    try:
        workflow = yaml.safe_load(workflow_text)
    except yaml.YAMLError as exc:
        raise PacketError(f"{lane}: malformed workflow YAML: {exc}") from exc
    if not isinstance(workflow, dict):
        raise PacketError(f"{lane}: workflow root is not a mapping")
    jobs = workflow.get("jobs")
    deploy = jobs.get("deploy") if isinstance(jobs, dict) else None
    if not isinstance(deploy, dict):
        raise PacketError(f"{lane}: deploy job missing")
    defaults = deploy.get("defaults", {}).get("run", {})
    if not isinstance(defaults, dict) or defaults.get("working-directory") != "apps/signup-worker":
        raise PacketError(f"{lane}: deploy job working-directory is not apps/signup-worker")
    steps = deploy.get("steps")
    if not isinstance(steps, list):
        raise PacketError(f"{lane}: deploy steps missing")
    named: dict[str, list[tuple[int, dict]]] = {}
    for index, step in enumerate(steps):
        if isinstance(step, dict) and isinstance(step.get("name"), str):
            named.setdefault(step["name"], []).append((index, step))
    verify_entries = named.get("Verify required runtime secrets", [])
    deploy_entries = named.get("Deploy Worker", [])
    if len(verify_entries) != 1 or len(deploy_entries) != 1:
        raise PacketError(f"{lane}: required verify/deploy steps missing")
    verify_index, verify = verify_entries[0]
    deploy_index, deploy_step = deploy_entries[0]
    if verify_index >= deploy_index:
        raise PacketError(f"{lane}: runtime secret gate must precede Deploy Worker")
    if verify.get("run") != "bash ../../scripts/verify-signup-worker-secrets.sh":
        raise PacketError(f"{lane}: secret gate run path changed")
    if verify.get("working-directory", "apps/signup-worker") != "apps/signup-worker":
        raise PacketError(f"{lane}: secret gate working-directory escapes signup-worker")
    if deploy_step.get("run") != "wrangler deploy":
        raise PacketError(f"{lane}: deploy step command changed")


def check_b229(root: Path) -> dict[str, object]:
    lane = "B-229"
    packet = _read(root, "docs/internal/b215-b230-runtime-owner-actions.md", lane)
    _require(packet, ("production deploy", "CLERK_WEBHOOK_SECRET", "deploy gate", "gate output is retained", "one real verified Clerk webhook", "signature check"), lane, "runtime owner packet")
    _required_secret_array(_read(root, "scripts/verify-signup-worker-secrets.sh", lane), lane)
    _check_b229_workflow(root, lane)
    return _evidence_status(root, ("reports/owner-actions/b229-production-deploy-gate.json", "reports/owner-actions/b229-verified-webhook.json"))


CHECKS = {"B-216": check_b216, "B-229": check_b229}


def verify(root: Path = ROOT, lane: str | None = None) -> dict[str, dict[str, object]]:
    selected = (lane,) if lane else tuple(CHECKS)
    result: dict[str, dict[str, object]] = {}
    for item in selected:
        if item not in CHECKS:
            raise PacketError(f"unknown or excluded lane: {item}")
        result[item] = CHECKS[item](root)
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--id", choices=tuple(CHECKS))
    args = parser.parse_args(argv)
    try:
        result = verify(ROOT, args.id)
    except (PacketError, OSError, UnicodeError, yaml.YAMLError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
