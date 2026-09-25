import unittest

from scripts.verify_i1863_mutants_hosted import validate_successful_receipt
from scripts.verify_i2457_mutants_shards import AGGREGATE_SCHEMA, SHARD_COUNT, config_digest, digest


class HostedReceiptValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.sha = "a" * 40
        self.receipt = {
            "schema": AGGREGATE_SCHEMA,
            "run_id": "1234",
            "run_attempt": 2,
            "sha": self.sha,
            "tool_version": "27.0.0",
            "config_digest": config_digest(),
            "inventory_digest": digest(["inventory"]),
            "baseline_digest": digest(["baseline"]),
            "shard_count": SHARD_COUNT,
            "covered_mutants": 27,
            "inventory_mutant_counts": [{"identity": "sha256:" + "1" * 64, "count": 27}],
            "covered_mutant_counts": [{"identity": "sha256:" + "1" * 64, "count": 27}],
            "per_shard_occurrence_counts": [
                {
                    "index": index,
                    "occurrences": 1,
                    "multiset_digest": digest(["shard-count", index]),
                }
                for index in range(SHARD_COUNT)
            ],
            "shard_artifact_digests": [
                {
                    "index": index,
                    "run_attempt": 1 if index else 2,
                    "evidence_digest": digest(["evidence", index]),
                    "artifact_digest": digest(["artifact", index]),
                }
                for index in range(SHARD_COUNT)
            ],
            "attempt_lineage": [1, 2],
            "status": "success",
        }
        self.receipt["coverage_digest"] = digest(
            {
                "mutant_counts": self.receipt["covered_mutant_counts"],
                "per_shard_occurrence_counts": self.receipt["per_shard_occurrence_counts"],
            }
        )
        self.run = {
            "databaseId": 1234,
            "attempt": 2,
            "headSha": self.sha,
            "headBranch": "main",
            "status": "completed",
            "conclusion": "success",
        }

    def test_accepts_successful_exact_sha_aggregate(self) -> None:
        validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_failed_or_in_progress_run(self) -> None:
        self.run["conclusion"] = "failure"
        with self.assertRaisesRegex(ValueError, "complete successfully"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_wrong_run_or_attempt(self) -> None:
        self.receipt["run_id"] = "9999"
        with self.assertRaisesRegex(ValueError, "run_id"):
            validate_successful_receipt(self.receipt, self.run, self.sha)
        self.receipt["run_id"] = "1234"
        self.receipt["run_attempt"] = 1
        with self.assertRaisesRegex(ValueError, "run_attempt"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_mixed_sha_or_non_main(self) -> None:
        self.receipt["sha"] = "b" * 40
        with self.assertRaisesRegex(ValueError, "SHA"):
            validate_successful_receipt(self.receipt, self.run, self.sha)
        self.receipt["sha"] = self.sha
        self.run["headBranch"] = "feature"
        with self.assertRaisesRegex(ValueError, "protected main"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_unpinned_tool_or_configuration(self) -> None:
        self.receipt["tool_version"] = "27.0.1"
        with self.assertRaisesRegex(ValueError, "tool version"):
            validate_successful_receipt(self.receipt, self.run, self.sha)
        self.receipt["tool_version"] = "27.0.0"
        self.receipt["config_digest"] = digest(["drift"])
        with self.assertRaisesRegex(ValueError, "configuration digest"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_incomplete_or_bad_lineage(self) -> None:
        self.receipt["covered_mutants"] = 0
        with self.assertRaisesRegex(ValueError, "nonempty"):
            validate_successful_receipt(self.receipt, self.run, self.sha)
        self.receipt["covered_mutants"] = 27
        self.receipt["attempt_lineage"] = [3]
        with self.assertRaisesRegex(ValueError, "lineage"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_missing_or_duplicate_per_shard_artifact_digests(self) -> None:
        self.receipt["shard_artifact_digests"] = self.receipt["shard_artifact_digests"][:-1]
        with self.assertRaisesRegex(ValueError, "per-shard artifact digests"):
            validate_successful_receipt(self.receipt, self.run, self.sha)
        self.receipt["shard_artifact_digests"].append(self.receipt["shard_artifact_digests"][0])
        with self.assertRaisesRegex(ValueError, "per-shard artifact"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_missing_or_mismatched_occurrence_coverage(self) -> None:
        self.receipt["covered_mutant_counts"] = [{"identity": "sha256:" + "1" * 64, "count": 26}]
        with self.assertRaisesRegex(ValueError, "multiplicity"):
            validate_successful_receipt(self.receipt, self.run, self.sha)
        self.receipt["covered_mutant_counts"] = [{"identity": "sha256:" + "1" * 64, "count": 27}]
        self.receipt["per_shard_occurrence_counts"] = self.receipt["per_shard_occurrence_counts"][:-1]
        with self.assertRaisesRegex(ValueError, "per-shard occurrence"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_boolean_per_shard_occurrence_index(self) -> None:
        self.receipt["per_shard_occurrence_counts"][0]["index"] = False
        self.receipt["coverage_digest"] = digest(
            {
                "mutant_counts": self.receipt["covered_mutant_counts"],
                "per_shard_occurrence_counts": self.receipt["per_shard_occurrence_counts"],
            }
        )
        with self.assertRaisesRegex(ValueError, "per-shard occurrence identity"):
            validate_successful_receipt(self.receipt, self.run, self.sha)


if __name__ == "__main__":
    unittest.main()
