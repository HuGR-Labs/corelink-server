#!/usr/bin/env python3
"""Verify the credentialless B-105 cache cost contract.

This gate audits the checked-in cache path and replays only the arithmetic that
is possible from the retained evidence. It never invokes Cargo, sccache, a
network client, GitHub, or a provider API. A passing result is a repository
contract; B-105 remains open until the paired production receipt exists.
"""
from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "evidence/owner-actions/B-105/cache-cost-contract.json"
EXPECTED_SCHEMA = "corelink.b105.cache-cost-contract.v1"
REQUIRED_TOP_LEVEL = {
    "schema",
    "issue",
    "backlog_id",
    "status",
    "credentialless",
    "network_calls",
    "mutating_actions",
    "runner_contract",
    "sources",
    "current_path",
    "observed_cost_model",
    "acceptance",
    "invariants",
    "external_blocker",
    "prohibited",
}


class ContractError(RuntimeError):
    pass


def load_manifest(path: Path = MANIFEST) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ContractError(f"manifest is unreadable: {exc}") from exc
    if not isinstance(value, dict):
        raise ContractError("manifest must be a JSON object")
    if set(value) != REQUIRED_TOP_LEVEL:
        raise ContractError("manifest top-level keys drifted")
    if value["schema"] != EXPECTED_SCHEMA:
        raise ContractError("unsupported B-105 manifest schema")
    if value["issue"] != 1661 or value["backlog_id"] != "B-105":
        raise ContractError("B-105 issue identity drifted")
    if value["status"] != "open_external_measurement_required":
        raise ContractError("B-105 must remain open pending production measurement")
    if value["credentialless"] is not True or value["network_calls"] is not False or value["mutating_actions"] is not False:
        raise ContractError("contract is not credentialless, network-free, and read-only")
    if value["runner_contract"] != "github-hosted":
        raise ContractError("contract must run on a GitHub-hosted runner")
    return value


def text(path: str) -> str:
    try:
        return (ROOT / path).read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise ContractError(f"source {path!r} is unreadable: {exc}") from exc


def require(source: str, needle: str, label: str) -> None:
    if needle not in source:
        raise ContractError(f"{label}: missing {needle!r}")


def check_paths(manifest: dict[str, Any]) -> None:
    sources = manifest["sources"]
    if not isinstance(sources, dict) or set(sources) != {
        "pilot_workflow",
        "customer_recipe",
        "server_auth_path",
        "server_gate",
        "diagnosis",
        "hosted_lane_workflow",
        "backlog",
        "verifier",
    }:
        raise ContractError("source inventory drifted")
    for label, path in sources.items():
        if not isinstance(path, str) or not path or not (ROOT / path).is_file():
            raise ContractError(f"source inventory path is missing: {label}")
    if sources["verifier"] != "scripts/verify_b105_cache_contract.py":
        raise ContractError("manifest verifier path drifted")
    check_hosted_lane(text(sources["hosted_lane_workflow"]), text("scripts/collect_b105_same_lane.py"))


def check_hosted_lane(workflow: str, collector: str) -> None:
    """Freeze the live lane's host, auth boundary, namespace, and cleanup."""
    required_workflow = (
        "workflow_dispatch:",
        "runs-on: ubuntu-24.04",
        "timeout-minutes: 240",
        "environment: production",
        "github.repository_id == '1232040291'",
        "github.ref == 'refs/heads/main'",
        "github.ref_protected",
        "inputs.confirm == 'measure-b105-cache-cost'",
        "CORELINK_SCCACHE_TOKEN",
        "cancel-in-progress: false",
        "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e0",
        "tool: sccache@0.17.0",
        "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
        "retention-days: 7",
        "--cleanup-only",
    )
    for needle in required_workflow:
        require(workflow, needle, "hosted measurement workflow")
    if "runs-on: corelink" in workflow or "actions/cache@" in workflow or "Swatinem/rust-cache@" in workflow:
        raise ContractError("hosted measurement must not use the self-hosted runner or shared caches")
    required_collector = (
        '"schema": "corelink.b105.lane.v3"',
        '"b105-',
        '"SCCACHE_IGNORE_SERVER_IO_ERROR"',
        '"RUSTC_WRAPPER"',
        '"CARGO_INCREMENTAL"',
        '"--no-run"',
        '"retained_indexed_payload_bytes"',
        '"verified_absent"',
        '"currency_cost"',
        '"read_errors"',
        '"write_errors"',
        "send_collection_root(parsed.path)",
    )
    for needle in required_collector:
        require(collector, needle, "hosted measurement collector")
    if "SCCACHE_IGNORE_SERVER_IO_ERROR\"] = \"1\"" in collector:
        raise ContractError("cache IO failures must fail the measurement closed")
    if "upstream_key = \"\"" in collector:
        raise ContractError("collection-root calls must not reach the shared production tenant root")


