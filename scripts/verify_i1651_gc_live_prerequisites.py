#!/usr/bin/env python3
"""Credentialless, fail-closed readiness audit for issue #1651.

This verifier deliberately reads repository declarations and redacted staging
evidence only.  It never calls Cloudflare, D1, R2, GitHub, or a GC binary and
never accepts a secret value.  A passing result means that the evidence package
is sufficient to *consider* one owner-approved staging observation; it never
authorizes deletion.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOPOLOGY = ROOT / "infra/staging/topology.json"
READINESS = ROOT / "evidence/staging/readiness.json"
DRY_RUN_WORKFLOW = ROOT / ".github/workflows/gc-sweep-dry-run.yml"
CONTROL_WORKFLOW = ROOT / ".github/workflows/issue-1651-gc-control.yml"

REQUIRED_STAGING_SECRETS = (
    "K6_STAGING_BYOK_CMK_ID",
    "K6_STAGING_MFA_STUB",
    "K6_STAGING_PAT",
    "K6_STAGING_STRIPE_WHSEC",
    "K6_TARGET_HOST",
)


class Blocked(RuntimeError):
    """The live observation boundary is not proven by repository evidence."""


def _text(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        raise Blocked(f"missing/non-regular file: {path.relative_to(ROOT)}")
    return path.read_text(encoding="utf-8")


def _json(path: Path) -> dict:
    try:
        value = json.loads(_text(path))
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        raise Blocked(f"invalid JSON: {path.relative_to(ROOT)} ({exc})") from exc
    if not isinstance(value, dict):
        raise Blocked(f"JSON root must be an object: {path.relative_to(ROOT)}")
    return value


def _secret_names(readiness: dict) -> set[str]:
    """Accept only redacted names, never values, from the evidence package."""
    names = readiness.get("secret_names")
    if isinstance(names, list) and all(isinstance(name, str) for name in names):
        return set(names)
    secrets = readiness.get("secrets")
    if isinstance(secrets, dict) and all(isinstance(name, str) for name in secrets):
        return set(secrets)
    raise Blocked(
        "readiness evidence must list redacted secret names under secret_names "
        "or secrets; secret values are forbidden"
    )


def _verify_observation_workflow(workflow_texts: dict[str, str]) -> None:
    """Require the dedicated staging invocation, not an image-build mention."""
    name = "issue-1651-gc-staging-observation.yml"
    workflow = workflow_texts.get(name)
    if workflow is None:
        raise Blocked(f"missing staging observation workflow: .github/workflows/{name}")

    for marker in (
        "workflow_dispatch:",
        "environment: staging",
        "corelink-gc-sweep-production",
        'GC_OBSERVATION_ONLY: "true"',
        'GC_LIVE_DELETE: "false"',
    ):
        if marker not in workflow:
            raise Blocked(f"staging observation workflow is missing safety marker: {marker}")


def verify() -> None:
    topology = _json(TOPOLOGY)
    blockers: list[str] = []

    if topology.get("deployment_state") != "ready":
        blockers.append(
            "staging deployment_state is not ready "
            f"(found {topology.get('deployment_state', '<missing>')!r})"
        )

    required_origin = "https://staging.corelink.humangr.com"
    if topology.get("canonical_origin") != required_origin:
        blockers.append("staging canonical_origin is not the approved target")

    try:
        readiness = _json(READINESS)
    except Blocked as exc:
        blockers.append(str(exc))
        blockers.append(
            "cannot prove staging secret presence (required redacted names: "
            + ", ".join(REQUIRED_STAGING_SECRETS)
            + ")"
        )
        readiness = None

    if readiness is not None:
        if readiness.get("deployment_state") != "ready":
            blockers.append("staging readiness evidence does not say deployment_state=ready")
        if readiness.get("target_host") != required_origin:
            blockers.append("staging readiness evidence does not bind the canonical target host")
        try:
            missing = sorted(set(REQUIRED_STAGING_SECRETS) - _secret_names(readiness))
        except Blocked as exc:
            blockers.append(str(exc))
        else:
            if missing:
                blockers.append("staging readiness evidence is missing secret names: " + ", ".join(missing))

    dry_run = _text(DRY_RUN_WORKFLOW)
    if 'runs-on: ubuntu-latest' not in dry_run:
        blockers.append("hosted dry-run lane is not pinned to a hosted runner")
    for marker in (
        'GC_LIVE_DELETE: "false"',
        "env -u CLOUDFLARE_API_TOKEN -u CLOUDFLARE_ACCOUNT_ID",
        "deleted_count             = 0",
        "deleted_bytes             = 0",
    ):
        if marker not in dry_run:
            blockers.append(f"hosted dry-run lane is missing safety marker: {marker}")

    control = _text(CONTROL_WORKFLOW)
    for marker in ("CLOUDFLARE_API_TOKEN: \"\"", "CLOUDFLARE_ACCOUNT_ID: \"\"", "R2_TDK_HEX: \"\""):
        if marker not in control:
            blockers.append(f"control lane is missing credentialless marker: {marker}")

    # A live observation must use its dedicated staging lane. The production
    # image build also names the binary, but that is not an observation.
    workflow_texts = {
        path.name: path.read_text(encoding="utf-8")
        for path in (ROOT / ".github/workflows").glob("*.yml")
        if path.is_file() and not path.is_symlink()
    }
    try:
        _verify_observation_workflow(workflow_texts)
    except Blocked as exc:
        blockers.append(str(exc))

    if blockers:
        raise Blocked("\n".join(f"- {item}" for item in blockers))


if __name__ == "__main__":
    try:
        verify()
    except (Blocked, OSError) as exc:
        print("I1651 GC live prerequisites: BLOCKED", file=sys.stderr)
        print(exc, file=sys.stderr)
        raise SystemExit(1)
    print("I1651 GC live prerequisites: PASS (evidence only; deletion remains disabled)")
