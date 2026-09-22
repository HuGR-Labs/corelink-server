"""Static safety contract for the credentialless #2045 hosted check."""

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/issue-2045-staging-renderer.yml"


class Issue2045WorkflowContractTests(unittest.TestCase):
    def test_check_is_hosted_focused_and_credentialless(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        required = (
            "pull_request:",
            "workflow_dispatch:",
            "runs-on: ubuntu-24.04",
            "permissions:\n  contents: read",
            "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
            "persist-credentials: false",
            "python3 -S -m unittest -v",
            "tests/test_render_staging_wrangler.py",
            "tests/test_issue_2045_renderer_workflow.py",
        )
        for fragment in required:
            with self.subTest(fragment=fragment):
                self.assertIn(fragment, source)

    def test_check_has_no_mutation_inputs(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8").lower()
        for forbidden in (
            "secrets.",
            "environment:",
            "wrangler",
            "cloudflare",
            "deploy",
            "write-token",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, source)


if __name__ == "__main__":
    unittest.main()