def check_current_path(manifest: dict[str, Any]) -> None:
    path = manifest["current_path"]
    expected = {
        "workload": "corelink-reapi pr-gate",
        "runner": "corelink",
        "pilot_toggle": "vars.CORELINK_SCCACHE_PILOT == 'on'",
        "wrapper": "RUSTC_WRAPPER=sccache",
        "endpoint": "https://corelink-api.humangr.com/cargo/<tenant>",
        "workflow_cache_chain": "remote-only (SCCACHE_MULTILEVEL_CHAIN is not set)",
        "customer_cache_chain": "disk,webdav",
        "write_behavior": "cache misses write through the /cargo WebDAV surface; writes are excluded from the zero-miss observation",
    }
    if path != expected:
        raise ContractError("current cache path contract drifted")
    workflow = text(path=".github/workflows/corelink-reapi.yml")
    require(workflow, "runs-on: corelink", "pilot workflow")
    require(workflow, "vars.CORELINK_SCCACHE_PILOT == 'on'", "pilot workflow")
    require(workflow, "echo \"RUSTC_WRAPPER=sccache\"", "pilot workflow")
    require(workflow, "SCCACHE_WEBDAV_ENDPOINT=https://corelink-api.humangr.com/cargo/${SCCACHE_TENANT}", "pilot workflow")
    require(workflow, "SCCACHE_IGNORE_SERVER_IO_ERROR=1", "pilot workflow")
    if "SCCACHE_MULTILEVEL_CHAIN" in workflow:
        raise ContractError("pilot workflow unexpectedly gained a local sccache chain")
    recipe = text(path="apps/docs/docs/integrations/sccache-cargo.md")
    require(recipe, 'SCCACHE_MULTILEVEL_CHAIN="disk,webdav"', "customer recipe")
    require(recipe, "remote-only", "customer recipe remote-only warning")


def check_server_cause() -> None:
    verifier = text(path="crates/corelink-container/src/adapter_pat_verifier.rs")
    require(verifier, "secret_match_memo", "adapter verifier")
    require(verifier, "leaves the D1 row read per-request", "adapter verifier")
    require(verifier, "SECRET_MATCH_MEMO_TTL", "adapter verifier")
    gate = text(path="crates/corelink-container/src/adapter_pat_gate.rs")
    require(gate, "ARGON2_PER_TENANT_PERMITS", "adapter gate")
    diagnosis = text(path="docs/internal/sccache-pilot-diagnosis-2026-08-04.md")
    require(diagnosis, "Cache misses                           0", "retained diagnosis")
    require(diagnosis, "Argon2id", "retained diagnosis")


