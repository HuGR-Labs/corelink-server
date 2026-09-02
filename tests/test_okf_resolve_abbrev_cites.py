"""Contract tests for the abbreviated-citation resolver (B-059)."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import types
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "scripts" / "okf_resolve_abbrev_cites.py"
VALIDATOR = REPO_ROOT / "scripts" / "validate_okf.py"
WORKFLOW = REPO_ROOT / ".github" / "workflows" / "okf_wiki.yml"
SPEC = importlib.util.spec_from_file_location("okf_resolve_abbrev_cites", SCRIPT)
assert SPEC and SPEC.loader
RESOLVER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = RESOLVER
SPEC.loader.exec_module(RESOLVER)


def _scan(tmp_path: Path, concept_body: str, source_files: dict[str, str]):
    RESOLVER.REPO_ROOT = tmp_path
    concept = tmp_path / "docs" / "knowledge" / "concept.md"
    concept.parent.mkdir(parents=True)
    concept.write_text(concept_body, encoding="utf-8")
    for rel, body in source_files.items():
        source = tmp_path / rel
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text(body, encoding="utf-8")

    full_re, bare_re = RESOLVER._compile()
    return RESOLVER.scan_concept(
        concept,
        RESOLVER.SourceCache(tmp_path),
        full_re,
        bare_re,
    )


class AbbreviatedCitationResolverTest(unittest.TestCase):
    """Portable contract tests also executed directly by okf_wiki.yml."""

    def scan(self, concept_body: str, source_files: dict[str, str]):
        with tempfile.TemporaryDirectory() as tmp:
            return _scan(Path(tmp), concept_body, source_files)

    def test_inherits_across_lines_and_accepts_non_code_extensions(self) -> None:
        seen, findings = self.scan(
            "# Citations\n1. `migrations/d1/0074_team_member.sql:1`\n2. `:2`\n",
            {"migrations/d1/0074_team_member.sql": "CREATE TABLE x;\nCREATE INDEX x_i;\n"},
        )

        self.assertEqual(seen, 1)
        self.assertEqual(findings, [])

    def test_repo_relative_path_cannot_escape_source_root(self) -> None:
        seen, findings = self.scan(
            "# Citations\n1. `../outside.rs:1`\n2. `:1`\n",
            {"outside.rs": "fn outside() {}\n"},
        )

        self.assertEqual(seen, 1)
        self.assertEqual(len(findings), 2)
        self.assertIn("full-file-missing", findings[0])
        self.assertIn("file-missing", findings[1])

    def test_rejects_zero_and_past_eof_ranges_and_malformed_bare_range(self) -> None:
        seen, findings = self.scan(
            "# Citations\n"
            "1. `src/live.rs:0`\n"
            "2. `src/live.rs:1-4`\n"
            "3. `src/live.rs:1` then `:999-`\n",
            {"src/live.rs": "fn live() {}\n"},
        )

        self.assertEqual(seen, 1)
        self.assertTrue(any("full-line-before-start" in finding for finding in findings))
        self.assertTrue(any("full-past-eof" in finding for finding in findings))
        self.assertTrue(any("malformed-range" in finding for finding in findings))

    def test_path_only_anchor_resolves_nested_and_root_source_files(self) -> None:
        seen, findings = self.scan(
            "---\nsource_files:\n"
            "  - crates/example/src/customer_d1.rs\n"
            "  - README.md\n"
            "  - .github/workflows/foo.yml\n"
            "  - .gitignore\n---\n"
            "# Citations\n"
            "Nested source `crates/example/src/customer_d1.rs`; see `:1` and `:2`.\n"
            "Root source `README.md`; see `:1` and `:2`.\n"
            "Dot path `.github/workflows/foo.yml`; see `:1`.\n"
            "Dotfile `.gitignore`; see `:1`.\n",
            {
                "crates/example/src/customer_d1.rs": "impl Customer {\nfn method() {}\n}\n",
                "README.md": "CoreLink production service.\nOperational notes.\n",
                ".github/workflows/foo.yml": "name: test\n",
                ".gitignore": "target/\n",
            },
        )

        self.assertEqual(seen, 6)
        self.assertEqual(findings, [])

    def test_path_only_traversal_is_rejected_and_plain_identifiers_do_not_inherit(self) -> None:
        seen, findings = self.scan(
            "---\nsource_files:\n  - ../outside.rs\n---\n"
            "# Citations\n"
            "An inline identifier `customer` must not retarget `:1`.\n"
            "Outside source `../outside.rs`; see `:1`.\n",
            {"outside.rs": "fn outside() {}\n", "customer": "fn decoy() {}\n"},
        )

        self.assertEqual(seen, 2)
        self.assertIn("no-inherited-path", findings[0])
        self.assertIn("no-inherited-path", findings[1])

    def test_dotted_identifiers_never_inherit_without_declared_source_context(self) -> None:
        seen, findings = self.scan(
            "---\nsource_files:\n  - README.md\n---\n"
            "`customer.subscription.deleted` then `:1`.\n"
            "`tenant.tier` then `:1`.\n"
            "`invoice.payment_failed` then `:1`.\n",
            {
                "README.md": "Grounded source.\n",
                "customer.subscription.deleted": "decoy event\n",
                "tenant.tier": "decoy field\n",
                "invoice.payment_failed": "decoy event\n",
            },
        )

        self.assertEqual(seen, 3)
        self.assertEqual(len(findings), 3)
        self.assertTrue(all("no-inherited-path" in finding for finding in findings))

    def test_path_only_anchor_guard_requires_exact_declared_readable_source(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "README.md").write_text("grounded\n", encoding="utf-8")
            (root / "customer.subscription.deleted").write_text("decoy\n", encoding="utf-8")
            cache = RESOLVER.SourceCache(root)

            self.assertEqual(
                RESOLVER._path_only_source_anchor("`README.md`", {"README.md"}, cache),
                "README.md",
            )
            (root / ".gitignore").write_text("target/\n", encoding="utf-8")
            (root / ".github" / "workflows").mkdir(parents=True)
            (root / ".github" / "workflows" / "foo.yml").write_text("name: test\n", encoding="utf-8")
            self.assertEqual(
                RESOLVER._path_only_source_anchor("`.gitignore`", {".gitignore"}, cache),
                ".gitignore",
            )
            self.assertEqual(
                RESOLVER._path_only_source_anchor(
                    "`.github/workflows/foo.yml`", {".github/workflows/foo.yml"}, cache
                ),
                ".github/workflows/foo.yml",
            )
            self.assertIsNone(
                RESOLVER._path_only_source_anchor(
                    "`customer.subscription.deleted`", {"README.md"}, cache
                )
            )
            # Mutation control: stripping source_files context must remove the
            # authority to inherit even when the token names a real file.
            self.assertIsNone(RESOLVER._path_only_source_anchor("`README.md`", set(), cache))

    def test_missing_named_concept_is_a_fatal_scan_failure(self) -> None:
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "/private/tmp/okf-concept-that-does-not-exist.md"],
            cwd=REPO_ROOT,
            text=True,
            capture_output=True,
            check=False,
        )

        self.assertEqual(result.returncode, 2)
        self.assertIn("FATAL", result.stderr)

    def test_official_gate_executes_this_contract_and_cite_re_mutation_is_red(self) -> None:
        """The B-059 diagnostic warning cannot be its only CI evidence."""
        workflow = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("run: python3 tests/test_okf_resolve_abbrev_cites.py", workflow)

        validator = VALIDATOR.read_text(encoding="utf-8")
        signature = 'CITE_RE = re.compile(r"^(?P<path>[A-Za-z0-9._/\\-]+):(?P<l1>'
        self.assertIn(signature, validator)

        # Mutation proof: the exact signature the open-polarity backlog oracle
        # requires cannot be silently removed while this test remains green.
        mutant = "\n".join(
            line for line in validator.splitlines() if not line.startswith("CITE_RE = ")
        )
        self.assertNotIn(signature, mutant)

        resolver = SCRIPT.read_text(encoding="utf-8")
        guard = "    if candidate not in declared_sources:\n        return None\n"
        self.assertIn(guard, resolver)
        mutant_source = resolver.replace(guard, "", 1)
        mutant = types.ModuleType("okf_resolve_abbrev_cites_without_source_context")
        mutant.__file__ = str(SCRIPT)
        exec(compile(mutant_source, str(SCRIPT), "exec"), mutant.__dict__)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "customer.subscription.deleted").write_text("decoy\n", encoding="utf-8")
            # The real guard rejects the dotted decoy; this explicit mutant
            # accepts it, so removing source_files context is necessarily RED.
            self.assertIsNone(
                RESOLVER._path_only_source_anchor(
                    "`customer.subscription.deleted`", {"README.md"}, RESOLVER.SourceCache(root)
                )
            )
            self.assertEqual(
                mutant._path_only_source_anchor(
                    "`customer.subscription.deleted`", {"README.md"}, mutant.SourceCache(root)
                ),
                "customer.subscription.deleted",
            )


if __name__ == "__main__":
    unittest.main()
