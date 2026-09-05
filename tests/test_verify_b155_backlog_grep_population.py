from __future__ import annotations

import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b155_backlog_grep_population.py"
spec = importlib.util.spec_from_file_location("b155_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)

REPAIR_SCRIPT = ROOT / "scripts/repair_b155_grep_population.py"
repair_spec = importlib.util.spec_from_file_location("b155_repair", REPAIR_SCRIPT)
assert repair_spec and repair_spec.loader
repair = importlib.util.module_from_spec(repair_spec)
sys.modules[repair_spec.name] = repair
repair_spec.loader.exec_module(repair)


class B155VerifierTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")

    def test_census_is_complete_and_population_is_closed(self) -> None:
        result = verifier.census(self.backlog)
        self.assertEqual(result.records, 170)
        self.assertEqual(result.command_records, 139)
        self.assertEqual(result.manual_records, 31)
        self.assertEqual(result.command_records + result.manual_records, result.records)
        self.assertEqual(result.grep_invocations, 301)
        self.assertEqual(len(result.assertions), 284)
        self.assertEqual(len(result.unsafe), 0)
        self.assertEqual(len(result.indeterminate), 0)

    def test_real_unanchored_member_mutation_changes_semantic_verdict(self) -> None:
        baseline = verifier.census(self.backlog)
        marker = 'grep -q "^[^#]*byok"'
        self.assertEqual(self.backlog.count(marker), 1)
        mutated = self.backlog.replace(marker, 'grep -q "byok"', 1)
        changed = verifier.census(mutated)
        self.assertGreater(len(changed.unsafe), len(baseline.unsafe))

    def test_parser_rejects_empty_population_instead_of_returning_done(self) -> None:
        with self.assertRaises(verifier.InstrumentError):
            verifier.census("no fenced backlog records")

    def test_exact_multiword_reproductions_are_comment_sensitive(self) -> None:
        # B087/B088/B125 were all missed when the detector tested only one
        # token from the pattern.  Keep the reproductions independent of the
        # repaired BACKLOG text so a future guard cannot delete the fixture.
        for record_id, pattern in (
            ("B-087", "Object Lock"),
            ("B-088", "No external pentest has been commissioned"),
            ("B-125", "fn resolve_seed"),
        ):
            check = verifier.GrepCheck(record_id, 1, pattern, "-q ", "grep", "'")
            self.assertTrue(verifier._matches_comment(check), record_id)

    def test_b084_fence_and_b164_positive_grep_removals_are_rejected(self) -> None:
        # B084: deleting an entire real record must not shrink the denominator
        # and leave a falsely clean census.
        fence = next(
            match
            for match in verifier.FENCE.finditer(self.backlog)
            if "id: B-084\n" in match.group(1)
        )
        without_b084 = self.backlog[: fence.start()] + self.backlog[fence.end() :]
        with self.assertRaises(verifier.InstrumentError):
            verifier.census(without_b084)

        # B164 still contains executable positive greps. Deleting one must
        # change the closed assertion population even though all records parse.
        record = next(
            item for item in verifier._records(self.backlog) if item["id"] == "B-164"
        )
        checks, _, _ = verifier._grep_checks(record)
        self.assertTrue(checks)
        fence = next(
            match
            for match in verifier.FENCE.finditer(self.backlog)
            if "id: B-164\n" in match.group(1)
        )
        raw_lines = fence.group(1).splitlines()
        raw_line = next(
            line for line in raw_lines
            if "grep" in line and "get_json_preserves_http_status_without_response_body" in line
        )
        raw_lines[raw_lines.index(raw_line)] = raw_line.replace("grep", "true", 1)
        mutated_block = "\n".join(raw_lines)
        mutated = self.backlog[: fence.start(1)] + mutated_block + self.backlog[fence.end(1) :]
        with self.assertRaises(verifier.InstrumentError):
            verifier.census(mutated)

    def test_unquoted_grep_in_if_is_parsed_and_reopens_gate(self) -> None:
        marker = 'grep -q "^[^#]*byok"'
        self.assertEqual(self.backlog.count(marker), 1)
        mutated = self.backlog.replace(marker, "if grep unsafe BACKLOG.md", 1)
        result = verifier.census(mutated)
        self.assertTrue(any(check.pattern == "unsafe" for check in result.unsafe))

    def test_nested_bash_c_grep_is_counted_and_guard_mutation_changes_verdict(self) -> None:
        record = next(item for item in verifier._records(self.backlog) if item["id"] == "B-112")
        checks, invocations, _ = verifier._grep_checks(record)
        self.assertEqual(invocations, 4)
        self.assertIn("^[^#]*cargo zigbuild", [check.pattern for check in checks])

        marker = 'grep -q "^[^#]*cargo zigbuild"'
        self.assertEqual(self.backlog.count(marker), 1)
        mutated = self.backlog.replace(marker, 'grep -q "cargo zigbuild"', 1)
        changed = verifier.census(mutated)
        baseline = verifier.census(self.backlog)
        self.assertGreater(len(changed.unsafe), len(baseline.unsafe))

    def test_dynamic_nested_shell_payload_fails_closed(self) -> None:
        record = {
            "id": "B-155",
            "verify": 'bash -c "$SCRIPT"',
        }
        with self.assertRaises(verifier.InstrumentError):
            verifier._grep_checks(record)

    def test_dynamic_nested_payload_expansions_fail_closed_at_every_position(self) -> None:
        # The old guard only inspected the beginning of the payload, allowing
        # an expansion to manufacture the rest of a script after a harmless
        # prefix.  A double-quoted argument is expanded by the invoking shell
        # at every position; all shell spellings must therefore be rejected.
        for shell in ("bash", "sh", "zsh"):
            for payload in (
                '"$SCRIPT; grep -q foo file"',
                '"echo hi $SCRIPT; grep -q foo file"',
                '"echo hi; grep -q foo file; $SCRIPT"',
                '"$(gen); grep -q foo file"',
                '"echo hi $(gen); grep -q foo file"',
                '"echo hi; grep -q foo file; $(gen)"',
            ):
                with self.subTest(shell=shell, payload=payload):
                    with self.assertRaises(verifier.InstrumentError):
                        verifier._grep_checks(
                            {"id": "B-155", "verify": f"{shell} -c {payload}"}
                        )

    def test_single_quoted_payload_is_literal_to_invoking_shell(self) -> None:
        # These expansions are data passed to the child shell, matching the
        # production backlog's literal bash -c bodies.  Conversely, a single
        # quote inside an outer double-quoted word does not suppress expansion
        # in the invoking shell and must still fail closed.
        for shell in ("bash", "sh", "zsh"):
            with self.subTest(shell=shell):
                checks, invocations, indeterminate = verifier._grep_checks(
                    {
                        "id": "B-155",
                        "verify": f"{shell} -c 'echo hi $SCRIPT; grep -q foo file'",
                    }
                )
                self.assertEqual(invocations, 1)
                self.assertEqual([check.pattern for check in checks], ["foo"])
                self.assertEqual(indeterminate, [])
                with self.assertRaises(verifier.InstrumentError):
                    verifier._grep_checks(
                        {
                            "id": "B-155",
                            "verify": f'{shell} -c "echo \'$SCRIPT\'; grep -q foo file"',
                        }
                    )

    def test_escaped_quotes_in_double_quoted_payload_are_decoded(self) -> None:
        checks, invocations, indeterminate = verifier._grep_checks(
            {"id": "B-155", "verify": 'bash -c "grep -q \\"foo\\" file.rs"'}
        )
        self.assertEqual(invocations, 1)
        self.assertEqual(indeterminate, [])
        self.assertEqual(len(checks), 1)
        self.assertEqual(checks[0].pattern, "foo")
        self.assertEqual(checks[0].quote, '"')

    def test_bre_ere_fixed_and_shell_variable_semantics_are_not_literal(self) -> None:
        self.assertIsNotNone(verifier._as_python_regex(r"foo\|bar").search("bar"))
        self.assertIsNone(verifier._as_python_regex(r"foo|bar").search("bar"))
        self.assertIsNotNone(verifier._as_python_regex("foo|bar", "-E ").search("bar"))
        self.assertIsNotNone(verifier._as_python_regex("foo|bar", "-F ").search("foo|bar"))
        dynamic = verifier.GrepCheck("B-155", 1, "$needle", "-q ", "grep", '"')
        self.assertTrue(verifier._matches_comment(dynamic))

    def test_mutation_self_test_covers_fixture_and_completeness_guard(self) -> None:
        verifier.mutation_self_test(self.backlog)

    def test_syntax_manifest_covers_comment_forms(self) -> None:
        for suffix, kind in (("rs", "rust"), ("ts", "typescript"), ("yml", "config"), ("md", "markdown"), ("sh", "shell")):
            with self.subTest(suffix=suffix):
                checks, invocations, indeterminate = verifier._grep_checks(
                    {"id": "B-155", "verify": f'grep -q "needle" file.{suffix}'}
                )
                self.assertEqual(invocations, 1)
                self.assertEqual(indeterminate, [])
                self.assertEqual(checks[0].source_kind, kind)
                self.assertTrue(verifier._matches_comment(checks[0]))

    def test_wrapped_grep_forms_are_counted_or_fail_closed(self) -> None:
        # A wrapper must not hide a grep from the command census.  File-backed
        # forms classify normally; a dynamic/no-file form is indeterminate and
        # therefore red rather than silently disappearing.
        wrapped = (
            "sudo grep -q \"needle\" file.rs",
            "env grep -q \"needle\" file.rs",
            "git grep -q \"needle\" file.rs",
            "xargs grep -q \"needle\" file.rs",
            "2>/dev/null grep -q \"needle\" file.rs",
        )
        for verify in wrapped:
            with self.subTest(verify=verify):
                checks, invocations, indeterminate = verifier._grep_checks(
                    {"id": "B-155", "verify": verify}
                )
                self.assertEqual(invocations, 1)
                self.assertEqual(len(checks), 1)
                self.assertEqual(checks[0].source_kind, "rust")
                self.assertEqual(indeterminate, [])

        checks, invocations, indeterminate = verifier._grep_checks(
            {"id": "B-155", "verify": "sudo grep -q \"needle\""}
        )
        self.assertEqual(invocations, 1)
        self.assertEqual(len(checks), 1)
        self.assertEqual(checks[0].source_kind, "unknown")
        self.assertEqual(len(indeterminate), 1)
        # Unknown syntax is fail-closed at census level, even when the helper
        # is exercised directly with a synthetic record.
        with self.assertRaises(verifier.InstrumentError):
            verifier.census(
                "```backlog\n"
                "id: B-155\nrepo: corelink-server\nowner: tl\nstatus: open\n"
                "verify: sudo grep -q needle\nverify-means: wrapped\n```\n"
            )

    def test_repair_is_idempotent_and_never_uses_global_prefix(self) -> None:
        rewritten, changed = repair.repair(self.backlog)
        self.assertEqual(changed, 0)
        self.assertEqual(rewritten, self.backlog)
        self.assertNotIn("^[^#/<*-]*", self.backlog)

    def test_b061_extracted_shell_payload_is_syntactically_valid(self) -> None:
        record = next(item for item in verifier._records(self.backlog) if item["id"] == "B-061")
        verify = record["verify"]
        self.assertIsInstance(verify, str)
        match = verifier.NESTED_SHELL.search(verify)
        self.assertIsNotNone(match)
        assert match is not None
        position = match.end()
        while position < len(verify) and verify[position].isspace():
            position += 1
        payload, _ = verifier._decode_nested_payload(verify, position)
        parsed = subprocess.run(
            ["bash", "-n"], input=payload, text=True, capture_output=True, check=False
        )
        self.assertEqual(parsed.returncode, 0, parsed.stderr)


if __name__ == "__main__":
    unittest.main()
