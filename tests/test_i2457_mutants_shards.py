"""Adversarial aggregate tests for deterministic hosted mutation shards."""

from __future__ import annotations

import copy
import json
import shutil
from argparse import Namespace
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from scripts.verify_i2457_mutants_shards import (
    BASELINE_SCHEMA,
    EVIDENCE_SCHEMA,
    SHARD_COUNT,
    SHARD_SCHEMA,
    VerificationError,
    aggregate_receipts,
    command_aggregate,
    command_write_shard,
    config_digest,
    directory_digest,
    digest,
    make_inventory,
    membership,
    mutant_counts,
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
        raw[1] = copy.deepcopy(raw[0])
        self.raw = raw
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
            "plan_digest": digest(["complete-test-plan"]),
            "shard_count": SHARD_COUNT,
            "covered_entries": 1,
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
                "mutant_counts": mutant_counts(membership(self.inventory["mutant_ids"], index)),
                "membership_digest": digest(membership(self.inventory["mutant_ids"], index)),
                "evidence_digest": digest([f"shard-{index}"]),
                "artifact_digest": digest([f"artifact-{index}"]),
                "evidence_present": True,
                "status": "success",
                "cargo_exit_code": 0,
                "outcomes_complete": True,
                "outcome_counts": {"caught": len(membership(self.inventory["mutant_ids"], index)), "missed": 0, "success": 0, "timeout": 0, "unviable": 0},
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
        self.assertEqual(sum(item["occurrences"] for item in receipt["per_shard_occurrence_counts"]), SHARD_COUNT * 2)
        self.assertEqual(receipt["covered_mutant_counts"], self.inventory["mutant_counts"])
        self.assertEqual(receipt["attempt_lineage"], [1])
        self.assertEqual(len(receipt["shard_artifact_digests"]), SHARD_COUNT)

    def test_inventory_receipt_uses_redacted_string_identities(self) -> None:
        self.assertEqual(validate_inventory(self.inventory), self.inventory["mutant_ids"])
        invalid = copy.deepcopy(self.inventory)
        invalid["mutant_ids"][0] = {"name": "mutant-0"}
        with self.assertRaisesRegex(VerificationError, "string identities"):
            validate_inventory(invalid)
        invalid = copy.deepcopy(self.inventory)
        invalid["mutant_counts"][0]["count"] = 3
        with self.assertRaisesRegex(VerificationError, "multiplicity"):
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

    def test_canonical_identity_retains_an_identical_duplicate_as_two_occurrences(self) -> None:
        raw = self.raw_mutant(0, name="same display name", file="crates/one/src/lib.rs", line=10)
        inventory = make_inventory([raw, copy.deepcopy(raw)], self.sha, self.run_id, 1)
        self.assertEqual(len(inventory["mutant_ids"]), 2)
        self.assertEqual(inventory["mutant_counts"], [{"identity": inventory["mutant_ids"][0], "count": 2}])

    def test_equal_records_in_separate_shards_have_no_identity_collision(self) -> None:
        shared = self.inventory["mutant_ids"][0]
        self.assertEqual(self.inventory["mutant_ids"][1], shared)
        self.assertEqual(membership(self.inventory["mutant_ids"], 0), [shared, self.inventory["mutant_ids"][27]])
        self.assertEqual(membership(self.inventory["mutant_ids"], 1), [shared, self.inventory["mutant_ids"][28]])
        self.assertIn({"identity": shared, "count": 1}, self.shards[0]["mutant_counts"])
        self.assertIn({"identity": shared, "count": 1}, self.shards[1]["mutant_counts"])

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

    def test_rejects_missing_or_extra_equal_record_occurrence(self) -> None:
        missing = copy.deepcopy(self.shards)
        missing[0]["mutant_ids"] = missing[0]["mutant_ids"][1:]
        with self.assertRaisesRegex(VerificationError, "deterministic membership"):
            self.aggregate(shards=missing)
        extra = copy.deepcopy(self.shards)
        extra[1]["mutant_ids"].append(extra[1]["mutant_ids"][0])
        with self.assertRaisesRegex(VerificationError, "deterministic membership"):
            self.aggregate(shards=extra)

    def test_rejects_unexpected_mutant_coverage(self) -> None:
        unexpected = copy.deepcopy(self.shards)
        unexpected[1]["mutant_ids"][0] = unexpected[2]["mutant_ids"][0]
        with self.assertRaisesRegex(VerificationError, "deterministic membership"):
            self.aggregate(shards=unexpected)

    def test_write_shard_rejects_missing_or_extra_observed_equal_occurrence(self) -> None:
        expected_raw = self.raw[0::SHARD_COUNT]
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            inventory = root / "inventory.json"
            baseline = root / "baseline.json"
            expected = root / "expected.json"
            observed = root / "observed.json"
            outcomes = root / "outcomes.json"
            evidence = root / "raw-evidence"
            redacted = root / "redacted-evidence"
            output = root / "receipt.json"
            inventory.write_text(json.dumps(self.inventory), encoding="utf-8")
            baseline.write_text(json.dumps(self.baseline), encoding="utf-8")
            expected.write_text(json.dumps(expected_raw), encoding="utf-8")
            evidence.mkdir()
            def terminal_outcomes(summary: str = "CaughtMutant") -> dict:
                return {
                    "cargo_mutants_version": "27.0.0",
                    "end_time": "2026-09-25T00:00:00Z",
                    "total_mutants": len(expected_raw),
                    "outcomes": [
                        {"scenario": {"Mutant": {"name": item["name"]}}, "summary": summary}
                        for item in expected_raw
                    ],
                }
            outcomes.write_text(json.dumps(terminal_outcomes()), encoding="utf-8")

            def write(observed_raw: list[dict]) -> None:
                observed.write_text(json.dumps(observed_raw), encoding="utf-8")
                command_write_shard(
                    Namespace(
                        inventory=inventory,
                        baseline=baseline,
                        expected=expected,
                        observed=observed,
                        outcomes=outcomes,
                        evidence=evidence,
                        redacted_evidence=redacted,
                        sha=self.sha,
                        run_id=self.run_id,
                        run_attempt=1,
                        shard=0,
                        exit_code=0,
                        out=output,
                    )
                )

            write(expected_raw)
            redacted_text = (redacted / "shard-evidence-manifest.json").read_text(encoding="utf-8")
            self.assertNotIn(expected_raw[0]["file"], redacted_text)
            write(expected_raw[1:])
            incomplete = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(incomplete["status"], "incomplete")
            self.assertFalse(incomplete["outcomes_complete"])
            write(expected_raw + [copy.deepcopy(expected_raw[0])])
            incomplete = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(incomplete["status"], "incomplete")

            observed.write_text(json.dumps(expected_raw), encoding="utf-8")
            outcomes.write_text(json.dumps(terminal_outcomes("MissedMutant")), encoding="utf-8")
            command_write_shard(
                Namespace(
                    inventory=inventory, baseline=baseline, expected=expected, observed=observed,
                    outcomes=outcomes, evidence=evidence, redacted_evidence=redacted, sha=self.sha,
                    run_id=self.run_id, run_attempt=1, shard=0, exit_code=1, out=output,
                )
            )
            terminal_failure = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(terminal_failure["status"], "failure")
            self.assertTrue(terminal_failure["outcomes_complete"])
            self.assertEqual(terminal_failure["outcome_counts"]["missed"], len(expected_raw))

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

    def test_rejects_nonterminal_shards_and_retains_complete_failure(self) -> None:
        failure = copy.deepcopy(self.shards)
        failure[4]["status"] = "failure"
        failure[4]["cargo_exit_code"] = 1
        failure[4]["outcome_counts"] = {"caught": 0, "missed": len(failure[4]["mutant_ids"]), "success": 0, "timeout": 0, "unviable": 0}
        self.assertEqual(self.aggregate(shards=failure)["status"], "failure")
        for status, code in (("cancelled", 125), ("timeout", 124), ("in_progress", 0)):
            with self.subTest(status=status):
                shards = copy.deepcopy(self.shards)
                shards[4]["status"] = status
                shards[4]["cargo_exit_code"] = code
                with self.assertRaisesRegex(VerificationError, "terminal status"):
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
                evidence = artifact_root / "redacted-evidence"
                evidence.mkdir(parents=True)
                (evidence / "shard-evidence-manifest.json").write_text(
                    json.dumps(
                        {
                            "schema": EVIDENCE_SCHEMA,
                            "expected_mutant_ids": shard["mutant_ids"],
                            "expected_mutant_counts": shard["mutant_counts"],
                            "observed_mutant_ids": shard["mutant_ids"],
                            "observed_mutant_counts": shard["mutant_counts"],
                            "terminal_outcomes": [
                                {"identity": identity, "outcome": "caught"}
                                for identity in shard["mutant_ids"]
                            ],
                            "outcome_counts": shard["outcome_counts"],
                        }
                    ),
                    encoding="utf-8",
                )
                receipt = copy.deepcopy(shard)
                receipt["evidence_digest"] = directory_digest(evidence)
                receipt["artifact_digest"] = shard_artifact_digest(evidence)
                (artifact_root / "shard-receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
                evidence_paths.append(evidence)

            output = root / "aggregate.json"
            command_aggregate(
                Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, out=output)
            )
            aggregate = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(len(aggregate["shard_artifact_digests"]), SHARD_COUNT)

            # Reproduce run 36127936753 shard 0: its expected and observed
            # redacted identities are complete, but cargo-mutants has no
            # terminal outcomes. Aggregation must retain a deterministic red
            # receipt that binds the evidence instead of raising before write.
            shard_zero = root / f"mutants-shard-0-{self.run_id}-1"
            shard_zero_receipt_path = shard_zero / "shard-receipt.json"
            shard_zero_receipt = json.loads(shard_zero_receipt_path.read_text(encoding="utf-8"))
            shard_zero_receipt.update(
                status="incomplete",
                cargo_exit_code=1,
                outcomes_complete=False,
                outcome_counts=None,
                incomplete_reason="cargo-mutants outcomes are not terminal",
            )
            shard_zero_manifest_path = shard_zero / "redacted-evidence/shard-evidence-manifest.json"
            shard_zero_manifest = json.loads(shard_zero_manifest_path.read_text(encoding="utf-8"))
            shard_zero_manifest.update(
                terminal_outcomes=None,
                outcome_counts=None,
                incomplete_reason="cargo-mutants outcomes are not terminal",
            )
            shard_zero_manifest_path.write_text(json.dumps(shard_zero_manifest), encoding="utf-8")
            shard_zero_receipt["evidence_digest"] = directory_digest(shard_zero / "redacted-evidence")
            shard_zero_receipt["artifact_digest"] = shard_artifact_digest(shard_zero / "redacted-evidence")
            shard_zero_receipt_path.write_text(json.dumps(shard_zero_receipt), encoding="utf-8")
            with self.assertRaisesRegex(VerificationError, "incomplete terminal outcomes"):
                command_aggregate(
                    Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, out=output)
                )
            failed = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(failed["status"], "failure")
            self.assertFalse(failed["outcomes_complete"])
            self.assertEqual(failed["inventory_digest"], self.inventory["inventory_digest"])
            self.assertEqual(failed["baseline_digest"], self.baseline["baseline_digest"])
            self.assertEqual(failed["expected_mutants"], SHARD_COUNT * 2)
            self.assertEqual(len(failed["expected_shard_coverage"]), SHARD_COUNT)
            self.assertEqual(len(failed["observed_shard_coverage"]), SHARD_COUNT)
            self.assertEqual(failed["received_shards"], list(range(SHARD_COUNT)))
            self.assertEqual(failed["missing_shards"], [])
            self.assertEqual(failed["terminal_shards"], SHARD_COUNT - 1)
            self.assertIn("shard 0 has incomplete terminal outcomes", failed["failure_reason"])
            shard_zero_digest = next(item for item in failed["shard_artifact_digests"] if item["index"] == 0)
            self.assertEqual(shard_zero_digest["artifact_digest"], shard_zero_receipt["artifact_digest"])
            first_failure_bytes = output.read_bytes()
            with self.assertRaisesRegex(VerificationError, "incomplete terminal outcomes"):
                command_aggregate(
                    Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, out=output)
                )
            self.assertEqual(output.read_bytes(), first_failure_bytes)

            shutil.rmtree(evidence_paths[8].parent)
            with self.assertRaisesRegex(VerificationError, "missing"):
                command_aggregate(
                    Namespace(artifacts=root, sha=self.sha, run_id=self.run_id, run_attempt=1, out=output)
                )
            incomplete = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(incomplete["status"], "failure")
            self.assertFalse(incomplete["outcomes_complete"])
            self.assertEqual(incomplete["missing_shards"], [8])


if __name__ == "__main__":
    unittest.main()
