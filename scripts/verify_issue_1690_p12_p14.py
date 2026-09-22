#!/usr/bin/env python3
"""Verify #1690's P12-P14 content after the #1746 squash merge.

Commit ancestry from the reviewed branch is intentionally not required on
current main: squash merge rewrites that ancestry.  The reachable #1746 merge
commit, its exact path inventory, and the landed blob ids are the provenance
contract for current main.  The historical source ids stay in the report.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

SHA_RE = re.compile(r"^[0-9a-f]{40}$")

# These values are independently anchored in the verifier.  The JSON file is
# the reviewable inventory, but changing that file alone must never redefine
# what this lane accepts.
EXPECTED_HISTORICAL_TREE = "48e15ff30b0556a1f366388a61b617ad01f5d363"
EXPECTED_HISTORICAL_SOURCES = {
    "p12": "84b99ae353c67eda6096633c7c7d7478ea49df3c",
    "p13": "a75431b8eadf1d2afaa55bee843fe945bacb7a8d",
    "p14": "432b4007a3accbbd56e4fff59358ce9e0c309d40",
}
EXPECTED_SQUASH_COMMIT = "e70d9d9349f1ae96d71fd918817af8ad2bcd4762"
EXPECTED_SQUASH_PARENT = "2e43814b41ffc2c9f9ed02c7819f2ab2d32a3859"
EXPECTED_SQUASH_TREE = "b457d53f281061bdfd3f95a5216ea103f11b39ea"
EXPECTED_SOURCE_INVENTORY = {
    "p12": {
        "commit": "2963bd510ff03df932c8a60e91b1c23742fbe1b6",
        "paths": [
            "worker/src/durable_object.ts",
            "worker/src/lib/devenv_cleanup_route.ts",
            "worker/tests/b126_m3_refactor.test.ts",
            "worker/tests/durable_object_part2_test_helpers.ts",
        ],
    },
    "p13": {
        "commit": "4e1844f7f5c644b5f4f01fc32653399e70918124",
        "paths": [
            "worker/src/index_special_customer.ts",
            "worker/tests/index_part5.test.ts",
            "worker/tests/index_special_customer_devenv_quota.test.ts",
            "worker/tests/quota_batch.test.ts",
            "worker/tests/region-map.test.ts",
        ],
    },
    "p14": {
        "commit": "59cfb2ca01c0732cede1d2a2693a3dea7ef1da59",
        "paths": [
            "worker/tests/do_miniflare_integration.miniflare.test.ts",
            "worker/tests/durable_object_part2_tests_1.ts",
        ],
    },
}
EXPECTED_PATH_BLOBS = {
    "worker/src/durable_object.ts": "1ea100789b9514ea1c7caf67212fe3b162bbe3e4",
    "worker/src/index_special_customer.ts": "3de39b8a8e4ac4d9e6d5161385b476c7453b71d3",
    "worker/src/lib/devenv_cleanup_route.ts": "9174b7800e380995d3514959246218a3a18c1b1c",
    "worker/tests/b126_m3_refactor.test.ts": "db20fffa4424e722e1d1bc3fc212f7087c8b853e",
    "worker/tests/do_miniflare_integration.miniflare.test.ts": "ea3807b9169bf1364ee442b889b406998848abde",
    "worker/tests/durable_object_part2_test_helpers.ts": "31871fdaa42d49a36e3800326f2968486533607a",
    "worker/tests/durable_object_part2_tests_1.ts": "7466a04b1658735dad6730518b0ae82de0bd96b3",
    "worker/tests/index_part5.test.ts": "21f4aec344cb570eb91b1bfb9ed17ab45e21cf6e",
    "worker/tests/index_special_customer_devenv_quota.test.ts": "4f69f90abb0fb8bd82b5e7340c32d7a2ca495d07",
    "worker/tests/quota_batch.test.ts": "b31adf04021c8db2e4134cf4f933545729280a50",
    "worker/tests/region-map.test.ts": "2b9f667977f61597764a2c85f14d2c896d22253e",
}
EXPECTED_PATH_COUNT = 11


class VerificationError(Exception):
    pass


def git(*args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode:
        raise VerificationError(f"git {' '.join(args)} failed")
    return result.stdout.strip()


def git_optional(*args: str) -> str | None:
    result = subprocess.run(
        ["git", *args],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode:
        return None
    return result.stdout.strip()


def require_sha(value: Any, label: str) -> str:
    if not isinstance(value, str) or not SHA_RE.fullmatch(value):
        raise VerificationError(f"{label} must be a 40-character lowercase SHA")
    return value


def commit_paths(commit: str) -> list[str]:
    output = git("diff-tree", "--no-commit-id", "--name-only", "-r", commit)
    return sorted(path for path in output.splitlines() if path)


def commit_exists(commit: str) -> bool:
    result = subprocess.run(
        ["git", "cat-file", "-e", f"{commit}^{{commit}}"],
        check=False,
        capture_output=True,
    )
    return result.returncode == 0


def ancestor(commit: str, head: str) -> bool:
    result = subprocess.run(
        ["git", "merge-base", "--is-ancestor", commit, head],
        check=False,
        capture_output=True,
    )
    if result.returncode not in (0, 1):
        raise VerificationError(f"cannot evaluate ancestry for {commit}")
    return result.returncode == 0


def load_manifest(path: Path) -> dict[str, Any]:
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise VerificationError(f"manifest cannot be read: {exc}") from exc
    if not isinstance(manifest, dict) or manifest.get("schema_version") != "2":
        raise VerificationError("manifest schema_version must be 2")
    if manifest.get("issue") != 1690:
        raise VerificationError("manifest issue must be 1690")
    return manifest


def verify(manifest: dict[str, Any], head: str, github_sha: str) -> dict[str, Any]:
    require_sha(head, "head")
    require_sha(github_sha, "github_sha")
    if head != github_sha:
        raise VerificationError("checked out HEAD differs from GITHUB_SHA")

    historical = manifest.get("historical_test")
    squash = manifest.get("squash_provenance")
    if not isinstance(historical, dict) or not isinstance(squash, dict):
        raise VerificationError("historical_test and squash_provenance are required")

    historical_tree = require_sha(historical.get("tree"), "historical tree")
    historical_sources = historical.get("source_commits")
    if not isinstance(historical_sources, dict) or set(historical_sources) != {"p12", "p13", "p14"}:
        raise VerificationError("historical source commit inventory is incomplete")
    for stage, commit in historical_sources.items():
        require_sha(commit, f"historical {stage} commit")

    merge_commit = require_sha(squash.get("merge_commit"), "squash merge commit")
    merge_parent = require_sha(squash.get("merge_parent"), "squash merge parent")
    squash_tree = require_sha(squash.get("tree"), "squash tree")
    source_inventory = squash.get("source_commits")
    path_blobs = squash.get("path_blobs")
    if not isinstance(source_inventory, dict) or set(source_inventory) != {"p12", "p13", "p14"}:
        raise VerificationError("squash source commit inventory is incomplete")
    if not isinstance(path_blobs, dict) or not path_blobs:
        raise VerificationError("squash path blob inventory is empty")

    expected_paths: set[str] = set()
    for stage, entry in source_inventory.items():
        if not isinstance(entry, dict):
            raise VerificationError(f"squash {stage} entry is invalid")
        commit = require_sha(entry.get("commit"), f"squash {stage} commit")
        paths = entry.get("paths")
        if not isinstance(paths, list) or not paths or any(not isinstance(p, str) or not p for p in paths):
            raise VerificationError(f"squash {stage} path inventory is invalid")
        if len(set(paths)) != len(paths):
            raise VerificationError(f"squash {stage} path inventory contains duplicates")
        expected_paths.update(paths)
        entry["commit"] = commit
        entry["paths"] = sorted(paths)

    if set(path_blobs) != expected_paths:
        raise VerificationError("path blob inventory does not equal the staged path inventory")
    for path, blob in path_blobs.items():
        if not isinstance(path, str) or not path or not isinstance(blob, str) or not SHA_RE.fullmatch(blob):
            raise VerificationError("path blob inventory contains an invalid entry")

    if historical_tree != EXPECTED_HISTORICAL_TREE:
        raise VerificationError("manifest historical tree differs from the verifier anchor")
    if historical_sources != EXPECTED_HISTORICAL_SOURCES:
        raise VerificationError("manifest historical source IDs differ from the verifier anchors")
    if merge_commit != EXPECTED_SQUASH_COMMIT:
        raise VerificationError("manifest squash commit differs from the verifier anchor")
    if merge_parent != EXPECTED_SQUASH_PARENT:
        raise VerificationError("manifest squash parent differs from the verifier anchor")
    if squash_tree != EXPECTED_SQUASH_TREE:
        raise VerificationError("manifest squash tree differs from the verifier anchor")
    normalized_inventory = {
        stage: {"commit": entry["commit"], "paths": sorted(entry["paths"])}
        for stage, entry in source_inventory.items()
    }
    if normalized_inventory != EXPECTED_SOURCE_INVENTORY:
        raise VerificationError("manifest source inventory differs from the verifier anchors")
    if len(path_blobs) != EXPECTED_PATH_COUNT:
        raise VerificationError("manifest path inventory does not contain exactly 11 paths")
    if path_blobs != EXPECTED_PATH_BLOBS:
        raise VerificationError("manifest path blob inventory differs from the verifier anchors")

    if not commit_exists(merge_commit):
        raise VerificationError("reachable squash merge commit is missing")
    observed_merge_parent = git("rev-parse", f"{merge_commit}^")
    observed_merge_tree = git("show", "-s", "--format=%T", merge_commit)
    if observed_merge_parent != merge_parent:
        raise VerificationError("squash merge parent does not match the manifest")
    if observed_merge_tree != squash_tree:
        raise VerificationError("squash merge tree does not match the manifest")
    if not ancestor(merge_commit, head):
        raise VerificationError("squash merge commit is not an ancestor of current main")

    observed_merge_paths = commit_paths(merge_commit)
    if observed_merge_paths != sorted(expected_paths):
        raise VerificationError("squash merge path inventory does not match the manifest")

    path_results: list[dict[str, Any]] = []
    mismatches: list[str] = []
    for path in sorted(path_blobs):
        expected_blob = path_blobs[path]
        observed_squash_blob = git_optional("rev-parse", f"{merge_commit}:{path}")
        observed_head_blob = git_optional("rev-parse", f"{head}:{path}")
        if observed_squash_blob != expected_blob or observed_head_blob != expected_blob:
            mismatches.append(path)
        path_results.append(
            {
                "path": path,
                "expected_blob": expected_blob,
                "squash_blob": observed_squash_blob,
                "head_blob": observed_head_blob,
                "matches": observed_squash_blob == expected_blob and observed_head_blob == expected_blob,
            }
        )
    if mismatches:
        raise VerificationError(f"current-main bundle path blobs differ: {', '.join(mismatches)}")

    source_results: dict[str, dict[str, Any]] = {}
    expected_parents = {
        "p12": merge_parent,
        "p13": source_inventory["p12"]["commit"],
        "p14": source_inventory["p13"]["commit"],
    }
    for stage, entry in source_inventory.items():
        commit = entry["commit"]
        present = commit_exists(commit)
        result: dict[str, Any] = {
            "commit": commit,
            "object_present": present,
            "ancestor_of_head": ancestor(commit, head) if present else False,
            "ancestry_rule": "historical source may be unreachable after squash; merge commit must be reachable",
        }
        if present:
            parents = git("rev-list", "--parents", "-n1", commit).split()
            observed_parent = parents[1] if len(parents) == 2 else None
            result["parent"] = observed_parent
            result["paths_match_inventory"] = commit_paths(commit) == sorted(entry["paths"])
            if observed_parent != expected_parents[stage] or not result["paths_match_inventory"]:
                raise VerificationError(f"squash {stage} source object conflicts with its inventory")
        source_results[stage] = result

    return {
        "schema_version": "2",
        "issue": 1690,
        "ref": "refs/heads/main",
        "head_sha": head,
        "github_sha": github_sha,
        "status": "PASS_SQUASH_PROVENANCE",
        "contract": {
            "historical_test_tree": historical_tree,
            "historical_tree_matches_current_main": git("show", "-s", "--format=%T", head) == historical_tree,
            "squash_merge_commit": merge_commit,
            "squash_merge_is_ancestor": True,
            "squash_tree": squash_tree,
            "current_main_tree": git("show", "-s", "--format=%T", head),
            "current_main_path_blobs_match_squash_tree": True,
            "ancestry_rule": "P12-P14 source ids are historical and may be unreachable after squash; the canonical #1746 squash commit and its path inventory must be reachable from main.",
        },
        "historical_source_commits": historical_sources,
        "squash_source_commits": source_results,
        "path_inventory": path_results,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--head", required=True)
    parser.add_argument("--github-sha", required=True)
    args = parser.parse_args()

    try:
        manifest = load_manifest(args.manifest)
        report = verify(manifest, args.head, args.github_sha)
    except VerificationError as exc:
        report = {
            "schema_version": "2",
            "issue": 1690,
            "ref": "refs/heads/main",
            "head_sha": args.head,
            "github_sha": args.github_sha,
            "status": "FAIL",
            "failure": str(exc),
        }
        print(json.dumps(report, indent=2, sort_keys=True))
        return 1

    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
