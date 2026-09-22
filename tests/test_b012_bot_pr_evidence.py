from __future__ import annotations

import copy
import json
import unittest

from scripts.verify_b012_bot_pr_evidence import DEFAULT_RECORD, EvidenceError, validate_record


class B012BotPrEvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.record = json.loads(DEFAULT_RECORD.read_text(encoding="utf-8"))

    def test_verified_receipt_has_both_hosted_job_urls(self) -> None:
        validate_record(copy.deepcopy(self.record))
        runs = {run["workflow"]: run for run in self.record["workflow_runs"]}
        self.assertEqual(set(runs), {"dco-check", "rustfmt"})
        for run in runs.values():
            self.assertEqual(run["job_count"], len(run["job_urls"]))
            self.assertEqual(run["runner_group"], "GitHub Actions")
            self.assertEqual(run["runner_labels"], ["ubuntu-latest"])

    def test_legacy_single_job_url_field_is_rejected(self) -> None:
        mutated = copy.deepcopy(self.record)
        run = mutated["workflow_runs"][0]
        run["job_url"] = run.pop("job_urls")[0]
        with self.assertRaises(EvidenceError):
            validate_record(mutated)

    def test_incomplete_or_unhosted_run_is_rejected(self) -> None:
        for field, value in (
            ("job_urls", []),
            ("conclusion", "failure"),
            ("runner_labels", ["self-hosted", "corelink"]),
            ("approval_state", "approval_required"),
        ):
            with self.subTest(field=field):
                mutated = copy.deepcopy(self.record)
                mutated["workflow_runs"][0][field] = value
                with self.assertRaises(EvidenceError):
                    validate_record(mutated)

    def test_missing_required_workflow_is_rejected(self) -> None:
        mutated = copy.deepcopy(self.record)
        mutated["workflow_runs"].pop()
        with self.assertRaises(EvidenceError):
            validate_record(mutated)

    def test_credentials_and_installation_identifiers_are_forbidden(self) -> None:
        for key in ("installation_id", "private_key", "jwt", "access_token"):
            with self.subTest(key=key):
                mutated = copy.deepcopy(self.record)
                mutated[key] = "must-not-be-recorded"
                with self.assertRaises(EvidenceError):
                    validate_record(mutated)


if __name__ == "__main__":
    unittest.main()
