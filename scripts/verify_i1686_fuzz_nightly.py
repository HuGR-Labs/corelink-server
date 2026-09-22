#!/usr/bin/env python3
"""Contract-only verifier for the restored #1686 fuzz nightly lane.

This check parses workflow structure and never starts cargo-fuzz.  Runtime
proof still requires a scheduled or manual Actions run with its artifacts.
"""
from __future__ import annotations

from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/fuzz-nightly.yml"
WORKFLOW_PATH = ".github/workflows/fuzz-nightly.yml"
EXPECTED_TARGETS = {
    ("corelink-byok", "wrapped_dek_parse"),
    ("corelink-byok", "envelope_roundtrip"),
    ("tenant-path", "derive_prefix_extended"),
    ("corelink-audit-chain", "merkle_append"),
    ("corelink-audit-chain", "jcs_canonicalize"),
    ("corelink-ac", "hkdf_expand"),
}
EXPECTED_ACTIONS = {
    "actions/checkout": "9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
    "dtolnay/rust-toolchain": "29eef336d9b2848a0b548edc03f92a220660cdb8",
    "actions/cache": "55cc8345863c7cc4c66a329aec7e433d2d1c52a9",
    "actions/upload-artifact": "043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
}


class ContractError(RuntimeError):
    """The workflow no longer satisfies an i1686 invariant."""


def _events(document: dict) -> dict:
    # PyYAML 1.1 parses the YAML 1.2 ``on`` key as boolean True.
    events = document.get("on", document.get(True, {}))
    if not isinstance(events, dict):
        raise ContractError("workflow trigger block must be a mapping")
    return events


def load(path: Path = WORKFLOW) -> tuple[dict, str]:
    source = path.read_text(encoding="utf-8")
    try:
        document = yaml.safe_load(source)
    except yaml.YAMLError as error:
        raise ContractError(f"invalid workflow YAML: {error}") from error
    if not isinstance(document, dict):
        raise ContractError("workflow root must be a mapping")
    return document, source


def _step(job: dict, name: str) -> dict:
    for candidate in job.get("steps", []):
        if isinstance(candidate, dict) and candidate.get("name") == name:
            return candidate
    raise ContractError(f"missing step: {name}")


