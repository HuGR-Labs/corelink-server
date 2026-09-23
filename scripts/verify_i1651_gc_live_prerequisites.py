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
import textwrap
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOPOLOGY = ROOT / "infra/staging/topology.json"
READINESS = ROOT / "evidence/staging/readiness.json"
DRY_RUN_WORKFLOW = ROOT / ".github/workflows/gc-sweep-dry-run.yml"
CONTROL_WORKFLOW = ROOT / ".github/workflows/issue-1651-gc-control.yml"
OBSERVATION_WORKFLOW = ROOT / ".github/workflows/gc-production-observation.yml"

REQUIRED_STAGING_SECRETS = (
    "K6_STAGING_BYOK_CMK_ID",
    "K6_STAGING_MFA_STUB",
    "K6_STAGING_PAT",
    "K6_STAGING_STRIPE_WHSEC",
    "K6_TARGET_HOST",
)

class Blocked(RuntimeError):
    """The live observation boundary is not proven by repository evidence."""


def _active_yaml_lines(workflow: str) -> list[tuple[int, str]]:
    """Return indentation and content for nonblank, noncomment YAML lines."""
    lines = []
    for raw_line in textwrap.dedent(workflow).splitlines():
        content = raw_line.lstrip()
        if not content or content.startswith("#"):
            continue
        indent = len(raw_line) - len(content)
        lines.append((indent, content.rstrip()))
    return lines


def observation_workflow_satisfies_contract(workflow: str) -> bool:
    """Require a manual, read-only workflow job that runs the scoped collector."""
    lines = _active_yaml_lines(workflow)

    on_index = next(
        (index for index, (indent, text) in enumerate(lines) if indent == 0 and text in ("on:", "'on':", '"on":')),
        None,
    )
    if on_index is None:
        return False
    events = []
    for indent, text in lines[on_index + 1 :]:
        if indent == 0:
            break
        if indent == 2 and text.endswith(":"):
            events.append(text[:-1])
    if events != ["workflow_dispatch"]:
        return False

    permissions_index = next(
        (index for index, (indent, text) in enumerate(lines) if indent == 0 and text == "permissions:"),
        None,
    )
    if permissions_index is None:
        return False
    permissions = []
    for indent, text in lines[permissions_index + 1 :]:
        if indent == 0:
            break
        if indent == 2:
            permissions.append(text)
    if permissions != ["contents: read"]:
        return False

    jobs_index = next(
        (index for index, (indent, text) in enumerate(lines) if indent == 0 and text == "jobs:"),
        None,
    )
    if jobs_index is None:
        return False
    job_blocks: list[list[tuple[int, str]]] = []
    current_job: list[tuple[int, str]] = []
    for indent, text in lines[jobs_index + 1 :]:
        if indent == 0:
            break
        if indent == 2 and text.endswith(":"):
            if current_job:
                job_blocks.append(current_job)
            current_job = [(indent, text)]
        elif current_job:
            current_job.append((indent, text))
    if current_job:
        job_blocks.append(current_job)

    for job in job_blocks:
        if (4, "environment: staging") not in job:
            continue
        if not any(indent == 4 and text.startswith("runs-on: ubuntu-") for indent, text in job):
            continue

        for index, (indent, text) in enumerate(job):
            if indent not in (6, 8) or not text.startswith("run:"):
                continue
            command_lines = [text.partition(":")[2].strip()]
            for body_indent, body_text in job[index + 1 :]:
                if body_indent <= indent:
                    break
                command_lines.append(body_text.strip())
            command = "\n".join(command_lines)
            if not any(
                line.rstrip("\\").strip() == "python3 scripts/collect_b071_gc_observation.py collect"
                for line in command.splitlines()
            ):
                continue
            if all(flag in command for flag in ("--scopes", "--image-digest", "--operator", "--output")):
                return True
    return False


def contract_self_test() -> None:
    """Adversarial examples keep generic binary mentions from passing readiness."""
    generic_binary_smoke = """
    name: container-build
    on:
      workflow_dispatch:
    permissions:
      contents: read
    jobs:
      smoke:
        runs-on: ubuntu-24.04
        environment: staging
        steps:
          - run: echo "python3 scripts/collect_b071_gc_observation.py collect --scopes X --image-digest X --operator X --output X"
    # Deliberately starts without scope and expects fail-closed.
    """
    if observation_workflow_satisfies_contract(generic_binary_smoke):
        raise AssertionError("generic binary mention incorrectly passed as observation workflow")

    missing_protection = """
    on:
      workflow_dispatch:
    permissions:
      contents: read
    jobs:
      observe:
        runs-on: ubuntu-24.04
        steps:
          - run: python3 scripts/collect_b071_gc_observation.py collect --scopes X --image-digest X --operator X --output X
    """
    if observation_workflow_satisfies_contract(missing_protection):
        raise AssertionError("collector command without staging protection incorrectly passed")

    dedicated_staging_observation = """
    name: gc-production-observation
    on:
      workflow_dispatch:
        inputs:
          scopes:
            required: true
    permissions:
      contents: read
    jobs:
      observe:
        runs-on: ubuntu-24.04
        environment: staging
        steps:
          - name: collect bounded observation
            run: |
              python3 scripts/collect_b071_gc_observation.py collect \\
                --scopes "${{ inputs.scopes }}" \\
                --image-digest "${{ inputs.image_digest }}" \\
                --operator "${{ github.actor }}" \\
                --output evidence/owner-actions/B-071/gc-production-dry-run.json
    """
    if not observation_workflow_satisfies_contract(dedicated_staging_observation):
        raise AssertionError("dedicated protected staging observation did not pass")


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

    # A live observation must be an explicitly reviewed staging workflow. Do
    # not treat the production-image smoke check as an observation: it invokes
    # the binary without scope only to prove that it fails closed.
    try:
        observation_workflow = _text(OBSERVATION_WORKFLOW)
    except Blocked as exc:
        blockers.append(str(exc))
    else:
        if not observation_workflow_satisfies_contract(observation_workflow):
            blockers.append(
                "dedicated GC observation workflow must use workflow_dispatch, grant only "
                "contents: read, and run the scoped collector in a hosted staging environment"
            )

    if blockers:
        raise Blocked("\n".join(f"- {item}" for item in blockers))


if __name__ == "__main__":
    if sys.argv[1:] == ["--contract-self-test"]:
        contract_self_test()
        print("I1651 observation workflow contract: PASS")
        raise SystemExit(0)
    if sys.argv[1:]:
        print("usage: verify_i1651_gc_live_prerequisites.py [--contract-self-test]", file=sys.stderr)
        raise SystemExit(2)
    try:
        verify()
    except (Blocked, OSError) as exc:
        print("I1651 GC live prerequisites: BLOCKED", file=sys.stderr)
        print(exc, file=sys.stderr)
        raise SystemExit(1)
    print("I1651 GC live prerequisites: PASS (evidence only; deletion remains disabled)")
