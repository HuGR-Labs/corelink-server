"""Static safety contract for the credentialless #2045 hosted check."""

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/issue-2045-staging-renderer.yml"
EXPECTED_RUN = (
    "python3 -S -m unittest -v tests/test_render_staging_wrangler.py "
    "tests/test_issue_2045_renderer_workflow.py"
)
CHECKOUT = "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0"


def _run_commands(source: str) -> list[str]:
    """Read YAML `run` blocks and normalize folded or literal shell text."""
    lines = source.splitlines()
    commands: list[str] = []
    for index, line in enumerate(lines):
        match = re.match(r"^( *)run:\s*(.*)$", line)
        if not match:
            continue
        indentation = len(match.group(1))
        inline = match.group(2).strip()
        parts = [] if inline in {"", ">", ">-", "|", "|-"} else [inline]
        for following in lines[index + 1 :]:
            if following.strip() and len(following) - len(following.lstrip()) <= indentation:
                break
            parts.append(following.strip())
        commands.append(" ".join(" ".join(parts).split()))
    return commands


def _safe_workflow(source: str) -> bool:
    """Check the workflow's executable structure, ignoring filenames and prose."""
    if "pull_request:" not in source or "workflow_dispatch:" not in source:
        return False

    permissions = re.search(
        r"(?ms)^permissions:\n((?:^  [^\n]*\n|^\n)*)", source
    )
    if permissions is None:
        return False
    permission_lines = [
        line.strip()
        for line in permissions.group(1).splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if permission_lines != ["contents: read"]:
        return False

    job_names = re.findall(r"(?m)^  ([a-zA-Z0-9_-]+):\s*$", source.split("jobs:", 1)[-1])
    if job_names != ["focused"]:
        return False
    uses = re.findall(r"(?m)^\s*-\s*uses:\s*([^\s]+)", source)
    if uses != [CHECKOUT]:
        return False
    if "persist-credentials: false" not in source:
        return False
    if re.search(r"(?m)^\s{6}environment\s*:", source):
        return False
    if re.search(r"\$\{\{\s*secrets\.", source, re.IGNORECASE):
        return False
    if _run_commands(source) != [EXPECTED_RUN]:
        return False
    return True


class Issue2045WorkflowContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.source = WORKFLOW.read_text(encoding="utf-8")

    def test_check_is_hosted_focused_and_credentialless(self) -> None:
        self.assertTrue(_safe_workflow(self.source))
        self.assertIn('runs-on: ubuntu-24.04', self.source)
        self.assertIn('"scripts/render_staging_wrangler.py"', self.source)

    def test_semantic_mutations_of_permissions_and_execution_are_rejected(self) -> None:
        mutations = (
            self.source.replace("  contents: read", "  contents: write", 1),
            self.source.replace(
                "          tests/test_issue_2045_renderer_workflow.py\n",
                "          tests/test_issue_2045_renderer_workflow.py\n          wrangler deploy\n",
                1,
            ),
            self.source.replace(
                "      - name: Run focused renderer and static workflow contract\n",
                "      - uses: cloudflare/wrangler-action@v3\n"
                "      - name: Run focused renderer and static workflow contract\n",
                1,
            ),
            self.source.replace(
                "  focused:\n",
                "  focused:\n    environment: staging\n",
                1,
            ),
            self.source.replace(
                "    runs-on: ubuntu-24.04\n",
                "    runs-on: ubuntu-24.04\n"
                "    env:\n      CF_API_TOKEN: ${{ secrets.CF_API_TOKEN }}\n",
                1,
            ),
        )
        for mutated in mutations:
            with self.subTest(workflow_mutation=mutated):
                self.assertFalse(_safe_workflow(mutated))


if __name__ == "__main__":
    unittest.main()
