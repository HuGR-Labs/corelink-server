"""Adversarial aggregate tests for deterministic hosted mutation shards."""

from __future__ import annotations

import copy
import json
from argparse import Namespace
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from scripts.verify_i2457_mutants_shards import (
    BASELINE_SCHEMA,
    SHARD_COUNT,
    SHARD_SCHEMA,
    VerificationError,
    aggregate_receipts,
    command_aggregate,
    config_digest,
    directory_digest,
    digest,
    make_inventory,
    membership,
    shard_artifact_digest,
    validate_inventory,
)


class MutantsShardAggregateTests(unittest.TestCase):
    @staticmethod
    def raw_mutant(
        index: int,
        *,
        name: str | None = None,
        file: str | None = None,
        line: int | None = None,
        replacement: str | None = None,
        diff: str | None = None,
    ) -> dict:
        line = index + 1 if line is None else line
        return {
            "name": name or f"mutant-{index}",
            "package": "corelink-test-package",
            "file": file or "crates/corelink-test/src/lib.rs",
            "span": {
                "start": {"line": line, "column": 1},
                "end": {"line": line, "column": 2},
            },
            "function": None,
            "replacement": str(index) if replacement is None else replacement,
            "genre": "FnValue",
            "diff": f"--- original-{index}\\n+++ replacement-{index}\\n" if diff is None else diff,
        }

    def setUp(self) -> None:
        self.sha = "a" * 40
        self.run_id = "777"
        raw = [self.raw_mutant(index) for index in range(SHARD_COUNT * 2)]
        self.inventory = make_inventory(raw, self.sha, self.run_id, 1)
        self.baseline = {
            "schema": BASELINE_SCHEMA,
            "run_id": self.run_id,
            "run_attempt": 1,
            "sha": self.sha,
            "tool_version": "27.0.0",
            "config_digest": config_digest(),
            "inventory_digest": self.inventory["inventory_digest"],
            "baseline_digest": digest(["baseline"]),
            "status": "success",
            "exit_code": 0,
        }
        self.shards = [
            {
                "schema": SHARD_SCHEMA,
                "run_id": self.run_id,
                "run_attempt": 1,
                "sha": self.sha,
                "tool_version": "27.0.0",
                "config_digest": config_digest(),
                "inventory_digest": self.inventory["inventory_digest"],
                "baseline_digest": self.baseline["baseline_digest"],
                "shard": {"index": index, "total": SHARD_COUNT, "sharding": "round-robin"},
                "mutant_ids": membership(self.inventory["mutant_ids"], index),
                "membership_digest": digest(membership(self.inventory["mutant_ids"], index)),
                "evidence_digest": digest([f"shard-{index}"]),
                "artifact_digest": digest([f"artifact-{index}"]),
                "evidence_present": True,
                "status": "success",
                "cargo_exit_code": 0,
                "outcomes_complete": True,
            }
            for index in range(SHARD_COUNT)
        ]

    def downloaded(self, shards=None):
        return {
            (item["shard"]["index"], item["run_attempt"]): {
                "evidence_digest": item["evidence_digest"],
                "artifact_digest": item["artifact_digest"],
            }
            for item in (shards or self.shards)
        }

    def aggregate(self, inventory=None, baseline=None, shards=None, attempt=1, downloaded=None):
        return aggregate_receipts(
            inventory or [self.inventory],
            baseline or [self.baseline],
            shards or self.shards,
            self.sha,
            self.run_id,
            attempt,
            downloaded,
        )

    def test_accepts_exact_complete_disjoint_union(self) -> None:
        receipt = self.aggregate(downloaded=self.downloaded())
        self.assertEqual(receipt["covered_mutants"], SHARD_COUNT * 2)
        self.assertEqual(receipt["attempt_lineage"], [1])
        self.assertEqual(len(receipt["shard_artifact_digests"]), SHARD_COUNT)

    def test_inventory_receipt_uses_redacted_string_identities(self) -> None:
        self.assertEqual(validate_inventory(self.inventory), self.inventory["mutant_ids"])
        invalid = copy.deepcopy(self.inventory)
        invalid["mutant_ids"][0] = {"name": "mutant-0"}
        with self.assertRaisesRegex(VerificationError, "string identities"):
            validate_inventory(invalid)
        invalid = copy.deepcopy(self.inventory)
        invalid["inventory_shard"] = "0/27"
        with self.assertRaisesRegex(VerificationError, "complete denominator-one"):
            validate_inventory(invalid)

    def test_canonical_identity_distinguishes_duplicate_display_names_by_full_record(self) -> None:
        raw = [
            self.raw_mutant(
                0, name="same display name", file="crates/one/src/lib.rs", line=10,
                replacement="0", diff="--- crates/one/src/lib.rs\\n+++ replacement A\\n",
            ),
            self.raw_mutant(
                1, name="same display name", file="crates/one/src/lib.rs", line=10,
                replacement="0", diff="--- crates/one/src/lib.rs\\n+++ replacement B\\n",
            ),
        ]
        first = make_inventory(raw, self.sha, self.run_id, 1)
        second = make_inventory(copy.deepcopy(raw), self.sha, self.run_id, 1)
        self.assertEqual(first["mutant_ids"], second["mutant_ids"])
        self.assertEqual(len(first["mutant_ids"]), len(set(first["mutant_ids"])))
        self.assertTrue(all(identity.startswith("sha256:") for identity in first["mutant_ids"]))

    def test_canonical_identity_rejects_an_identical_duplicate(self) -> None:
        raw = self.raw_mutant(0, name="same display name", file="crates/one/src/lib.rs", line=10)
        with self.assertRaisesRegex(VerificationError, "duplicate identities"):
            make_inventory([raw, copy.deepcopy(raw)], self.sha, self.run_id, 1)

    def test_canonical_identity_rejects_a_truncated_or_drifted_list_record(self) -> None:
        raw = self.raw_mutant(0)
        del raw["diff"]
        with self.assertRaisesRegex(VerificationError, "pinned cargo-mutants v27 list shape"):
            make_inventory([raw], self.sha, self.run_id, 1)

    def test_rejects_missing_duplicate_and_unexpected_shards(self) -> None:
        with self.assertRaisesRegex(VerificationError, "missing"):
            self.aggregate(shards=self.shards[:-1])
        duplicate = copy.deepcopy(self.shards)
        duplicate.append(copy.deepcopy(duplicate[0]))
        with self.assertRaisesRegex(VerificationError, "duplicate shard 0 receipt"):
            self.aggregate(shards=duplicate)
        unexpected = copy.deepcopy(self.shards)
        unexpected[0]["shard"]["index"] = SHARD_COUNT
        with self.assertRaisesRegex(VerificationError, "unexpected"):
            self.aggregate(shards=unexpected)

    def test_rejects_missing_duplicate_or_unexpected_mutant_coverage(self) -> None:
        missing = copy.deepcopy(self.shards)
        missing[0]["mutant_ids"] = missing[0]["mutant_ids"][1:]
        with self.assertRaisesRegex(VerificationError, "deterministic membership"):
            self.aggregate(shards=missing)
        duplicate = copy.deepcopy(self.shards)
        duplicate[1]["mutant_ids"][0] = duplicate[0]["mutant_ids"][0]
        with self.assertRaisesRegex(VerificationError, "deterministic membership"):
            self.aggregate(shards=duplicate)

    def test_rejects_mixed_sha_tool_configuration_or_run(self) -> None:
        for field, value, expected in (
            ("sha", "b" * 40, "does not bind"),
            ("tool_version", "27.0.1", "does not bind"),
            ("config_digest", "0" * 64, "does not bind"),
            ("run_id", "other-run", "does not bind"),
        ):
            with self.subTest(field=field):
                shards = copy.deepcopy(self.shards)
                shards[0][field] = value
                with self.assertRaisesRegex(VerificationError, expected):
                    self.aggregate(shards=shards)

    def test_rejects_failed_cancelled_timeout_or_nonterminal_shards(self) -> None:
        for status, code in (("failure", 1), ("cancelled", 125), ("timeout", 124), ("in_progress", 0)):
            with self.subTest(status=status):
                shards = copy.deepcopy(self.shards)
                shards[4]["status"] = status
                shards[4]["cargo_exit_code"] = code
                with self.assertRaisesRegex(VerificationError, "did not complete successfully"):
                    self.aggregate(shards=shards)

    def test_rejects_failed_baseline_and_bad_attempt_lineage(self) -> None:
        baseline = copy.deepcopy(self.baseline)
        baseline["status"] = "failure"
        with self.assertRaisesRegex(VerificationError, "baseline did not succeed"):
            self.aggregate(baseline=[baseline])
        shard = copy.deepcopy(self.shards)
        shard[1]["run_attempt"] = 2
        with self.assertRaisesRegex(VerificationError, "future attempt"):
            self.aggregate(shards=shard, attempt=1)

    def test_accepts_only_latest_successful_same_run_retry(self) -> None:
        failed = copy.deepcopy(self.shards[3])
        failed["run_attempt"] = 1
        failed["status"] = "failure"
        failed["cargo_exit_code"] = 1
        retried = copy.deepcopy(self.shards[3])
        retried["run_attempt"] = 2
        shards = [item for index, item in enumerate(self.shards) if index != 3] + [failed, retried]
        receipt = self.aggregate(shards=shards, attempt=2)
        self.assertEqual(receipt["attempt_lineage"], [1, 2])

    def test_rejects_corrupt_or_absent_evidence(self) -> None:
        for field, value in (("evidence_present", False), ("evidence_digest", "bad"), ("outcomes_complete", False)):
            with self.subTest(field=field):
                shards = copy.deepcopy(self.shards)
                shards[8][field] = value
                with self.assertRaises(VerificationError):
                    self.aggregate(shards=shards)

    def test_rejects_tampered_or_missing_downloaded_evidence_bytes(self) -> None:
        downloaded = self.downloaded()
        downloaded[(8, 1)]["evidence_digest"] = digest(["tampered-evidence"])
        with self.assertRaisesRegex(VerificationError, "downloaded evidence bytes"):
            self.aggregate(downloaded=downloaded)
        downloaded = self.downloaded()
        del downloaded[(8, 1)]
        with self.assertRaisesRegex(VerificationError, "downloaded evidence is missing"):
            self.aggregate(downloaded=downloaded)

    def test_rejects_receipt_or_downloaded_artifact_digest_mismatch(self) -> None:
        shards = copy.deepcopy(self.shards)
        shards[8]["artifact_digest"] = digest(["tampered-receipt"])
        with self.assertRaisesRegex(VerificationError, "downloaded artifact bytes"):
            self.aggregate(shards=shards, downloaded=self.downloaded())
        downloaded = self.downloaded()
        downloaded[(8, 1)]["artifact_digest"] = digest(["tampered-artifact"])
        with self.assertRaisesRegex(VerificationError, "downloaded artifact bytes"):
            self.aggregate(downloaded=downloaded)

    def test_command_aggregate_reads_upload_artifact_lca_layout_and_rejects_missing_bytes(self) -> None:
        """Exercise the actual download shape, not a precomputed digest map."""
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            inventory_dir = root / "mutants-campaign-inventory-777-1"
            baseline_dir = root / "mutants-baseline-777-1"
            inventory_dir.mkdir()
            baseline_dir.mkdir()
            (inventory_dir / "inventory-manifest.json").write_text(json.dumps(self.inventory), encoding="utf-8")
            (baseline_dir / "baseline-receipt.json").write_text(json.dumps(self.baseline), encoding="utf-8")
            evidence_paths = []
            for shard in self.shards:
                index = shard["shard"]["index"]
                artifact_root = root / f"mutants-shard-{index}-{self.run_id}-1"
                evidence = artifact_root / f"mutants-shard-{index}" / "mutants.out"
                evidence.mkdir(parents=True)
                expected = artifact_root / "expected-shard.json"
                raw = [{"name": name} for name in shard["mutant_ids"]]
                expected.write_text(json.dumps(raw), encoding="utf-8")
                (evidence / "mutants.json").write_text(json.dumps(raw), encoding="utf-8")
                (evidence / "outcomes.json").write_text("{}", encoding="utf-8")
                receipt = copy.deepcopy(shard)
                receipt["evidence_digest"] = directory_digest(evidence)
                receipt["artifact_digest"] = shard_artifact_digest(expected, evidence)
                (artifact_root / "shard-receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
                evidence_paths.append(evidence)

            output = root / "aggregate.json"
            command_aggregate(
                Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, out=output)
            )
            aggregate = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(len(aggregate["shard_artifact_digests"]), SHARD_COUNT)

            (evidence_paths[8] / "outcomes.json").unlink()
            with self.assertRaisesRegex(VerificationError, "downloaded evidence bytes"):
                command_aggregate(
                    Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, out=output)
                )


if __name__ == "__main__":
    unittest.main()
