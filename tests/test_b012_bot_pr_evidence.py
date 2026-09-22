from __future__ import annotations

import copy
import json
import unittest
from scripts.verify_b012_bot_pr_evidence import DEFAULT_RECORD, EvidenceError, validate_record


class B012BotPrEvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.record = json.loads(DEFAULT_RECORD.read_text(encoding="utf-8"))

    def test_verified_receipt_is_accepted(self) -> None:
        validate_record(copy.deepcopy(self.record))

    def test_credentials_and_installation_id_cannot_enter_receipt(self) -> None:
        for key in ("installation_id", "app_installation_id", "private_key", "jwt", "access_token"):
            with self.subTest(key=key):
                mutated = copy.deepcopy(self.record)
                mutated[key] = "must-not-be-recorded"
                with self.assertRaises(EvidenceError):
                    validate_record(mutated)

    def test_secret_name_state_mutation_is_rejected(self) -> None:
        mutated = copy.deepcopy(self.record)
        mutated["actions_secret_names"]["BOT_PR_TOKEN"] = "present"
        with self.assertRaises(EvidenceError):
            validate_record(mutated)

    def test_additional_selected_repository_is_rejected(self) -> None:
        mutated = copy.deepcopy(self.record)
        mutated["installation"]["repositories"].append("HuGR-dev/another-repo")
        with self.assertRaises(EvidenceError):
            validate_record(mutated)

    def test_failed_or_nonhosted_job_is_rejected(self) -> None:
        for field, value in (("conclusion", "failure"), ("runner_labels", ["self-hosted"])):
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


if __name__ == "__main__":
    unittest.main()
