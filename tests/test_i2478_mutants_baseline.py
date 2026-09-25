"""Negative controls for the complete mapped #2478 unmutated baseline."""

from __future__ import annotations

import copy
import json
import os
import unittest
from argparse import Namespace
from pathlib import Path
from tempfile import TemporaryDirectory

from scripts.verify_i2478_mutants_baseline import (
    RECEIPT_SCHEMA,
    SCHEMA,
    SHARD_COUNT,
    SHARD_SCHEMA,
    VerificationError,
    aggregate,
    build_plan,
    digest,
    membership,
    validate_plan,
)


class BaselineMappingTests(unittest.TestCase):
    sha = "a" * 40
    run_id = "777"

    def plan(self) -> dict:
        entries = [
            {"id": f"binary:pkg:lib:unit-{index}", "kind": "binary", "package": "pkg", "working_directory": "crates/pkg", "executable": f"debug/deps/unit-{index}", "test_names": [f"test_{index}"]}
            for index in range(SHARD_COUNT + 1)
        ]
        plan = {"schema": SCHEMA, "run_id": self.run_id, "run_attempt": 1, "sha": self.sha, "command": "cargo test --workspace --locked", "shard_count": SHARD_COUNT, "entries": entries}
        plan["plan_digest"] = digest({"entries": entries, "shard_count": SHARD_COUNT, "command": plan["command"]})
        return plan

    def shard(self, plan: dict, index: int) -> dict:
        ids = [entry["id"] for entry in membership(plan["entries"], index)]
        return {"schema": SHARD_SCHEMA, "run_id": self.run_id, "run_attempt": 1, "sha": self.sha, "plan_digest": plan["plan_digest"], "shard": index, "entry_ids": ids, "completed_entry_ids": ids, "failed_entry_ids": [], "status": "success"}

    def test_mapping_is_complete_and_round_robin(self) -> None:
        plan = self.plan()
        self.assertEqual(validate_plan(plan), plan["entries"])
        self.assertEqual(sum(len(membership(plan["entries"], index)) for index in range(SHARD_COUNT)), len(plan["entries"]))

    def test_build_plan_records_each_binary_test_name(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / "crates" / "pkg"
            target = root / "target"
            executable = target / "debug" / "deps" / "unit"
            package.mkdir(parents=True)
            executable.parent.mkdir(parents=True)
            executable.write_text("#!/bin/sh\nprintf 'alpha: test\\nbeta: test\\n2 tests, 0 benchmarks\\n'\n", encoding="utf-8")
            os.chmod(executable, 0o755)
            metadata = {"workspace_members": ["pkg-id"], "packages": [{"id": "pkg-id", "name": "pkg", "manifest_path": str(package / "Cargo.toml"), "targets": [{"kind": ["lib"]}]}]}
            artifacts = {"reason": "compiler-artifact", "package_id": "pkg-id", "profile": {"test": True}, "executable": str(executable), "target": {"name": "pkg", "kind": ["lib"]}}
            (root / "metadata.json").write_text(json.dumps(metadata), encoding="utf-8")
            (root / "artifacts.jsonl").write_text(json.dumps(artifacts) + "\n", encoding="utf-8")
            output = root / "plan.json"
            build_plan(Namespace(metadata=root / "metadata.json", artifacts=root / "artifacts.jsonl", workspace=root, target_dir=target, sha=self.sha, run_id=self.run_id, run_attempt=1, out=output))
            plan = json.loads(output.read_text(encoding="utf-8"))
            binary = next(item for item in plan["entries"] if item["kind"] == "binary")
            self.assertEqual(binary["test_names"], ["alpha", "beta"])

    def test_rejects_unsafe_or_tampered_mapping(self) -> None:
        plan = self.plan()
        plan["entries"][0]["executable"] = "../outside"
        with self.assertRaisesRegex(VerificationError, "unsafe"):
            validate_plan(plan)
        plan = self.plan()
        plan["entries"].pop()
        with self.assertRaisesRegex(VerificationError, "digest"):
            validate_plan(plan)

    def test_aggregate_rejects_missing_or_unsuccessful_coverage(self) -> None:
        plan = self.plan()
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "plan").mkdir()
            (root / "plan" / "baseline-test-plan.json").write_text(json.dumps(plan), encoding="utf-8")
            for index in range(SHARD_COUNT):
                directory = root / f"shard-{index}"
                directory.mkdir()
                (directory / "baseline-shard-receipt.json").write_text(json.dumps(self.shard(plan, index)), encoding="utf-8")
            output = root / "baseline.json"
            args = Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, config_digest="b" * 64, inventory_digest="c" * 64, out=output)
            aggregate(args)
            self.assertEqual(json.loads(output.read_text())["schema"], RECEIPT_SCHEMA)
            (root / "shard-0" / "baseline-shard-receipt.json").unlink()
            with self.assertRaisesRegex(VerificationError, "incomplete"):
                aggregate(args)

    def test_aggregate_rejects_failed_entry_even_when_the_receipt_exists(self) -> None:
        plan = self.plan()
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "plan").mkdir()
            (root / "plan" / "baseline-test-plan.json").write_text(json.dumps(plan), encoding="utf-8")
            for index in range(SHARD_COUNT):
                directory = root / f"shard-{index}"
                directory.mkdir()
                receipt = self.shard(plan, index)
                if index == 4:
                    receipt = copy.deepcopy(receipt)
                    receipt["completed_entry_ids"] = []
                    receipt["failed_entry_ids"] = receipt["entry_ids"]
                    receipt["status"] = "failure"
                (directory / "baseline-shard-receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
            args = Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, config_digest="b" * 64, inventory_digest="c" * 64, out=root / "baseline.json")
            with self.assertRaisesRegex(VerificationError, "did not complete"):
                aggregate(args)


if __name__ == "__main__":
    unittest.main()