def verify(document: dict, source: str = "") -> None:
    if "campaign-ci.yml" in source:
        raise ContractError("i1686 contract must not depend on campaign-ci")
    events = _events(document)
    schedule = events.get("schedule")
    if schedule != [{"cron": "17 3 * * *"}]:
        raise ContractError("schedule must contain the 03:17 UTC nightly trigger")
    if "workflow_dispatch" not in events:
        raise ContractError("workflow_dispatch trigger is missing")

    concurrency = document.get("concurrency")
    if not isinstance(concurrency, dict):
        raise ContractError("top-level concurrency contract is missing")
    group = concurrency.get("group", "")
    if not isinstance(group, str) or "${{ github.event_name }}" not in group:
        raise ContractError("concurrency group must separate event types")
    if "${{ github.ref }}" not in group:
        raise ContractError("concurrency group must remain ref-scoped")
    if concurrency.get("cancel-in-progress") is not False:
        raise ContractError("cancel-in-progress must preserve evidence")

    jobs = document.get("jobs")
    job = jobs.get("fuzz-matrix-expansion") if isinstance(jobs, dict) else None
    if not isinstance(job, dict):
        raise ContractError("fuzz-matrix-expansion job is missing")
    if job.get("runs-on") != "ubuntu-latest":
        raise ContractError("fuzz matrix must run on hosted Linux")
    if job.get("timeout-minutes") != 40:
        raise ContractError("job timeout must be 40 minutes")

    strategy = job.get("strategy")
    if not isinstance(strategy, dict) or strategy.get("fail-fast") is not False:
        raise ContractError("fuzz shards must not fail-fast")
    if strategy.get("max-parallel") != 6:
        raise ContractError("all six fuzz shards must be admitted concurrently")
    matrix = strategy.get("matrix", {}).get("include", [])
    targets = {
        (entry.get("crate"), entry.get("target"))
        for entry in matrix
        if isinstance(entry, dict)
    }
    if targets != EXPECTED_TARGETS or len(matrix) != len(EXPECTED_TARGETS):
        raise ContractError("fuzz shard matrix drifted")

    global_env = document.get("env")
    if not isinstance(global_env, dict) or global_env.get("FUZZ_DURATION") != "1800":
        raise ContractError("nightly fuzz duration must remain 1800 seconds")
    env = job.get("env")
    if not isinstance(env, dict):
        raise ContractError("fuzz matrix environment is missing")
    if env.get("RUSTUP_TOOLCHAIN") != "nightly":
        raise ContractError("hosted fuzz job must select nightly")
    if env.get("FUZZ_CRATE") != "${{ matrix.crate }}" or env.get("FUZZ_TARGET") != "${{ matrix.target }}":
        raise ContractError("matrix identity must bind to the isolated execution state")

    steps = job.get("steps")
    if not isinstance(steps, list):
        raise ContractError("fuzz job steps are missing")
    by_name = {
        step.get("name"): step
        for step in steps
        if isinstance(step, dict) and isinstance(step.get("name"), str)
    }
    checkout = next((step for step in steps if step.get("uses", "").startswith("actions/checkout@")), None)
    toolchain = next((step for step in steps if step.get("uses", "").startswith("dtolnay/rust-toolchain@")), None)
    if checkout is None or checkout["uses"] != f"actions/checkout@{EXPECTED_ACTIONS['actions/checkout']}":
        raise ContractError("checkout action pin drifted")
    if toolchain is None or toolchain["uses"] != f"dtolnay/rust-toolchain@{EXPECTED_ACTIONS['dtolnay/rust-toolchain']}":
        raise ContractError("toolchain action pin drifted")
    if toolchain.get("with", {}).get("toolchain") != "nightly":
        raise ContractError("nightly toolchain setup is missing")

    install = _step(job, "Install cargo-fuzz (isolated CARGO_HOME — never the shared one)")
    if install.get("timeout-minutes") != 20 or "cargo-fuzz --locked --version =0.13.1" not in install.get("run", ""):
        raise ContractError("cargo-fuzz pin or installer timeout drifted")
    if 'CARGO_HOME="$TOOLS"' not in install.get("run", ""):
        raise ContractError("cargo-fuzz installer must keep its own Cargo home")

    restore = _step(job, "Restore fuzz corpus (warm-start across nightly runs)")
    if restore.get("uses") != f"actions/cache@{EXPECTED_ACTIONS['actions/cache']}":
        raise ContractError("corpus cache action pin drifted")
    cache = restore.get("with", {})
    if cache.get("path") != "crates/${{ matrix.crate }}/fuzz/corpus/${{ matrix.target }}":
        raise ContractError("corpus cache path drifted")
    if cache.get("key") != "fuzz-corpus-${{ matrix.crate }}-${{ matrix.target }}-${{ github.run_id }}":
        raise ContractError("corpus cache key must be run-specific")
    if str(cache.get("restore-keys", "")).strip() != "fuzz-corpus-${{ matrix.crate }}-${{ matrix.target }}-":
        raise ContractError("corpus restore key drifted")

    run = _step(job, "Run fuzz target (${{ env.FUZZ_DURATION }}s)")
    if run.get("timeout-minutes") != 35:
        raise ContractError("fuzz process timeout must be 35 minutes")
    command = run.get("run", "")
    if "set -euo pipefail" not in command or "cargo fuzz run ${{ matrix.target }}" not in command:
        raise ContractError("fuzz crash must fail the job")
    if run.get("continue-on-error") is True:
        raise ContractError("fuzz crash cannot be ignored")

    upload = _step(job, "Upload fuzz artifacts on failure")
    if upload.get("if") != "failure()" or upload.get("uses") != f"actions/upload-artifact@{EXPECTED_ACTIONS['actions/upload-artifact']}":
        raise ContractError("crash artifact upload contract drifted")
    artifact = upload.get("with", {})
    if artifact.get("path") != "crates/${{ matrix.crate }}/fuzz/artifacts/":
        raise ContractError("crash artifact path drifted")
    if artifact.get("retention-days") != 14 or artifact.get("if-no-files-found") != "ignore":
        raise ContractError("crash artifact retention contract drifted")

def mutation_checks() -> None:
    document, source = load()
    verify(document, source)
    mutations = (
        ("cancel-in-progress", lambda d: d["concurrency"].update({"cancel-in-progress": True})),
        ("event-scoping", lambda d: d["concurrency"].update({"group": "ci-fuzz-${{ github.ref }}"})),
        ("schedule", lambda d: _events(d).update({"schedule": []})),
        ("runner", lambda d: d["jobs"]["fuzz-matrix-expansion"].update({"runs-on": "corelink"})),
        ("shard", lambda d: d["jobs"]["fuzz-matrix-expansion"]["strategy"].update({"max-parallel": 1})),
        ("timeout", lambda d: d["jobs"]["fuzz-matrix-expansion"].update({"timeout-minutes": 0})),
    )
    for name, mutate in mutations:
        import copy

        mutated = copy.deepcopy(document)
        mutate(mutated)
        try:
            verify(mutated, source)
        except ContractError:
            continue
        raise ContractError(f"mutation was accepted: {name}")


if __name__ == "__main__":
    import sys

    try:
        if "--self-test" in sys.argv:
            mutation_checks()
        else:
            document, source = load()
            verify(document, source)
    except (ContractError, OSError) as error:
        print(f"i1686 fuzz nightly contract: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
    print(f"i1686 fuzz nightly contract: PASS ({WORKFLOW_PATH})")
