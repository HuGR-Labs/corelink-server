import unittest

from scripts.verify_i1863_mutants_hosted import validate_successful_receipt


class HostedReceiptValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.sha = "a" * 40
        self.receipt = {
            "schema": "corelink.hosted-mutants-receipt.v1",
            "run_id": "1234",
            "run_attempt": "1",
            "sha": self.sha,
            "ref": "refs/heads/main",
            "status": "success",
        }
        self.run = {
            "databaseId": 1234,
            "attempt": 1,
            "headSha": self.sha,
            "headBranch": "main",
            "status": "completed",
            "conclusion": "success",
        }

    def test_accepts_successful_exact_sha_receipt(self) -> None:
        validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_in_progress_run(self) -> None:
        self.run["status"] = "in_progress"
        with self.assertRaisesRegex(ValueError, "not completed"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_failed_run_and_failed_receipt(self) -> None:
        self.run["conclusion"] = "failure"
        with self.assertRaisesRegex(ValueError, "conclusion is not success"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_failed_receipt_even_if_run_metadata_says_success(self) -> None:
        self.receipt["status"] = "failure"
        with self.assertRaisesRegex(ValueError, "receipt status is not success"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_receipt_for_another_run(self) -> None:
        self.receipt["run_id"] = "9999"
        with self.assertRaisesRegex(ValueError, "run_id does not match"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_missing_run_attempt(self) -> None:
        self.run.pop("attempt")
        with self.assertRaisesRegex(ValueError, "run_attempt does not match"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_mismatched_run_attempt(self) -> None:
        self.receipt["run_attempt"] = "2"
        with self.assertRaisesRegex(ValueError, "run_attempt does not match"):
            validate_successful_receipt(self.receipt, self.run, self.sha)

    def test_rejects_non_exact_sha(self) -> None:
        with self.assertRaisesRegex(ValueError, "expected exact SHA"):
            validate_successful_receipt(self.receipt, self.run, "b" * 40)

    def test_rejects_non_main_run(self) -> None:
        self.run["headBranch"] = "feature"
        with self.assertRaisesRegex(ValueError, "not from protected main"):
            validate_successful_receipt(self.receipt, self.run, self.sha)


if __name__ == "__main__":
    unittest.main()
