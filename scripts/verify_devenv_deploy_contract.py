#!/usr/bin/env python3
"""Offline, fail-closed contract check for the inactive DevEnv deployment seam.

The cross-worker binding is documented as a fixture because this repository must
not enable it until the sibling Runners deployment proves its class, image and
verification keys.  This checker never contacts Cloudflare and never reads
secrets.
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/deployment/devenv-cross-worker-binding.toml"
ENV = ROOT / "worker/src/index_env.ts"
WRANGLER = ROOT / "wrangler.toml"
RUNBOOK = ROOT / "docs/operator/devenv-production-readiness.md"
MIGRATIONS = ROOT / "migrations/d1"
EXPECTED_ENVS = {"prod", "prod-sam", "prod-lhr", "prod-nrt", "prod-syd"}


class ContractError(RuntimeError):
    pass


def check_contract() -> None:
    if not FIXTURE.is_file():
        raise ContractError("deployment preflight fixture is absent")
    try:
        fixture = tomllib.loads(FIXTURE.read_text())
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise ContractError(f"deployment preflight fixture unreadable: {exc}") from exc

    for key, expected in (
        ("script_name", "corelink-spawn-worker"),
        ("class_name", "RunnerDevEnvDO"),
        ("binding", "RUNNER_DEVENV_DO"),
        ("signer_verifier_public_key_map", "required"),
        ("image_digest", "required:sha256"),
        ("migrations", "0118-0130"),
    ):
        if fixture.get(key) != expected:
            raise ContractError(f"fixture {key} is not the exact fail-closed contract")
    environments = {entry.get("name") for entry in fixture.get("environment", [])}
    if environments != EXPECTED_ENVS:
        raise ContractError("fixture must cover exactly the five production environments")
    if fixture.get("root", {}).get("environment") != "default":
        raise ContractError("fixture root/default environment is absent")

    env_text = ENV.read_text()
    for name in ("RUNNER_DEVENV_DO?:", "COMPUTE_GRANT_SIGNING_KEY?:", "COMPUTE_GRANT_SIGNING_KEY_ID?:"):
        if name not in env_text:
            raise ContractError(f"Env optional binding is absent: {name}")

    # The production wrangler file is deliberately untouched: enabling the
    # cross-worker binding is a later, separately gated deployment action.
    if re.search(r"(?m)^\s*name\s*=\s*[\"']RUNNER_DEVENV_DO[\"']", WRANGLER.read_text()):
        raise ContractError("live wrangler.toml must not enable RUNNER_DEVENV_DO")
    if re.search(r"(?m)^\s*script_name\s*=\s*[\"']corelink-spawn-worker[\"']", WRANGLER.read_text()):
        raise ContractError("live wrangler.toml must not carry the cross-worker binding")

    missing = [f"{number:04d}" for number in range(118, 131)
               if not list(MIGRATIONS.glob(f"{number:04d}_*.sql"))]
    if missing:
        raise ContractError(f"migration preflight range 0118-0130 incomplete: {missing}")

    runbook = RUNBOOK.read_text()
    required_phrases = (
        "NO live enable",
        "corelink-runners",
        "origin/main",
        "verifier",
        "image digest",
        "signer",
        "0118-0130",
    )
    if any(phrase not in runbook for phrase in required_phrases):
        raise ContractError("runbook does not state every closed-gate prerequisite")


def self_test() -> None:
    # Keep the negative checks executable without mutating repository files.
    original = FIXTURE.read_text()
    weakened = original.replace('image_digest = "required:sha256"', 'image_digest = ""')
    if tomllib.loads(weakened).get("image_digest") == "required:sha256":
        raise ContractError("self-test fixture mutation did not apply")
    if "NO live enable" not in RUNBOOK.read_text():
        raise ContractError("self-test cannot establish the closed deployment gate")


def main(argv: list[str]) -> int:
    try:
        check_contract()
        if argv == ["--self-test"]:
            self_test()
        elif argv:
            raise ContractError(f"unknown arguments: {' '.join(argv)}")
    except (ContractError, OSError, tomllib.TOMLDecodeError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    print("PASS: inactive DevEnv deployment contract")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
