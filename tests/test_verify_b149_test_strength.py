from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b149_test_strength.py"
spec = importlib.util.spec_from_file_location("b149_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


VARIANTS = (
    "BadTenantId",
    "BadBillingPeriod",
    "BadRegion",
    "BadIdemKey",
    "EmptySource",
    "SourceTooLong",
)


def fixture_sources() -> dict[str, str]:
    enum = "\n    ".join(f"{name}," for name in VARIANTS[:-1]) + f"\n    {VARIANTS[-1]}"
    arms = "\n        ".join(
        f'RecordError::{name} => "{verifier._camel_reason(name)}",' for name in VARIANTS
    )
    cases = "\n        ".join(
        f'(RecordError::{name}, "{verifier._camel_reason(name)}"),' for name in VARIANTS
    )
    return {
        verifier.AUDIT: """
fn empty_batch_issues_no_statement_at_all() {
    let rows: Vec<AuditRow> = Vec::new();
    let statements = D1AuditOutboxSink::build_batch_statements(&rows).unwrap();
    assert!(statements.is_empty());
}
""",
        verifier.AUTH: """
fn build_state_returns_none_when_secret_absent() {
    let key = configured_ingest_auth_key(None);
    assert!(key.is_none());
}
""",
        verifier.INGEST: f"""
enum RecordError {{
    {enum}
}}
impl RecordError {{
    const fn code(&self) -> &'static str {{
        match self {{
            {''.join(f'Self::{name} => "{verifier._camel_reason(name)}",' for name in VARIANTS)}
        }}
    }}
}}
""",
        verifier.VALIDATE: f"""
fn record_error_variant_name(error: RecordError) -> &'static str {{
    match error {{
        {arms}
    }}
}}
fn every_record_error_variant_has_a_rejection_fixture() {{
    let fixtures = [
        {cases}
    ];
    for (expected, reason) in fixtures {{
        let actual = validate_record(wire_for(expected));
        assert_eq!(actual, Err(expected));
        assert_eq!(expected.code(), reason);
    }}
}}
""",
        verifier.SKIP: """
fn bad_tenant_id_is_skipped_not_fatal() {
    assert_eq!(RecordError::BadTenantId.code(), "bad_tenant_id");
}
""",
    }


class B149VerifierTests(unittest.TestCase):
    def make_root(self) -> tuple[tempfile.TemporaryDirectory[str], Path, dict[str, str]]:
        temp = tempfile.TemporaryDirectory()
        root = Path(temp.name)
        sources = fixture_sources()
        for relative, content in sources.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        return temp, root, sources

    def assert_gap(self, mutate, expected: str) -> None:
        temp, root, sources = self.make_root()
        with temp:
            mutate(sources)
            for relative, content in sources.items():
                (root / relative).write_text(content, encoding="utf-8")
            self.assertIn(expected, verifier.assess(root))

    def test_explicit_open_fixture_has_exactly_four_gaps(self) -> None:
        temp, root, sources = self.make_root()
        with temp:
            sources[verifier.AUDIT] = """
fn empty_batch_issues_no_statement_at_all() {
    append_batch_async(Vec::new());
}
"""
            sources[verifier.AUTH] = """
fn build_state_returns_none_when_secret_absent() {
    if std::env::var("BILLING_INGEST_AUTH_KEY").is_err() {
        assert!(configured_ingest_auth_key(None).is_none());
    }
}
"""
            sources[verifier.VALIDATE] = """
fn record_error_variant_name(error: RecordError) -> &'static str {
    match error { fallback => "fallback" }
}
fn every_record_error_variant_has_a_rejection_fixture() {}
"""
            sources[verifier.SKIP] = """
fn bad_tenant_id_is_skipped_not_fatal() {
    validate_record_reasons_pinned();
}
"""
            for relative, content in sources.items():
                (root / relative).write_text(content, encoding="utf-8")
            self.assertEqual(
                verifier.assess(root),
                [
                    "empty_batch-no-empty-statement-assertion",
                    "secret-absent-is-environment-conditional",
                    "record-error-fixtures-or-reason-codes-not-exhaustive",
                    "record-skip-delegation-does-not-reference-exhaustive-proof",
                ],
            )

    def test_fully_fixed_fixture_is_done_and_cli_reverses_expectation(self) -> None:
        temp, root, _ = self.make_root()
        with temp:
            self.assertEqual(verifier.assess(root), [])
            done = subprocess.run(
                [sys.executable, str(SCRIPT), "--root", str(root), "--expect", "done"],
                text=True, capture_output=True, check=False,
            )
            opened = subprocess.run(
                [sys.executable, str(SCRIPT), "--root", str(root), "--expect", "open"],
                text=True, capture_output=True, check=False,
            )
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertEqual(opened.returncode, 1)

    def test_tokens_in_comment_string_or_neighbor_function_do_not_count(self) -> None:
        def comment(s):
            s[verifier.AUDIT] = "fn empty_batch_issues_no_statement_at_all() { /* Vec::new(); build_batch_insert; assert!(statements.is_empty()); */ }"
        def string(s):
            s[verifier.AUDIT] = 'fn empty_batch_issues_no_statement_at_all() { let lie = "Vec::new(); build_batch_insert; assert!(statements.is_empty());"; }'
        def neighbor(s):
            s[verifier.AUDIT] = """fn empty_batch_issues_no_statement_at_all() {}
fn unrelated() { let rows: Vec<AuditRow> = Vec::new(); let statements = rows.map(build_batch_insert); assert!(statements.is_empty()); }"""
        for mutation in (comment, string, neighbor):
            with self.subTest(mutation=mutation.__name__):
                self.assert_gap(mutation, "empty_batch-no-empty-statement-assertion")

    def test_disconnected_audit_result_does_not_count(self) -> None:
        self.assert_gap(lambda s: s.__setitem__(verifier.AUDIT, """
fn empty_batch_issues_no_statement_at_all() {
    let _ignored = D1AuditOutboxSink::build_batch_statements(&[]).unwrap();
    let statements: Vec<Statement> = Vec::new();
    assert!(statements.is_empty());
}
"""), "empty_batch-no-empty-statement-assertion")

    def test_auth_requires_direct_none_without_environment_access(self) -> None:
        self.assert_gap(lambda s: s.__setitem__(verifier.AUTH, """
fn build_state_returns_none_when_secret_absent() {
    if std::env::var("BILLING_INGEST_AUTH_KEY").is_err() {
        let key = configured_ingest_auth_key(None); assert!(key.is_none());
    }
}"""), "secret-absent-is-environment-conditional")
        self.assert_gap(lambda s: s.__setitem__(verifier.AUTH, """
fn build_state_returns_none_when_secret_absent() {
    if false { assert!(configured_ingest_auth_key(None).is_none()); }
}
"""), "secret-absent-is-environment-conditional")

    def test_fixture_removal_and_duplicate_replacement_are_detected(self) -> None:
        self.assert_gap(lambda s: s.__setitem__(
            verifier.VALIDATE, s[verifier.VALIDATE].replace('(RecordError::BadRegion, "bad_region"),', ""),
        ), "record-error-fixtures-or-reason-codes-not-exhaustive")

    def test_discarded_validation_and_code_results_do_not_count(self) -> None:
        self.assert_gap(lambda s: s.__setitem__(
            verifier.VALIDATE,
            s[verifier.VALIDATE].replace(
                """let actual = validate_record(wire_for(expected));
        assert_eq!(actual, Err(expected));
        assert_eq!(expected.code(), reason);""",
                """let _actual = validate_record(wire_for(expected));
        let _code = expected.code();""",
            ),
        ), "record-error-fixtures-or-reason-codes-not-exhaustive")
        self.assert_gap(lambda s: s.__setitem__(
            verifier.VALIDATE, s[verifier.VALIDATE].replace('(RecordError::BadRegion, "bad_region"),', '(RecordError::BadTenantId, "bad_region"),'),
        ), "record-error-fixtures-or-reason-codes-not-exhaustive")

    def test_final_variant_without_comma_is_parsed_and_named_wildcard_fails(self) -> None:
        temp, root, _ = self.make_root()
        with temp:
            self.assertEqual(verifier.assess(root), [])
        self.assert_gap(lambda s: s.__setitem__(
            verifier.VALIDATE, s[verifier.VALIDATE].replace('RecordError::SourceTooLong => "source_too_long",', 'fallback => "source_too_long",'),
        ), "record-error-fixtures-or-reason-codes-not-exhaustive")

    def test_swapped_reason_and_old_delegation_are_detected(self) -> None:
        self.assert_gap(lambda s: s.__setitem__(
            verifier.VALIDATE, s[verifier.VALIDATE].replace('"bad_tenant_id"', '"bad_tenant_id_swapped"'),
        ), "record-error-fixtures-or-reason-codes-not-exhaustive")
        self.assert_gap(lambda s: s.__setitem__(
            verifier.SKIP, "fn bad_tenant_id_is_skipped_not_fatal() { validate_record_reasons_pinned(); }",
        ), "record-skip-delegation-does-not-reference-exhaustive-proof")

    def test_production_reason_mapping_mutation_is_detected(self) -> None:
        self.assert_gap(lambda s: s.__setitem__(
            verifier.INGEST,
            s[verifier.INGEST].replace(
                'Self::BadTenantId => "bad_tenant_id"',
                'Self::BadTenantId => "bad_region"',
            ),
        ), "record-error-fixtures-or-reason-codes-not-exhaustive")

    def test_missing_file_and_malformed_braces_are_instrument_errors(self) -> None:
        temp, root, sources = self.make_root()
        with temp:
            (root / verifier.AUTH).unlink()
            with self.assertRaises(verifier.InstrumentError):
                verifier.assess(root)
        temp, root, sources = self.make_root()
        with temp:
            sources[verifier.INGEST] = "enum RecordError { BadTenantId, BadRegion "
            (root / verifier.INGEST).write_text(sources[verifier.INGEST], encoding="utf-8")
            with self.assertRaises(verifier.InstrumentError):
                verifier.assess(root)


if __name__ == "__main__":
    unittest.main()
