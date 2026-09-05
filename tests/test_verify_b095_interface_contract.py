from __future__ import annotations

import importlib.util
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b095_interface_contract.py"
spec = importlib.util.spec_from_file_location("b095_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


class B095InterfaceContractTests(unittest.TestCase):
    def test_workflow_triggers_cover_every_verifier_input(self) -> None:
        workflow = (ROOT / ".github/workflows/backlog-verify.yml").read_text(
            encoding="utf-8"
        )
        expected = {
            str(path) for path in (*verifier.FILES.values(), *verifier.QUICKSTARTS)
        }
        for event in ("pull_request", "push"):
            match = re.search(
                rf"(?ms)^  {event}:\n(.*?)(?=^  (?:push|schedule|workflow_dispatch):|\Z)",
                workflow,
            )
            self.assertIsNotNone(match, f"workflow must define {event}")
            paths = set(
                re.findall(r'^\s+- "([^"]+)"$', match.group(1), re.MULTILINE)
            )
            self.assertEqual(
                expected - paths,
                set(),
                f"{event}.paths must include every B-095 verifier input",
            )

    def copy_fixture(self) -> tempfile.TemporaryDirectory[str]:
        temp = tempfile.TemporaryDirectory()
        root = Path(temp.name)
        for relative in (*verifier.FILES.values(), *verifier.QUICKSTARTS):
            source = ROOT / relative
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        return temp

    def run_cli(self, root: Path, expect: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(root), "--expect", expect],
            text=True,
            capture_output=True,
            check=False,
        )

    def test_repaired_tree_is_done(self) -> None:
        self.assertEqual(verifier.assess(ROOT), [])
        result = self.run_cli(ROOT, "done")
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)

    def test_historical_installer_prose_does_not_reopen_contract(self) -> None:
        temp = self.copy_fixture()
        with temp:
            path = Path(temp.name) / verifier.FILES["installer"]
            path.write_text(
                path.read_text(encoding="utf-8")
                + "\n// Historical note: --region was once accepted as a no-op.\n",
                encoding="utf-8",
            )
            self.assertEqual(verifier.assess(Path(temp.name)), [])

    def test_each_contract_mutation_reopens_by_named_reason(self) -> None:
        mutations = {
            "installer-region-parser": (
                verifier.FILES["installer"],
                "  case \"$1\" in\n",
                "  --region=*) REGION=\"${1#--region=}\" ;;\n  case \"$1\" in\n",
            ),
            "team-ui-developer-role": (
                verifier.FILES["team-ui"],
                'export const INVITABLE_ROLES: InviteRole[] = ["admin", "member", "viewer"];',
                'const ROLES = ["Owner", "Admin", "Developer", "Viewer"];',
            ),
            "workspace-pin-toggle": (
                verifier.FILES["workspaces"],
                "SET pinned = ?3",
                "SET pinned = 1 - pinned",
            ),
            "quickstart-region-id:apps/docs/docs/tutorials/quickstart-10min.mdx": (
                verifier.QUICKSTARTS[0],
                "## Before you start",
                "Available regions: `ord`\n\n## Before you start",
            ),
            "role-catalog-owner-transition-matrix": (
                verifier.FILES["role-catalog"],
                "| `Owner`            | ❌",
                "| `Owner`            | Grant / revoke",
            ),
        }
        for reason, (relative, old, new) in mutations.items():
            with self.subTest(reason=reason):
                temp = self.copy_fixture()
                with temp:
                    path = Path(temp.name) / relative
                    content = path.read_text(encoding="utf-8")
                    self.assertIn(old, content)
                    path.write_text(content.replace(old, new, 1), encoding="utf-8")
                    gaps = verifier.assess(Path(temp.name))
                    self.assertIn(reason, gaps)

    def test_missing_contract_is_an_instrument_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            result = self.run_cli(Path(directory), "done")
        self.assertEqual(result.returncode, 2)
        self.assertIn("instrument error", result.stderr)


if __name__ == "__main__":
    unittest.main()