def cost_report(model: dict[str, Any]) -> dict[str, Any]:
    cold = model["cold_lane_seconds"]
    warm = model["warm_lane_seconds"]
    hits = model["warm_cache_hits"]
    misses = model["warm_cache_misses"]
    if not isinstance(cold, list) or len(cold) != 2 or cold != [409, 423]:
        raise ContractError("cold baseline population drifted")
    if warm != 435 or hits != 827 or misses != 0:
        raise ContractError("warm zero-miss observation drifted")
    for key in ("warm_cache_errors", "warm_read_errors", "warm_write_errors"):
        if model[key] != 0:
            raise ContractError(f"warm observation has a non-zero {key}")
    midpoint = sum(cold) / len(cold)
    delta = warm - midpoint
    delta_range = [warm - max(cold), warm - min(cold)]
    if midpoint != model["cold_baseline_midpoint_seconds"] or delta != model["observed_delta_seconds"]:
        raise ContractError("derived cold midpoint or delta is not reproducible")
    if delta_range != model["observed_delta_range_seconds"]:
        raise ContractError("derived baseline range is not reproducible")
    # With no misses, a miss/write cost contributes exactly zero. Do not infer
    # a write-path explanation from a treatment that performed no writes.
    write_contribution = misses * 0
    if write_contribution != model["write_cost_contribution_seconds"]:
        raise ContractError("zero-miss write contribution is not zero")
    return {
        "cold_midpoint_seconds": midpoint,
        "warm_seconds": warm,
        "warm_minus_cold_midpoint_seconds": delta,
        "warm_minus_cold_range_seconds": delta_range,
        "hits": hits,
        "misses": misses,
        "write_cost_contribution_seconds": write_contribution,
        "verdict": "negative_observation_preserved",
        "closure": "external_paired_measurement_required",
    }


def verify(manifest: dict[str, Any]) -> dict[str, Any]:
    check_paths(manifest)
    check_current_path(manifest)
    check_server_cause()
    report = cost_report(manifest["observed_cost_model"])
    acceptance = manifest["acceptance"]
    if acceptance["result_labels"] != ["faster", "slower", "indeterminate"]:
        raise ContractError("result labels drifted")
    if manifest["status"] != "open_external_measurement_required":
        raise ContractError("closure status drifted")
    return {
        "schema": EXPECTED_SCHEMA,
        "issue": 1661,
        "backlog_id": "B-105",
        "status": manifest["status"],
        "credentialless": True,
        "network_calls": False,
        "mutating_actions": False,
        "runner_contract": manifest["runner_contract"],
        "cost_model": report,
        "source_paths": manifest["sources"],
    }


def self_test(manifest: dict[str, Any]) -> None:
    """Exercise the negative path so a future verifier cannot hide it."""
    report = verify(manifest)
    if report["cost_model"]["verdict"] != "negative_observation_preserved":
        raise ContractError("negative result was not preserved")
    mutated = copy.deepcopy(manifest)
    mutated["observed_cost_model"]["warm_cache_misses"] = 1
    try:
        cost_report(mutated["observed_cost_model"])
    except ContractError:
        pass
    else:
        raise ContractError("miss mutation did not invalidate the zero-write model")
    mutated = copy.deepcopy(manifest)
    mutated["current_path"]["workflow_cache_chain"] = "disk,webdav"
    try:
        check_current_path(mutated)
    except ContractError:
        pass
    else:
        raise ContractError("cache-chain mutation did not invalidate the current-path contract")
    sources = manifest["sources"]
    workflow = text(sources["hosted_lane_workflow"])
    collector = text("scripts/collect_b105_same_lane.py")
    bad_workflow = workflow.replace("runs-on: ubuntu-24.04", "runs-on: corelink", 1)
    try:
        check_hosted_lane(bad_workflow, collector)
    except ContractError:
        pass
    else:
        raise ContractError("self-hosted runner mutation did not invalidate the hosted lane")
    bad_collector = collector.replace("self.send_collection_root(parsed.path)", 'upstream_key = ""', 1)
    try:
        check_hosted_lane(workflow, bad_collector)
    except ContractError:
        pass
    else:
        raise ContractError("tenant-root collection forwarding mutation did not invalidate the lane")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    try:
        manifest = load_manifest()
        if args.self_test:
            self_test(manifest)
        report = verify(manifest)
    except ContractError as exc:
        print(f"B-105 contract: FAIL: {exc}", file=sys.stderr)
        return 2
    if args.json or args.self_test:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print("B-105 contract: PASS (negative result preserved; production measurement still required)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
