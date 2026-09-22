from contextlib import nullcontext
import hashlib
import json
import re
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import scripts.verify_b139_prtarget_data_boundary as guard
from scripts.verify_b139_prtarget_data_boundary import (
    SUPPRESSION_RULE,
    VerificationError,
    approved_suppression_sites,
    verify_workflows,
)


ROOT = Path(__file__).parents[1]


class B139BoundaryTests(unittest.TestCase):
    def _suppression_tree(self, td: str) -> Path:
        """Copy the integrated B139 workflow fixture, including e5 annotations."""
        workflow_dir = Path(td) / ".github" / "workflows"
        workflow_dir.mkdir(parents=True)
        for source in (ROOT / ".github" / "workflows").iterdir():
            if source.is_file():
                shutil.copy2(source, workflow_dir / source.name)
        return workflow_dir.parent.parent

    def _digests(self, root: Path) -> dict[str, str]:
        digests = {}
        for name in guard.KNOWN:
            doc = guard._parse(root / ".github" / "workflows" / name)
            payload = json.dumps(guard._jsonable(doc), sort_keys=True, separators=(",", ":"))
            digests[name] = hashlib.sha256(payload.encode()).hexdigest()
        return digests

    def test_nine_e5_suppressions_are_exactly_allowlisted(self):
        with tempfile.TemporaryDirectory() as td:
            root = self._suppression_tree(td)
            sites = approved_suppression_sites(root)
            self.assertEqual(len(sites), 9)
            for workflow, line_no, _rule in sites:
                lines = (root / workflow).read_text(encoding="utf-8").splitlines()
                self.assertRegex(lines[line_no - 1], r"^\s*-\s+name:")
                checkout = next(
                    line for line in lines[line_no:]
                    if re.match(r"^\s+uses:\s*actions/checkout@[0-9a-f]{40}\b", line)
                )
                self.assertNotIn("nosemgrep:", checkout)

    def test_suppression_bypasses_fail_closed(self):
        mutations = {
            "wildcard rule": lambda text: text.replace(SUPPRESSION_RULE, SUPPRESSION_RULE + "*", 1),
            "empty reason": lambda text: re.sub(r"reason: candidate is data-only; BASE checker runs separately; ADR-0101", "reason:  ADR-0101", text, count=1),
            "wrong ADR": lambda text: text.replace("ADR-0101", "ADR-9999", 1),
            "changed checkout": lambda text: text.replace("actions/checkout@9f698171ed81b15d1823a05fc7211befd50c8ae0", "actions/checkout@" + "0" * 40, 1),
            "changed ref": lambda text: text.replace("pull_request.head.sha", "pull_request.head.ref", 1),
            "changed path": lambda text: text.replace("path: _candidate", "path: _candidate2", 1),
            "changed permissions": lambda text: text.replace("contents: read", "contents: write", 1),
        }
        source = ROOT / ".github" / "workflows" / "backlog-verify.yml"
        for label, mutate in mutations.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory() as td:
                root = self._suppression_tree(td)
                path = root / ".github" / "workflows" / source.name
                path.write_text(mutate(path.read_text()))
                if label in {"wildcard rule", "empty reason", "wrong ADR"}:
                    context = patch.dict(guard.EXPECTED_DOCUMENT_DIGESTS, self._digests(root), clear=False)
                else:
                    context = nullcontext()
                with context:
                    with self.assertRaises(VerificationError):
                        approved_suppression_sites(root)

    def test_suppression_on_trusted_checkout_and_raw_commented_use_fail(self):
        with tempfile.TemporaryDirectory() as td:
            root = self._suppression_tree(td)
            path = root / ".github" / "workflows" / "backlog-verify.yml"
            text = path.read_text()
            trusted = "uses: actions/checkout@9f698171ed81b15d1823a05fc7211befd50c8ae0  # v6.0.3"
            text = text.replace(trusted, trusted + " # nosemgrep: " + SUPPRESSION_RULE + " # reason: fake trusted site; ADR-0101", 1)
            text += "# uses: actions/checkout@" + "9f698171ed81b15d1823a05fc7211befd50c8ae0 # nosemgrep: " + SUPPRESSION_RULE + " # reason: fake; ADR-0101\n"
            path.write_text(text)
            with patch.dict(guard.EXPECTED_DOCUMENT_DIGESTS, self._digests(root), clear=False):
                with self.assertRaises(VerificationError):
                    approved_suppression_sites(root)

    def test_unknown_target_workflow_candidate_checkout_fails(self):
        with tempfile.TemporaryDirectory() as td:
            workflow_dir = Path(td)
            (workflow_dir / "unknown.yml").write_text(
                "name: unknown\non:\n  pull_request_target:\n    types: [opened]\njobs:\n"
                "  check:\n    runs-on: ubuntu-latest\n    steps:\n"
                "      - uses: actions/checkout@9f698171ed81b15d1823a05fc7211befd50c8ae0\n"
                "        with:\n          ref: ${{ github.event.pull_request.head.sha }}\n"
            )
            with self.assertRaises(VerificationError):
                verify_workflows(workflow_dir)

    def test_baseline_passes(self):
        verify_workflows(ROOT / ".github" / "workflows")

    def test_mutations_fail(self):
        mutations = {
            "movable head ref": ("ref: ${{ github.event.pull_request.head.sha", "ref: ${{ github.event.pull_request.head.ref"),
            "missing path": ("path: _base", "# path: _base"),
            "persist credentials": ("persist-credentials: false", "persist-credentials: true"),
            "local action": ("actions/checkout@9f698171ed81b15d1823a05fc7211befd50c8ae0", "./.github/actions/checkout"),
            "candidate workdir": ("working-directory: _base", "working-directory: _candidate"),
            "base ref head": ("pull_request.base.sha", "pull_request.head.sha"),
            "write permissions": ("contents: read", "contents: write"),
            "malformed expression": ("${{ github.event.pull_request.base.sha", "${{ github.event.pull_request.base.sha ||"),
        }
        source = ROOT / ".github" / "workflows" / "backlog-verify.yml"
        for label, (old, new) in mutations.items():
            with self.subTest(label=label):
                with tempfile.TemporaryDirectory() as td:
                    dst = Path(td) / "backlog-verify.yml"
                    text = source.read_text()
                    self.assertIn(old, text)
                    dst.write_text(text.replace(old, new, 1))
                    with self.assertRaises(VerificationError):
                        verify_workflows(Path(td))

    def test_candidate_code_and_new_checkout_fail(self):
        source = (ROOT / ".github" / "workflows" / "backlog-verify.yml").read_text()
        cases = {
            "candidate executable script injected": source.replace("working-directory: _base", "working-directory: _candidate", 1),
            "new candidate checkout": source.replace("path: _base", "path: _candidate2", 1),
            "changed run content": source.replace("scripts/backlog_verify.py", "scripts/evil.py", 1),
        }
        for label, text in cases.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory() as td:
                (Path(td) / "backlog-verify.yml").write_text(text)
                with self.assertRaises(VerificationError):
                    verify_workflows(Path(td))

    def test_anchors_are_rejected(self):
        source = (ROOT / ".github" / "workflows" / "backlog-verify.yml").read_text()
        with tempfile.TemporaryDirectory() as td:
            (Path(td) / "backlog-verify.yml").write_text(source.replace("contents: read", "contents: &mode read\n  pull-requests: *mode", 1))
            with self.assertRaises(VerificationError):
                verify_workflows(Path(td))


if __name__ == "__main__":
    unittest.main()
