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
    run_shard,
    validate_plan,
)
from scripts.verify_i2457_mutants_shards import VerificationError as MutantsEvidenceVerificationError
from scripts.verify_i2457_mutants_shards import config_digest, validate_baseline


HISTORICAL_SHARD_31 = Path(__file__).with_name("fixtures") / "i2638-shard-31-36195446816.json"


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

    def test_build_plan_excludes_ordinary_examples_and_keeps_test_enabled_examples(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / "crates" / "pkg"
            target = root / "target"
            executable = target / "debug" / "deps" / "unit"
            ordinary_example = target / "debug" / "examples" / "ordinary"
            test_example = target / "debug" / "examples" / "test-enabled"
            package.mkdir(parents=True)
            executable.parent.mkdir(parents=True)
            executable.write_text("#!/bin/sh\nprintf 'alpha: test\\nbeta: test\\n2 tests, 0 benchmarks\\n'\n", encoding="utf-8")
            ordinary_example.parent.mkdir(parents=True)
            ordinary_example.write_text("#!/bin/sh\nexit 91\n", encoding="utf-8")
            test_example.write_text("#!/bin/sh\nprintf 'example_test: test\\n1 test, 0 benchmarks\\n'\n", encoding="utf-8")
            os.chmod(executable, 0o755)
            os.chmod(ordinary_example, 0o755)
            os.chmod(test_example, 0o755)
            metadata = {"workspace_members": ["pkg-id"], "packages": [{"id": "pkg-id", "name": "pkg", "manifest_path": str(package / "Cargo.toml"), "targets": [{"name": "pkg", "kind": ["lib"], "test": True}, {"name": "ordinary", "kind": ["example"], "test": False}, {"name": "test-enabled", "kind": ["example"], "test": True}]}]}
            artifacts = [
                {"reason": "compiler-artifact", "package_id": "pkg-id", "profile": {"test": True}, "executable": str(executable), "target": {"name": "pkg", "kind": ["lib"]}},
                {"reason": "compiler-artifact", "package_id": "pkg-id", "profile": {"test": True}, "executable": str(ordinary_example), "target": {"name": "ordinary", "kind": ["example"]}},
                {"reason": "compiler-artifact", "package_id": "pkg-id", "profile": {"test": True}, "executable": str(test_example), "target": {"name": "test-enabled", "kind": ["example"]}},
            ]
            (root / "metadata.json").write_text(json.dumps(metadata), encoding="utf-8")
            (root / "artifacts.jsonl").write_text("".join(json.dumps(item) + "\n" for item in artifacts), encoding="utf-8")
            output = root / "plan.json"
            build_plan(Namespace(metadata=root / "metadata.json", artifacts=root / "artifacts.jsonl", workspace=root, target_dir=target, sha=self.sha, run_id=self.run_id, run_attempt=1, out=output))
            plan = json.loads(output.read_text(encoding="utf-8"))
            binary_entries = {item["id"]: item for item in plan["entries"] if item["kind"] == "binary"}
            self.assertEqual(binary_entries["binary:pkg:lib:pkg"]["test_names"], ["alpha", "beta"])
            self.assertEqual(binary_entries["binary:pkg:example:test-enabled"]["test_names"], ["example_test"])
            self.assertNotIn("binary:pkg:example:ordinary", binary_entries)

    def test_rejects_unsafe_or_tampered_mapping(self) -> None:
        plan = self.plan()
        plan["entries"][0]["executable"] = "../outside"
        with self.assertRaisesRegex(VerificationError, "unsafe"):
            validate_plan(plan)
        plan = self.plan()
        plan["entries"].pop()
        with self.assertRaisesRegex(VerificationError, "digest"):
            validate_plan(plan)

    def test_run_shard_executes_workspace_relative_binary_from_package_directory(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            workspace = root / "workspace"
            target = workspace / "target"
            package = workspace / "crates" / "pkg"
            package.mkdir(parents=True)
            plan = self.plan()
            for entry in membership(plan["entries"], 0):
                executable = target / entry["executable"]
                executable.parent.mkdir(parents=True, exist_ok=True)
                executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
                os.chmod(executable, 0o755)
            plan_path = root / "plan.json"
            plan_path.write_text(json.dumps(plan), encoding="utf-8")
            output = root / "receipt.json"

            run_shard(Namespace(plan=plan_path, workspace=workspace, target_dir=target, sha=self.sha, run_id=self.run_id, run_attempt=1, shard=0, out=output))

            receipt = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(receipt["status"], "success")
            self.assertEqual(receipt["completed_entry_ids"], receipt["entry_ids"])

    def test_run_shard_writes_failure_receipt_when_binary_is_missing(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            workspace = root / "workspace"
            (workspace / "crates" / "pkg").mkdir(parents=True)
            target = workspace / "target"
            plan = self.plan()
            plan_path = root / "plan.json"
            plan_path.write_text(json.dumps(plan), encoding="utf-8")
            output = root / "receipt.json"

            with self.assertRaisesRegex(SystemExit, "failed entries"):
                run_shard(Namespace(plan=plan_path, workspace=workspace, target_dir=target, sha=self.sha, run_id=self.run_id, run_attempt=1, shard=0, out=output))

            receipt = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(receipt["status"], "failure")
            self.assertEqual(receipt["failed_entry_ids"], receipt["entry_ids"])

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
            args = Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, config_digest=config_digest(), inventory_digest="c" * 64, out=output)
            aggregate(args)
            self.assertEqual(json.loads(output.read_text())["schema"], RECEIPT_SCHEMA)
            (root / "shard-0" / "baseline-shard-receipt.json").unlink()
            with self.assertRaisesRegex(VerificationError, "incomplete"):
                aggregate(args)

    def test_aggregate_retains_failed_entry_as_a_failed_baseline(self) -> None:
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
            args = Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, config_digest=config_digest(), inventory_digest="c" * 64, out=root / "baseline.json")
            self.assertEqual(aggregate(args), 1)
            baseline = json.loads(args.out.read_text(encoding="utf-8"))
            self.assertEqual((baseline["status"], baseline["exit_code"]), ("failure", 1))
            with self.assertRaisesRegex(MutantsEvidenceVerificationError, "did not succeed"):
                validate_baseline(
                    baseline,
                    {
                        "run_id": self.run_id,
                        "sha": self.sha,
                        "tool_version": "27.0.0",
                        "config_digest": args.config_digest,
                        "inventory_digest": args.inventory_digest,
                    },
                )

    def test_aggregate_retains_historical_shard_31_execution_as_failed_baseline(self) -> None:
        """Run 36195446816 shard 31 completed every entry but one test failed."""
        historical = json.loads(HISTORICAL_SHARD_31.read_text(encoding="utf-8"))
        self.assertEqual(set(historical["completed_entry_ids"] + historical["failed_entry_ids"]), set(historical["entry_ids"]))
        self.assertEqual(historical["status"], "failure")
        entries = [
            {"id": f"binary:pkg:lib:unit-{index}", "kind": "binary", "package": "pkg", "working_directory": "crates/pkg", "executable": f"debug/deps/unit-{index}", "test_names": [f"test_{index}"]}
            for index in range(SHARD_COUNT * len(historical["entry_ids"]))
        ]
        for index, entry_id in zip(range(31, len(entries), SHARD_COUNT), historical["entry_ids"], strict=True):
            entries[index]["id"] = entry_id
        plan = {
            "schema": SCHEMA,
            "run_id": historical["run_id"],
            "run_attempt": historical["run_attempt"],
            "sha": historical["sha"],
            "command": "cargo test --workspace --locked",
            "shard_count": SHARD_COUNT,
            "entries": entries,
        }
        plan["plan_digest"] = digest({"entries": entries, "shard_count": SHARD_COUNT, "command": plan["command"]})
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "plan").mkdir()
            (root / "plan" / "baseline-test-plan.json").write_text(json.dumps(plan), encoding="utf-8")
            for index in range(SHARD_COUNT):
                directory = root / f"shard-{index}"
                directory.mkdir()
                receipt = self.shard(plan, index)
                receipt["run_id"] = plan["run_id"]
                receipt["run_attempt"] = plan["run_attempt"]
                receipt["sha"] = plan["sha"]
                if index == 31:
                    receipt = copy.deepcopy(historical)
                    receipt["plan_digest"] = plan["plan_digest"]
                (directory / "baseline-shard-receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
            args = Namespace(artifacts=root, sha=historical["sha"], run_id=historical["run_id"], run_attempt=historical["run_attempt"], config_digest=config_digest(), inventory_digest="c" * 64, out=root / "baseline.json")
            self.assertEqual(aggregate(args), 1)
            baseline = json.loads(args.out.read_text(encoding="utf-8"))
            self.assertEqual((baseline["status"], baseline["exit_code"]), ("failure", 1))

    def test_aggregate_rejects_malformed_or_duplicate_completion_evidence(self) -> None:
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
                    receipt["completed_entry_ids"] = receipt["completed_entry_ids"] + [receipt["entry_ids"][0]]
                (directory / "baseline-shard-receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
            args = Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, config_digest=config_digest(), inventory_digest="c" * 64, out=root / "baseline.json")
            with self.assertRaisesRegex(VerificationError, "completion evidence"):
                aggregate(args)

    def test_aggregate_rejects_wrong_run_or_sha_receipts(self) -> None:
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
                    receipt["run_id"] = "forged-run"
                (directory / "baseline-shard-receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
            args = Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, config_digest=config_digest(), inventory_digest="c" * 64, out=root / "baseline.json")
            with self.assertRaisesRegex(VerificationError, "provenance"):
                aggregate(args)

    def test_aggregate_rejects_a_malformed_shard_identity_alongside_valid_coverage(self) -> None:
        plan = self.plan()
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "plan").mkdir()
            (root / "plan" / "baseline-test-plan.json").write_text(json.dumps(plan), encoding="utf-8")
            for index in range(SHARD_COUNT):
                directory = root / f"shard-{index}"
                directory.mkdir()
                (directory / "baseline-shard-receipt.json").write_text(json.dumps(self.shard(plan, index)), encoding="utf-8")
            malformed = self.shard(plan, 0)
            malformed["shard"] = "0"
            extra = root / "malformed"
            extra.mkdir()
            (extra / "baseline-shard-receipt.json").write_text(json.dumps(malformed), encoding="utf-8")
            args = Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, config_digest=config_digest(), inventory_digest="c" * 64, out=root / "baseline.json")
            with self.assertRaisesRegex(VerificationError, "identity"):
                aggregate(args)


if __name__ == "__main__":
    unittest.main()
