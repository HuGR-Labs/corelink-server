#!/usr/bin/env python3
"""Credentialless readiness audit for issue #1650/B-068.

This verifier reads only the reviewed manifest, workflow text, and redacted
owner packet. It never reads Actions secrets, contacts a provider, dispatches a
workflow, or authorizes a real harness. ``blocked`` is the expected state until
an owner records every protected input, isolated resource, and cleanup receipt.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
PACKET = ROOT / "docs/handoff/2026-09-22-i1650-real-integration-readiness.json"
MANIFEST = ROOT / "scripts/real-ignored-harness-manifest.json"
WORKFLOW = ROOT / ".github/workflows/real-ignored-harnesses.yml"
PROFILES = ("d1", "r2", "stripe", "neon")
FORBIDDEN = ("-----BEGIN ", "github_pat_", "ghp_", "gho_", "sk_live_", "whsec_")


def fail(message: str) -> None:
    raise ValueError(message)


def walk(value: Any, path: str = "packet") -> None:
    if isinstance(value, str):
        upper = value.upper()
        for marker in FORBIDDEN:
            if marker.upper() in upper:
                fail(f"{path} contains credential material")
    elif isinstance(value, dict):
        for key, child in value.items():
            walk(child, f"{path}.{key}")
    elif isinstance(value, list):
        for i, child in enumerate(value):
            walk(child, f"{path}[{i}]")


def load(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read {path.relative_to(ROOT)}: {exc}")
    if not isinstance(value, dict):
        fail(f"{path.relative_to(ROOT)} must contain an object")
    return value


def validate() -> bool:
    packet = load(PACKET)
    manifest = load(MANIFEST)
    workflow = WORKFLOW.read_text(encoding="utf-8")
    walk(packet)
    if packet.get("schema_version") != "i1650.real-integration-readiness.v1":
        fail("unsupported packet schema")
    if packet.get("issue") != 1650 or packet.get("credentialless") is not True:
        fail("packet is not bound to issue 1650 and credentialless")
    if packet.get("status") not in {"blocked", "ready"}:
        fail("packet status must be blocked or ready")
    if packet.get("redaction") != {"secret_values_recorded": False, "private_material_present": False}:
        fail("redaction contract drifted")
    policy = packet.get("dispatch_policy")
    required_policy = {"manual_only", "protected_ref", "environment", "approval_required", "dispatch_authorized", "production_mutation_allowed"}
    if not isinstance(policy, dict) or set(policy) != required_policy:
        fail("dispatch policy is incomplete")
    if policy != {**policy, "manual_only": True, "protected_ref": "refs/heads/main", "environment": "real-integration", "approval_required": True, "dispatch_authorized": False, "production_mutation_allowed": False}:
        fail("dispatch policy is not fail-closed")
    profiles = packet.get("profiles")
    manifest_profiles = manifest.get("profiles")
    if not isinstance(profiles, dict) or set(profiles) != set(PROFILES):
        fail("profile set drifted")
    if not isinstance(manifest_profiles, dict) or set(manifest_profiles) != set(PROFILES):
        fail("executor manifest profile set drifted")
    for profile in PROFILES:
        item = profiles[profile]
        expected = list(manifest_profiles[profile].get("required_env", ()))
        env = item.get("environment")
        if item.get("status") not in {"blocked", "ready"} or not isinstance(env, list):
            fail(f"{profile} packet shape is invalid")
        if [entry.get("name") for entry in env] != expected:
            fail(f"{profile} environment inventory does not match executor manifest")
        if any(set(entry) != {"name", "kind", "scope", "status"} for entry in env):
            fail(f"{profile} environment entry shape drifted")
        if any(entry["status"] not in {"missing", "verified"} for entry in env):
            fail(f"{profile} environment status is invalid")
        resources = item.get("resources")
        cleanup = item.get("cleanup")
        if not isinstance(resources, list) or not resources or not isinstance(cleanup, list) or not cleanup or any(not isinstance(x, str) or not x.strip() for x in cleanup):
            fail(f"{profile} resource/cleanup inventory is incomplete")
        blockers = item.get("blockers")
        if not isinstance(blockers, list) or any(not isinstance(x, str) or not x.strip() for x in blockers):
            fail(f"{profile} blockers are invalid")
        ready = item["status"] == "ready"
        if ready and (blockers or any(x["status"] != "verified" for x in env)):
            fail(f"{profile} claims ready without verified inputs")
    if packet["status"] == "ready" and any(profiles[p]["status"] != "ready" for p in PROFILES):
        fail("packet claims ready while a profile is blocked")

    # The real executor itself must remain a manual, protected, credentialed
    # lane. This audit does not run it and does not accept automatic triggers.
    if not re.search(r"(?m)^\s{2}workflow_dispatch:\s*(?:\{\})?\s*$", workflow):
        fail("real executor is missing workflow_dispatch")
    for event in ("pull_request", "pull_request_target", "push", "schedule", "workflow_call"):
        if re.search(rf"(?m)^\s{{2}}{event}:\s*", workflow):
            fail(f"real executor has automatic trigger: {event}")
    for marker in ('environment: real-integration', 'test "$GITHUB_REF" = "refs/heads/main"', 'persist-credentials: false', 'python3 scripts/verify_real_ignored_harnesses.py'):
        if marker not in workflow:
            fail(f"real executor missing safety marker: {marker}")
    if re.search(r"(?m)^\s*if:\s*.*secrets\.", workflow):
        fail("workflow condition interpolates a secret")
    # No owner packet can authorize the secret PAT seed harness.
    if any(token in workflow for token in ("CORELINK_PAT_SIGNING_KEY_HEX", "emit_e2e_seed", "PAT_PLAINTEXT", "SEED_SQL")):
        fail("forbidden PAT seed input reached the real executor")
    return packet["status"] == "ready"


def main() -> int:
    try:
        ready = validate()
    except (OSError, ValueError) as exc:
        print(f"I1650 real integration readiness: CONTRACT FAILED: {exc}", file=sys.stderr)
        return 1
    if ready:
        print("I1650 real integration readiness: READY (owner approval and dispatch still required)")
    else:
        print("I1650 real integration readiness: BLOCKED (missing external resources/credentials/cleanup evidence)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
