#!/usr/bin/env python3
"""Contract and mutation gate for CI migration bundle 2 (#2194)."""

from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import re
import sys

import yaml


WORKFLOWS = {
    ".github/workflows/alerts-validate.yml": {
        "events": {"pull_request", "push", "workflow_dispatch"},
        "permissions": {"contents": "read"},
        "jobs": {"alerts-validate": 10},
    },
    ".github/workflows/api-deprecation-check.yml": {
        "events": {"pull_request"},
        "permissions": {"contents": "read", "pull-requests": "read"},
        "jobs": {"validate-deprecations": 5},
    },
    ".github/workflows/audit-finding-coverage.yml": {
        "events": {"pull_request", "push", "workflow_dispatch"},
        "permissions": {"contents": "read"},
        "jobs": {"b373-admission": 10, "verify": 10},
    },
    ".github/workflows/b156-published-claims.yml": {
        "events": {"pull_request", "push", "workflow_dispatch"},
        "permissions": {"contents": "read"},
        "jobs": {"verify": 10},
    },
    ".github/workflows/b249-dt-webhook-sunset.yml": {
        "events": {"pull_request", "push"},
        "permissions": {"contents": "read"},
        "jobs": {"b249-sunset": 5},
    },
    ".github/workflows/canonical-consistency.yml": {
        "events": {"pull_request", "push"},
        "permissions": {"contents": "read", "pull-requests": "read"},
        "jobs": {"canonical-consistency": 5},
    },
}
SHA_PIN = re.compile(r"^[0-9a-f]{40}$")


class ContractError(AssertionError):
    pass


def _on(workflow: dict) -> dict:
    # PyYAML YAML 1.1 maps the unquoted key on to True.
    value = workflow.get("on", workflow.get(True))
    if not isinstance(value, dict):
        raise ContractError("workflow has no event map")
    return value


def _load(texts: dict[str, str]) -> dict[str, dict]:
    parsed = {}
    for path in WORKFLOWS:
        text = texts.get(path)
        if text is None:
            raise ContractError(f"missing workflow: {path}")
        try:
            value = yaml.safe_load(text)
        except yaml.YAMLError as error:
            raise ContractError(f"invalid YAML in {path}: {error}") from error
        if not isinstance(value, dict):
            raise ContractError(f"workflow root is not a mapping: {path}")
        parsed[path] = value
    return parsed


def _verify_parsed(parsed: dict[str, dict]) -> None:
    for path, spec in WORKFLOWS.items():
        workflow = parsed[path]
        events = set(_on(workflow))
        if events != spec["events"]:
            raise ContractError(f"event triggers changed in {path}: {sorted(events)}")
        if workflow.get("permissions") != spec["permissions"]:
            raise ContractError(f"least-privilege permissions changed in {path}")
        jobs = workflow.get("jobs", {})
        if set(jobs) != set(spec["jobs"]):
            raise ContractError(f"job population changed in {path}: {sorted(jobs)}")
        for name, timeout in spec["jobs"].items():
            job = jobs.get(name)
            if not isinstance(job, dict):
                raise ContractError(f"missing job {name} in {path}")
            if job.get("runs-on") != "ubuntu-24.04":
                raise ContractError(f"{path}:{name} is not on ubuntu-24.04")
            if job.get("timeout-minutes") != timeout:
                raise ContractError(f"{path}:{name} timeout changed")
            if job.get("permissions", spec["permissions"]) != spec["permissions"]:
                raise ContractError(f"{path}:{name} permissions changed")
            steps = job.get("steps", [])
            checkouts = [
                step for step in steps
                if isinstance(step, dict)
                and str(step.get("uses", "")).startswith("actions/checkout@")
            ]
            if len(checkouts) != 1:
                raise ContractError(f"{path}:{name} must have one pinned checkout")
            checkout = checkouts[0]
            uses = checkout["uses"].split("@", 1)[1]
            if not SHA_PIN.fullmatch(uses):
                raise ContractError(f"{path}:{name} checkout is not SHA-pinned")
            if checkout.get("with", {}).get("persist-credentials") is not False:
                raise ContractError(f"{path}:{name} persists checkout credentials")
            for step in steps:
                if not isinstance(step, dict):
                    continue
                action = step.get("uses")
                if not action or action.startswith("./"):
                    continue
                if "@" not in action or not SHA_PIN.fullmatch(action.split("@", 1)[1]):
                    raise ContractError(f"{path}:{name} has an unpinned action: {action}")


def verify(texts: dict[str, str]) -> None:
    _verify_parsed(_load(texts))


def verify_mutations(texts: dict[str, str]) -> None:
    mutations = (
        (".github/workflows/alerts-validate.yml", ("jobs", "alerts-validate", "runs-on"), "corelink"),
        (".github/workflows/api-deprecation-check.yml", ("permissions", "contents"), "write"),
        (".github/workflows/audit-finding-coverage.yml", ("jobs", "verify", "timeout-minutes"), 360),
        (".github/workflows/b156-published-claims.yml", ("jobs", "verify", "steps", 0, "with", "persist-credentials"), True),
        (".github/workflows/b249-dt-webhook-sunset.yml", ("on", "schedule"), []),
        (".github/workflows/canonical-consistency.yml", ("jobs", "canonical-consistency", "runs-on"), "self-hosted"),
    )
    parsed = _load(texts)
    for path, keys, replacement in mutations:
        mutant = deepcopy(parsed)
        target = mutant[path]
        if keys[0] == "on":
            _on(target)[keys[1]] = replacement
        else:
            for key in keys[:-1]:
                target = target[key]
            target[keys[-1]] = replacement
        try:
            _verify_parsed(mutant)
        except ContractError:
            continue
        raise ContractError(f"adversarial mutation survived: {path}:{keys}")


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    texts = {
        path: (root / path).read_text(encoding="utf-8")
        for path in WORKFLOWS
    }
    verify(texts)
    verify_mutations(texts)
    print("CI bundle 2 runner, trigger, permissions, timeout, pinning, and mutation contract: PASS")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ContractError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
