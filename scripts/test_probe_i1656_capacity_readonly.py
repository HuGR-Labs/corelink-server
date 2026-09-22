import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROBE = (ROOT / "scripts/probe_i1656_capacity_readonly.py").read_text(encoding="utf-8")
WORKFLOW = (ROOT / ".github/workflows/issue-1656-capacity-refresh.yml").read_text(encoding="utf-8")


class Issue1656CapacityRefreshContractTests(unittest.TestCase):
    def test_probe_has_fixed_get_only_endpoint_and_dedicated_token(self) -> None:
        self.assertIn('method="GET"', PROBE)
        self.assertIn("CANONICAL_ACCOUNT_ID", PROBE)
        self.assertIn("CLOUDFLARE_CAPACITY_READ_TOKEN", PROBE)
        self.assertNotIn("--account", PROBE)
        self.assertNotIn("--url", PROBE)
        self.assertNotIn("data=", PROBE)


    def test_workflow_is_protected_manual_main_only_and_redacted(self) -> None:
        self.assertIn("workflow_dispatch:", WORKFLOW)
        self.assertIn("run-1656-read-only", WORKFLOW)
        self.assertIn("production-capacity-read", WORKFLOW)
        self.assertIn("refs/heads/main", WORKFLOW)
        self.assertIn("CLOUDFLARE_CAPACITY_READ_TOKEN", WORKFLOW)
        self.assertIn("persist-credentials: false", WORKFLOW)
        self.assertNotIn("PATCH", WORKFLOW)
        self.assertNotIn("POST", WORKFLOW)
        self.assertNotIn("DELETE", WORKFLOW)
