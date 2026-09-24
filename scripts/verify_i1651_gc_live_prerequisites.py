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
OBSERVATION_WORKFLOW = "issue-1651-gc-staging-observation.yml"

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


def _block(source: list[str], header: str, indent: int) -> list[str] | None:
    """Return one indentation-scoped YAML block without accepting comments."""
    expected = " " * indent + header
    try:
        start = source.index(expected)
    except ValueError:
        return None

    result: list[str] = []
    for line in source[start + 1 :]:
        if len(line) - len(line.lstrip()) <= indent:
            break
        result.append(line)
    return result


def _verify_observation_workflow(workflow_texts: dict[str, str]) -> None:
    """Require a dedicated, bounded staging observation lane.

    Image-build references are not execution evidence.  This checks only a
    future dedicated lane's declarative safety boundary; it never dispatches
    that lane or accepts it as provider or owner evidence.
    """
    workflow = workflow_texts.get(OBSERVATION_WORKFLOW)
    if workflow is None:
        raise Blocked(
            "missing dedicated staging observation workflow: "
            f".github/workflows/{OBSERVATION_WORKFLOW}"
        )

    lines = [
        line.rstrip()
        for line in workflow.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    triggers = _block(lines, "on:", 0)
    if triggers != ["  workflow_dispatch:"]:
        raise Blocked("staging observation workflow must have only a manual trigger")

    permissions = _block(lines, "permissions:", 0)
    if permissions != ["  contents: read"]:
        raise Blocked("staging observation workflow must grant contents: read only")

    jobs = _block(lines, "jobs:", 0)
    if jobs is None or [line for line in jobs if len(line) - len(line.lstrip()) == 2] != [
        "  observe:"
    ]:
        raise Blocked("staging observation workflow must define only the observe job")
    job = _block(jobs, "observe:", 2)
    required_job_lines = (
        "    runs-on: ubuntu-24.04",
        "    environment: staging",
        "    timeout-minutes: 5",
    )
    if job is None or any(job.count(line) != 1 for line in required_job_lines):
        raise Blocked("staging observation job is missing its bounded staging controls")
    if any(line.strip().startswith(("if:", "continue-on-error:")) for line in job):
        raise Blocked("staging observation job cannot be conditional or suppress errors")

    steps = _block(job, "steps:", 4)
    step_header = "      - name: Invoke production GC observation"
    step_headers = (
        [line for line in steps if len(line) - len(line.lstrip()) == 6]
        if steps is not None
        else []
    )
    if step_headers != [step_header]:
        raise Blocked("staging observation job must have one production observation step")
    step = _block(steps, "- name: Invoke production GC observation", 6)
    required_step_lines = (
        "        run: /usr/local/bin/corelink-gc-sweep-production",
        "        env:",
        '          GC_OBSERVATION_ONLY: "true"',
        '          GC_LIVE_DELETE: "false"',
    )
    if step is None or any(step.count(line) != 1 for line in required_step_lines):
        raise Blocked("production observation step is missing its read-only runtime fence")
    if sum(line.strip().startswith("run:") for line in step) != 1:
        raise Blocked("production observation step must have exactly one command")
    if any(line.strip().startswith(("if:", "continue-on-error:")) for line in step):
        raise Blocked("production observation step cannot be conditional or suppress errors")


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

    # A binary in the image-build workflow does not establish an observation
    # path.  Require the dedicated lane by name so an unrelated reference or a
    # comment cannot suppress this blocker.
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
