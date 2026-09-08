from __future__ import annotations

import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b155_batch_d.py"
spec = importlib.util.spec_from_file_location("b155_batch_d", SCRIPT)
assert spec and spec.loader
batch = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = batch
spec.loader.exec_module(batch)


def write(root: Path, relative: str, content: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def copy_sources(root: Path, relatives: tuple[str, ...]) -> None:
    for relative in relatives:
        destination = root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / relative, destination)


class B155BatchDTests(unittest.TestCase):
    def test_baseline_helpers_pass_without_dependency_install(self) -> None:
        for record_id in ("B-136", "B-141", "B-159", "B-164"):
            with self.subTest(record_id=record_id):
                batch.CHECKS[record_id](ROOT)

    def test_lexer_removes_comment_string_and_dead_bait(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write(
                root,
                "probe.rs",
                '// Method::GET\n'
                'fn live() { let bait = "Method::PUT"; '
                'if false { Method::HEAD; } Method::DELETE; }\n',
            )
            active = batch._active(root, "probe.rs", "rust")
            self.assertNotIn("Method::GET", active)
            self.assertIn('"Method::PUT"', active)
            self.assertNotIn("Method::HEAD", active)
            self.assertIn("Method::DELETE", active)

            write(root, "probe.md", "<!-- DELETE internal-only -->\nGET\n")
            active_markdown = batch._active(root, "probe.md", "markdown")
            self.assertNotIn("internal-only", active_markdown)
            self.assertIn("GET", active_markdown)

    def test_b136_comment_string_dead_absent_and_ambiguous_mutations_fail(self) -> None:
        good = (
            "concurrency:\n"
            "  group: backlog-${{ github.event_name }}-${{ github.ref }}\n"
            "  cancel-in-progress: true\n"
        )
        mutations = (
            "# group: backlog-${{ github.event_name }}-${{ github.ref }}\n"
            "  cancel-in-progress: true\n",
            '  name: "group: backlog-${{ github.event_name }}-${{ github.ref }}"\n'
            "  cancel-in-progress: true\n",
            "  if(false){group: backlog-${{ github.event_name }}-${{ github.ref }}}\n"
            "  cancel-in-progress: true\n",
            "  cancel-in-progress: true\n",
            "  group: one-${{ github.event_name }}-${{ github.ref }}\n"
            "  group: two-${{ github.event_name }}-${{ github.ref }}\n"
            "  cancel-in-progress: true\n",
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                with tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    write(root, ".github/workflows/backlog-verify.yml", "concurrency:\n" + mutation)
                    with self.assertRaises(batch.CheckError):
                        batch.check_b136(root)

        # Moving the apparently valid group out of top-level `concurrency`
        # must not satisfy the gate with a job-level lookalike.
        relocated = (
            "concurrency:\n"
            "  cancel-in-progress: true\n"
            "jobs:\n"
            "  fake:\n"
            "    group: backlog-${{ github.event_name }}-${{ github.ref }}\n"
        )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write(root, ".github/workflows/backlog-verify.yml", relocated)
            with self.assertRaises(batch.CheckError):
                batch.check_b136(root)

    def test_b159_string_dead_absent_and_ambiguous_mutations_fail(self) -> None:
        relatives = (
            "apps/docs/docs/integrations/sccache-cargo.md",
            "apps/docs/i18n/de/docusaurus-plugin-content-docs/current/integrations/sccache-cargo.md",
            "apps/docs/i18n/es-419/docusaurus-plugin-content-docs/current/integrations/sccache-cargo.md",
            "apps/docs/i18n/pt-BR/docusaurus-plugin-content-docs/current/integrations/sccache-cargo.md",
            "crates/corelink-container/src/routes/cargo/part-00.rs",
            "crates/corelink-container/src/routes/cargo/tests-00-00.rs",
        )
        mutations = (
            (relatives[0], "| `GET` |", "<!-- | `GET` | -->"),
            (relatives[4], "async fn handle_delete", 'let _bait = "async fn handle_delete";'),
            (relatives[4], "async fn handle_delete", "if(false){ async fn handle_delete() {} }"),
            (relatives[4], 'req.method().as_str() == "MKCOL"', ""),
            (relatives[0], "| `DELETE` |", "| `DELETE` |\n| `DELETE` |"),
        )
        for relative, old, new in mutations:
            with self.subTest(relative=relative, new=new):
                with tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    copy_sources(root, relatives)
                    path = root / relative
                    source = path.read_text(encoding="utf-8")
                    self.assertIn(old, source)
                    path.write_text(source.replace(old, new, 1), encoding="utf-8")
                    with self.assertRaises(batch.CheckError):
                        batch.check_b159(root)

    def test_b164_comment_string_dead_absent_and_ambiguous_mutations_fail(self) -> None:
        relatives = (
            "tools/cli/src/doctor.rs",
            "tools/cli/src/client.rs",
            "apps/docs/docs/integrations/npm.md",
            "apps/docs/i18n/es-419/docusaurus-plugin-content-docs/current/integrations/npm.md",
            "apps/docs/i18n/pt-BR/docusaurus-plugin-content-docs/current/integrations/npm.md",
            "apps/docs/i18n/de/docusaurus-plugin-content-docs/current/integrations/npm.md",
        )
        mutations = (
            (relatives[0], 'CliError::HttpStatus { status: 401 } =>', '// CliError::HttpStatus { status: 401 } =>'),
            (relatives[0], "async fn check_quota_unauthorized_is_auth_failure", 'let _bait = "async fn check_quota_unauthorized_is_auth_failure";'),
            (relatives[0], "async fn check_quota_unauthorized_is_auth_failure", "if(false){ async fn check_quota_unauthorized_is_auth_failure() {} }"),
            (relatives[2], "npm config get registry --location=user", ""),
            (relatives[2], "npm config get registry --location=project", "npm config get registry --location=project npm config get registry --location=project"),
        )
        for relative, old, new in mutations:
            with self.subTest(relative=relative, new=new):
                with tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    copy_sources(root, relatives)
                    path = root / relative
                    source = path.read_text(encoding="utf-8")
                    self.assertIn(old, source)
                    path.write_text(source.replace(old, new, 1), encoding="utf-8")
                    with self.assertRaises(batch.CheckError):
                        batch.check_b164(root)

    def test_require_is_fail_closed_for_absent_and_ambiguous_markers(self) -> None:
        with self.assertRaises(batch.CheckError):
            batch._require("comment only", r"active", "absent")
        with self.assertRaises(batch.CheckError):
            batch._require("active\nactive", r"active", "ambiguous", count=1)


if __name__ == "__main__":
    unittest.main()
