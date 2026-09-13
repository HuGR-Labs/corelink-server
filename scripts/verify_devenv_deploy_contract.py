#!/usr/bin/env python3
"""Validate the inactive DevEnv cross-worker deployment contract."""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURE = ROOT / "tests/fixtures/deployment/devenv-cross-worker-binding.toml"
WRANGLER = ROOT / "wrangler.toml"
ENV_TYPE = ROOT / "worker/src/index_env.ts"
RUNBOOK = ROOT / "docs/operator/devenv-production-readiness.md"
MATRIX = ROOT / "docs/internal/secrets-checklist.md"
ENVIRONMENTS = ("root", "prod", "prod-sam", "prod-lhr", "prod-nrt", "prod-syd")
EXPECTED = {"name": "RUNNER_DEVENV_DO", "class_name": "RunnerDevEnvDO", "script_name": "corelink-spawn-worker"}


def bindings(config: dict, environment: str) -> list[dict]:
    if environment == "root":
        return config.get("durable_objects", {}).get("bindings", [])
    return config.get("env", {}).get(environment, {}).get("durable_objects", {}).get("bindings", [])


def main() -> int:
    fixture = tomllib.loads(FIXTURE.read_text(encoding="utf-8"))
    current = tomllib.loads(WRANGLER.read_text(encoding="utf-8"))
    for environment in ENVIRONMENTS:
        candidate = [binding for binding in bindings(fixture, environment) if binding.get("name") == EXPECTED["name"]]
        if candidate != [EXPECTED]:
            raise AssertionError(f"fixture {environment} binding is not the exact cross-worker contract")
        live = [binding for binding in bindings(current, environment) if binding.get("name") == EXPECTED["name"]]
        if live:
            raise AssertionError(f"live {environment} unexpectedly enables DevEnv before target preflight")

    if "RUNNER_DEVENV_DO?: DurableObjectNamespace<RunnerDevEnvRpc>;" not in ENV_TYPE.read_text(encoding="utf-8"):
        raise AssertionError("Server Env must retain the optional typed DevEnv RPC namespace")
    runbook = RUNBOOK.read_text(encoding="utf-8")
    for required in ("COMPUTE_GRANT_SIGNING_KEY", "COMPUTE_GRANT_KEY_ID", "FABRIC_COMPUTE_GRANT_PUBLIC_KEYS", "b281859", "b26d785", "0145e461fc1d35e2672f4a2190008fef4214c691bc8485ec68902cb8852fd441", "0118", "0130", "not deployable"):
        if required not in runbook:
            raise AssertionError(f"runbook omits required readiness contract: {required}")
    matrix = MATRIX.read_text(encoding="utf-8")
    for required in ("COMPUTE_GRANT_SIGNING_KEY", "COMPUTE_GRANT_KEY_ID"):
        if required not in matrix:
            raise AssertionError(f"secrets matrix omits Server issuer input: {required}")
    print("devenv deployment contract: OK (inactive until Runners target preflight passes)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, tomllib.TOMLDecodeError) as error:
        print(f"devenv deployment contract: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
