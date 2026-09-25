#!/usr/bin/env python3
"""Freeze and verify the complete, bounded unmutated workspace baseline for #2478."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any, Mapping


SHARD_COUNT = 120
SCHEMA = "corelink.hosted-mutants-baseline-test-plan.v1"
SHARD_SCHEMA = "corelink.hosted-mutants-baseline-test-shard.v1"
RECEIPT_SCHEMA = "corelink.hosted-mutants-baseline-receipt.v6"


class VerificationError(ValueError):
    pass


def canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def digest(value: Any) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def read(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def write(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise VerificationError(message)


def build_plan(args: argparse.Namespace) -> None:
    metadata = read(args.metadata)
    packages = {item["id"]: item for item in metadata["packages"] if item["id"] in metadata["workspace_members"]}
    require(packages, "workspace metadata has no members")
    targets = {
        (package_id, target["name"], tuple(sorted(target["kind"]))): target
        for package_id, package in packages.items()
        for target in package["targets"]
    }
    entries: dict[str, dict[str, Any]] = {}
    for line in args.artifacts.read_text(encoding="utf-8").splitlines():
        event = json.loads(line)
        if event.get("reason") != "compiler-artifact" or not event.get("profile", {}).get("test"):
            continue
        package = packages.get(event.get("package_id"))
        executable = event.get("executable")
        target = event.get("target", {})
        kinds = target.get("kind")
        if package is None or not executable or not isinstance(kinds, list):
            continue
        kind = ",".join(sorted(kinds))
        if not set(kinds) & {"lib", "bin", "test", "example", "bench"}:
            continue
        metadata_target = targets.get((event["package_id"], target.get("name"), tuple(sorted(kinds))))
        require(metadata_target is not None, f"cargo test artifact has no metadata target: {package['name']}:{target.get('name')}")
        # Cargo compiles ordinary examples in a test profile, but it does not
        # turn them into libtest harnesses or run them.  The metadata `test`
        # flag is Cargo's target-level execution contract: include every true
        # runnable target and never try `--list` on a normal example binary.
        if metadata_target.get("test") is not True:
            continue
        executable_path = Path(executable).resolve()
        try:
            relative = executable_path.relative_to(args.target_dir.resolve()).as_posix()
        except ValueError as error:
            raise VerificationError(f"test executable escapes target directory: {executable_path}") from error
        entry_id = f"binary:{package['name']}:{kind}:{target.get('name')}"
        require(entry_id not in entries, f"cargo test produced duplicate executable mapping: {entry_id}")
        listed = subprocess.run(
            [str(executable_path), "--list", "--format", "terse"],
            cwd=Path(package["manifest_path"]).parent,
            check=False,
            capture_output=True,
            text=True,
        )
        require(listed.returncode == 0, f"cannot enumerate tests for {entry_id}")
        test_names = sorted(
            line.rsplit(": ", 1)[0]
            for line in listed.stdout.splitlines()
            if line.endswith(": test")
        )
        require(len(test_names) == len(set(test_names)), f"test executable has duplicate test names: {entry_id}")
        entries[entry_id] = {
            "id": entry_id,
            "kind": "binary",
            "package": package["name"],
            "working_directory": Path(package["manifest_path"]).parent.resolve().relative_to(args.workspace.resolve()).as_posix(),
            "executable": relative,
            "test_names": test_names,
        }
    for package in packages.values():
        if any("lib" in target["kind"] for target in package["targets"]):
            entry_id = f"doctest:{package['name']}"
            entries[entry_id] = {"id": entry_id, "kind": "doctest", "package": package["name"]}
    ordered = [entries[key] for key in sorted(entries)]
    require(ordered, "cargo test --no-run produced no executable or doctest coverage")
    plan = {
        "schema": SCHEMA,
        "run_id": args.run_id,
        "run_attempt": args.run_attempt,
        "sha": args.sha,
        "command": "cargo test --workspace --locked",
        "shard_count": SHARD_COUNT,
        "entries": ordered,
    }
    plan["plan_digest"] = digest({"entries": ordered, "shard_count": SHARD_COUNT, "command": plan["command"]})
    write(args.out, plan)


def validate_plan(plan: Mapping[str, Any]) -> list[dict[str, Any]]:
    require(plan.get("schema") == SCHEMA, "unsupported baseline test plan")
    require(isinstance(plan.get("run_id"), str) and plan["run_id"], "baseline plan run identity is missing")
    require(isinstance(plan.get("run_attempt"), int) and not isinstance(plan["run_attempt"], bool) and plan["run_attempt"] > 0, "baseline plan attempt is invalid")
    require(isinstance(plan.get("sha"), str) and len(plan["sha"]) == 40, "baseline plan SHA is invalid")
    require(plan.get("command") == "cargo test --workspace --locked", "baseline command drifted")
    require(plan.get("shard_count") == SHARD_COUNT, "baseline shard count drifted")
    entries = plan.get("entries")
    require(isinstance(entries, list) and entries, "baseline plan has no entries")
    seen: set[str] = set()
    for item in entries:
        require(isinstance(item, dict) and isinstance(item.get("id"), str), "baseline entry is malformed")
        require(item["id"] not in seen, "baseline entry is duplicated")
        seen.add(item["id"])
        require(item.get("kind") in {"binary", "doctest"} and isinstance(item.get("package"), str), "baseline entry kind drifted")
        if item["kind"] == "binary":
            executable = item.get("executable")
            require(isinstance(executable, str) and executable and not Path(executable).is_absolute() and ".." not in Path(executable).parts, "baseline executable is unsafe")
            working_directory = item.get("working_directory")
            require(isinstance(working_directory, str) and not Path(working_directory).is_absolute() and ".." not in Path(working_directory).parts, "baseline binary working directory is unsafe")
            test_names = item.get("test_names")
            require(isinstance(test_names, list) and all(isinstance(name, str) and name for name in test_names) and test_names == sorted(set(test_names)), "baseline binary test mapping is malformed")
    expected = digest({"entries": entries, "shard_count": SHARD_COUNT, "command": plan["command"]})
    require(plan.get("plan_digest") == expected, "baseline plan digest does not bind entries")
    return entries


def membership(entries: list[dict[str, Any]], shard: int) -> list[dict[str, Any]]:
    require(0 <= shard < SHARD_COUNT, "baseline shard index is invalid")
    return entries[shard::SHARD_COUNT]


def run_shard(args: argparse.Namespace) -> None:
    plan = read(args.plan)
    entries = validate_plan(plan)
    require(plan.get("sha") == args.sha and plan.get("run_id") == args.run_id and plan.get("run_attempt") <= args.run_attempt, "baseline plan is not from this run lineage")
    target_dir = args.target_dir.resolve()
    selected = membership(entries, args.shard)
    completed: list[str] = []
    failed: list[str] = []
    for entry in selected:
        if entry["kind"] == "binary":
            executable = (target_dir / entry["executable"]).resolve()
            require(executable.is_relative_to(target_dir), "baseline executable escapes target directory")
            command = [str(executable)]
            cwd = args.workspace / entry["working_directory"]
            if not executable.is_file():
                failed.append(entry["id"])
                continue
        else:
            command = ["cargo", "test", "--locked", "--package", entry["package"], "--doc"]
            cwd = args.workspace
        try:
            result = subprocess.run(command, cwd=cwd, check=False)
        except OSError:
            # Keep a failure receipt even if a binary disappears or cannot be
            # launched after the presence check. The workflow still fails closed.
            failed.append(entry["id"])
            continue
        (completed if result.returncode == 0 else failed).append(entry["id"])
    receipt = {
        "schema": SHARD_SCHEMA,
        "run_id": args.run_id,
        "run_attempt": args.run_attempt,
        "sha": args.sha,
        "plan_digest": plan["plan_digest"],
        "shard": args.shard,
        "entry_ids": [entry["id"] for entry in selected],
        "completed_entry_ids": completed,
        "failed_entry_ids": failed,
        "status": "success" if not failed else "failure",
    }
    write(args.out, receipt)
    if failed:
        raise SystemExit(f"baseline shard {args.shard} failed entries: {', '.join(failed)}")


def aggregate(args: argparse.Namespace) -> None:
    plans = [read(path) for path in args.artifacts.rglob("baseline-test-plan.json")]
    require(plans, "baseline test plan artifact is missing")
    candidates = [item for item in plans if item.get("run_id") == args.run_id and item.get("sha") == args.sha and isinstance(item.get("run_attempt"), int) and item["run_attempt"] <= args.run_attempt]
    require(candidates, "baseline plan is not from this exact run")
    attempts = [item["run_attempt"] for item in candidates]
    require(len(attempts) == len(set(attempts)), "baseline plan has duplicate attempts")
    plan = max(candidates, key=lambda item: item["run_attempt"])
    entries = validate_plan(plan)
    receipts = [read(path) for path in args.artifacts.rglob("baseline-shard-receipt.json")]
    by_shard: dict[int, list[Mapping[str, Any]]] = defaultdict(list)
    for receipt in receipts:
        if receipt.get("run_id") == args.run_id and receipt.get("sha") == args.sha and receipt.get("plan_digest") == plan["plan_digest"]:
            shard = receipt.get("shard")
            if isinstance(shard, int):
                by_shard[shard].append(receipt)
    require(set(by_shard) == set(range(SHARD_COUNT)), "baseline shards are incomplete or unexpected")
    selected: list[Mapping[str, Any]] = []
    for shard in range(SHARD_COUNT):
        attempts = [item.get("run_attempt") for item in by_shard[shard]]
        require(len(attempts) == len(set(attempts)), f"baseline shard {shard} has duplicate attempts")
        receipt = max(by_shard[shard], key=lambda item: item.get("run_attempt", 0))
        expected = [entry["id"] for entry in membership(entries, shard)]
        require(receipt.get("run_attempt", 0) <= args.run_attempt, "baseline shard is from a future attempt")
        require(receipt.get("entry_ids") == expected, f"baseline shard {shard} mapping drifted")
        require(receipt.get("completed_entry_ids") == expected and receipt.get("failed_entry_ids") == [] and receipt.get("status") == "success", f"baseline shard {shard} did not complete")
        selected.append(receipt)
    receipt = {
        "schema": RECEIPT_SCHEMA,
        "run_id": args.run_id,
        "run_attempt": args.run_attempt,
        "sha": args.sha,
        "tool_version": "27.0.0",
        "config_digest": args.config_digest,
        "inventory_digest": args.inventory_digest,
        "baseline_digest": digest({"sha": args.sha, "run_id": args.run_id, "run_attempt": args.run_attempt, "command": "cargo test --workspace --locked", "plan_digest": plan["plan_digest"]}),
        "plan_digest": plan["plan_digest"],
        "shard_count": SHARD_COUNT,
        "covered_entries": len(entries),
        "status": "success",
        "exit_code": 0,
    }
    write(args.out, receipt)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build = commands.add_parser("build-plan")
    build.add_argument("--metadata", type=Path, required=True); build.add_argument("--artifacts", type=Path, required=True); build.add_argument("--workspace", type=Path, required=True); build.add_argument("--target-dir", type=Path, required=True)
    build.add_argument("--sha", required=True); build.add_argument("--run-id", required=True); build.add_argument("--run-attempt", type=int, required=True); build.add_argument("--out", type=Path, required=True); build.set_defaults(func=build_plan)
    run = commands.add_parser("run-shard")
    run.add_argument("--plan", type=Path, required=True); run.add_argument("--workspace", type=Path, required=True); run.add_argument("--target-dir", type=Path, required=True); run.add_argument("--sha", required=True); run.add_argument("--run-id", required=True); run.add_argument("--run-attempt", type=int, required=True); run.add_argument("--shard", type=int, required=True); run.add_argument("--out", type=Path, required=True); run.set_defaults(func=run_shard)
    aggregate_parser = commands.add_parser("aggregate")
    aggregate_parser.add_argument("--artifacts", type=Path, required=True); aggregate_parser.add_argument("--sha", required=True); aggregate_parser.add_argument("--run-id", required=True); aggregate_parser.add_argument("--run-attempt", type=int, required=True); aggregate_parser.add_argument("--config-digest", required=True); aggregate_parser.add_argument("--inventory-digest", required=True); aggregate_parser.add_argument("--out", type=Path, required=True); aggregate_parser.set_defaults(func=aggregate)
    args = parser.parse_args()
    try:
        args.func(args)
    except (OSError, json.JSONDecodeError, VerificationError) as error:
        print(f"#2478 baseline rejected: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
