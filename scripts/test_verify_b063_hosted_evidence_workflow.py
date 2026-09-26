"""Adversarial cases for the B-063 hosted evidence workflow contract."""

from __future__ import annotations

import copy
import unittest
from pathlib import Path

import yaml

from scripts.verify_b063_hosted_evidence_workflow import verify


WORKFLOW = Path(".github/workflows/issue-1648-b063-read-only-evidence.yml")


def render(document: dict[str, object]) -> str:
    return yaml.safe_dump(document, sort_keys=False)


class B063WorkflowContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.document = yaml.load(WORKFLOW.read_text(encoding="utf-8"), Loader=yaml.BaseLoader)

    def test_current_workflow_passes(self) -> None:
        verify(WORKFLOW.read_text(encoding="utf-8"))

    def test_rejects_user_selectable_production_or_alternate_environment(self) -> None:
        changed = copy.deepcopy(self.document)
        changed["on"]["workflow_dispatch"]["inputs"]["environment"] = {
            "default": "production",
            "options": ["production", "staging"],
        }
        with self.assertRaisesRegex(AssertionError, "must not be user-selectable"):
            verify(render(changed))

    def test_pr_path_filter_includes_its_own_adversarial_contract(self) -> None:
        changed = copy.deepcopy(self.document)
        changed["on"]["pull_request"]["paths"].remove("scripts/test_verify_b063_hosted_evidence_workflow.py")
        with self.assertRaisesRegex(AssertionError, "PR path filter must include"):
            verify(render(changed))

    def test_rejects_missing_or_unreviewed_approval_binding(self) -> None:
        for environment in (None, "production", "production-capacity-read"):
            with self.subTest(environment=environment):
                changed = copy.deepcopy(self.document)
                if environment is None:
                    del changed["jobs"]["live-read-only"]["environment"]
                else:
                    changed["jobs"]["live-read-only"]["environment"] = environment
                with self.assertRaisesRegex(AssertionError, "reviewed staging approval environment"):
                    verify(render(changed))

    def test_rejects_permission_escalation(self) -> None:
        changed = copy.deepcopy(self.document)
        changed["jobs"]["live-read-only"]["permissions"] = {"contents": "write"}
        with self.assertRaisesRegex(AssertionError, "permissions must remain read-only"):
            verify(render(changed))

    def test_rejects_mutating_sql(self) -> None:
        changed = copy.deepcopy(self.document)
        steps = changed["jobs"]["live-read-only"]["steps"]
        capture = next(step for step in steps if step.get("name") == "Capture three independent SELECT-only partition reads")
        run = capture["run"]
        assignment = run.index("sql = (")
        query = run.index("rows = query(sql)", assignment)
        capture["run"] = run[:assignment] + 'sql = "DELETE FROM audit_outbox"\n' + run[query:]
        with self.assertRaisesRegex(AssertionError, "SELECT|mutating SQL"):
            verify(render(changed))

    def test_rejects_network_heredoc_without_auditable_sql_assignment(self) -> None:
        changed = copy.deepcopy(self.document)
        steps = changed["jobs"]["live-read-only"]["steps"]
        capture = next(step for step in steps if step.get("name") == "Capture three independent SELECT-only partition reads")
        run = capture["run"]
        assignment = run.index("sql = (")
        query = run.index("rows = query(sql)", assignment)
        capture["run"] = run[:assignment] + 'rows = query("DELETE FROM audit_outbox")\n' + run[query + len("rows = query(sql)\n"):]
        with self.assertRaisesRegex(AssertionError, "one auditable SQL assignment"):
            verify(render(changed))

    def test_rejects_unreviewed_python_network_client_import(self) -> None:
        changed = copy.deepcopy(self.document)
        steps = changed["jobs"]["live-read-only"]["steps"]
        capture = next(step for step in steps if step.get("name") == "Capture three independent SELECT-only partition reads")
        capture["run"] = capture["run"].replace(
            "          import json\n",
            "          import json\n          import requests\n",
            1,
        )
        with self.assertRaisesRegex(AssertionError, "reviewed read-only allowlist"):
            verify(render(changed))

    def test_rejects_archive_endpoint_instead_of_d1_query_endpoint(self) -> None:
        changed = copy.deepcopy(self.document)
        steps = changed["jobs"]["live-read-only"]["steps"]
        capture = next(step for step in steps if step.get("name") == "Capture three independent SELECT-only partition reads")
        capture["run"] = capture["run"].replace(
            "https://api.cloudflare.com/client/v4/accounts/{account}/d1/database/{database}/query",
            "https://api.cloudflare.com/client/v4/accounts/{account}/workers/scripts/corelink-prod",
        )
        with self.assertRaisesRegex(AssertionError, "reviewed Cloudflare D1 query endpoint"):
            verify(render(changed))

    def test_rejects_archive_route_literal(self) -> None:
        changed = copy.deepcopy(self.document)
        steps = changed["jobs"]["live-read-only"]["steps"]
        capture = next(step for step in steps if step.get("name") == "Capture three independent SELECT-only partition reads")
        capture["run"] += "\n/_internal/audit/archive"
        with self.assertRaisesRegex(AssertionError, "must not contain PagerDuty mutation"):
            verify(render(changed))

    def test_rejects_dispatch_from_unprotected_or_drifted_ref(self) -> None:
        for missing_gate in ("github.ref == 'refs/heads/main'", "github.ref_protected == true"):
            with self.subTest(missing_gate=missing_gate):
                changed = copy.deepcopy(self.document)
                changed["jobs"]["live-read-only"]["if"] = changed["jobs"]["live-read-only"]["if"].replace(
                    missing_gate, ""
                )
                with self.assertRaisesRegex(AssertionError, missing_gate):
                    verify(render(changed))


if __name__ == "__main__":
    unittest.main()
