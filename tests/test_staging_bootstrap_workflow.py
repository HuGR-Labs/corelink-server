"""Static contract for the credentialless #1700 hosted validation job."""
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/staging-bootstrap.yml"


class StagingBootstrapWorkflowTests(unittest.TestCase):
    def test_pr_validation_is_credentialless_and_apply_stays_manual(self):
        source = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("pull_request:", source)
        self.assertIn("name: staging bootstrap static validation", source)
        self.assertIn("github.event_name == 'pull_request'", source)
        self.assertIn("python3 -S -m unittest -v", source)
        self.assertIn("tests/test_render_staging_wrangler.py", source)
        self.assertIn("tests/test_plan_staging_provider.py", source)
        self.assertIn("tests/test_staging_bootstrap_workflow.py", source)
        self.assertIn("workflow_dispatch:", source)
        self.assertIn("github.event_name == 'workflow_dispatch'", source)
        self.assertIn("environment: staging", source)
        self.assertIn("github.ref == 'refs/heads/main'", source)
        self.assertIn("RUNNER_TEMP", source)


if __name__ == "__main__":
    unittest.main()
