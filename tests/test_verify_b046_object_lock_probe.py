import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b046_object_lock_probe", ROOT / "scripts/verify_b046_object_lock_probe.py"
)
assert SPEC and SPEC.loader
verifier = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = verifier
SPEC.loader.exec_module(verifier)


class B046ObjectLockProbeTests(unittest.TestCase):
    def test_only_explicit_not_implemented_is_provider_block(self) -> None:
        self.assertEqual(verifier.classify_operation(1, "", "NotImplemented"), "NOT_SUPPORTED")
        self.assertEqual(verifier.classify_operation(1, "", "Not Implemented"), "NOT_SUPPORTED")
        self.assertEqual(verifier.classify_operation(1, "", "AccessDenied"), "INDETERMINATE")
        self.assertEqual(verifier.classify_operation(1, "", "object lock unsupported"), "INDETERMINATE")
        self.assertEqual(verifier.classify_operation(1, "", "NotImplementedError"), "INDETERMINATE")
        self.assertEqual(verifier.classify_operation(1, "", "timeout"), "INDETERMINATE")
        self.assertEqual(verifier.classify_operation(0, "NotImplemented", ""), "PASS")

    def test_both_operations_are_required(self) -> None:
        passed = lambda name: verifier.OperationResult(name, "PASS", 0, "")
        blocked = lambda name: verifier.OperationResult(name, "NOT_SUPPORTED", 1, "NotImplemented")
        unknown = lambda name: verifier.OperationResult(name, "INDETERMINATE", 1, "AccessDenied")
        skipped = lambda name: verifier.OperationResult(name, "SKIPPED", 125, "not attempted")
        self.assertEqual(verifier.evaluate_operations(passed("create"), passed("put")), "SUPPORTED")
        self.assertEqual(verifier.evaluate_operations(blocked("create"), passed("put")), "BLOCKED")
        self.assertEqual(verifier.evaluate_operations(passed("create"), unknown("put")), "INDETERMINATE")
        self.assertEqual(verifier.evaluate_operations(blocked("create"), skipped("put")), "BLOCKED")
        self.assertEqual(verifier.evaluate_operations(unknown("create"), skipped("put")), "INDETERMINATE")

    def test_aws_credentials_are_child_env_only_and_redacted(self) -> None:
        access_key, secret_key, session_token = "ak", "sk", "tok"
        child_env = verifier._aws_child_env(access_key, secret_key, session_token)
        self.assertEqual(child_env["AWS_ACCESS_KEY_ID"], access_key)
        self.assertEqual(child_env["AWS_SECRET_ACCESS_KEY"], secret_key)
        self.assertEqual(child_env["AWS_SESSION_TOKEN"], session_token)
        self.assertNotIn("R2_S3_ACCESS_KEY_ID", child_env)
        self.assertNotIn("R2_S3_SECRET_ACCESS_KEY", child_env)
        self.assertNotIn("R2_S3_SESSION_TOKEN", child_env)
        fake_cli = (
            "import os, sys; "
            "print('env-ok' if all(os.getenv(name) for name in "
            "('AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY', 'AWS_SESSION_TOKEN')) "
            "else 'env-missing'); "
            "print('argv-clean' if len(sys.argv) == 1 else 'argv-extra'); "
            "print('|'.join(os.getenv(name, '') for name in "
            "('AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY', 'AWS_SESSION_TOKEN')))"
        )
        command = (sys.executable, "-c", fake_cli)
        result = verifier._run(command, (access_key, secret_key, session_token), child_env)
        self.assertEqual(result.status, "PASS")
        self.assertIn("env-ok", result.detail)
        self.assertIn("argv-clean", result.detail)
        self.assertIn("<redacted>", result.detail)
        for secret in (access_key, secret_key, session_token):
            self.assertNotIn(secret, result.detail)

        # Mutation: putting any credential in argv is rejected before the fake
        # CLI can run, pinning the env-only contract at the process boundary.
        with self.assertRaises(verifier.ProbeError):
            verifier._run((*command, access_key), (access_key, secret_key, session_token), child_env)

    def test_contract_and_mutations_fail_closed(self) -> None:
        verifier.validate_repository_contract()
        paths = {path: Path(path).read_text(encoding="utf-8") for path in verifier._required_markers()}

        mutated = dict(paths)
        backlog = verifier.BACKLOG_PATH.as_posix()
        b046_start = mutated[backlog].index("id: B-046")
        prefix, suffix = mutated[backlog][:b046_start], mutated[backlog][b046_start:]
        mutated[backlog] = prefix + suffix.replace("status: parked", "status: done", 1)
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_repository_contract(mutated)

        mutated = dict(paths)
        mutated[backlog] = prefix + suffix.replace(
            "verify: " + verifier.B046_VERIFY_COMMAND, "verify: true", 1
        )
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_repository_contract(mutated)

        mutated = dict(paths)
        mutated[backlog] += "\n```backlog\nid: B-046\nstatus: parked\nverify: manual\n```\n"
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_repository_contract(mutated)

        mutated = dict(paths)
        migration = verifier.MIGRATION_PATH.as_posix()
        mutated[migration] = mutated[migration].replace("CHECK (mode IN ('governance'))", "CHECK (mode IN ('governance', 'compliance'))")
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_repository_contract(mutated)

        mutated = dict(paths)
        adapter = verifier.ADAPTER_PATH.as_posix()
        mutated[adapter] = mutated[adapter].replace("NOT storage immutability", "storage immutability", 1)
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_repository_contract(mutated)

        mutated = dict(paths)
        okf = verifier.OKF_PATH.as_posix()
        mutated[okf] = mutated[okf].replace("INDETERMINATE", "PASS")
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_repository_contract(mutated)

        mutated = dict(paths)
        workflow = verifier.WORKFLOW_PATH.as_posix()
        mutated[workflow] = mutated[workflow].replace("scripts/verify_b046_object_lock_probe.py", "scripts/missing.py")
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_repository_contract(mutated)

    def test_receipt_schema_and_capability_polarity_fail_closed(self) -> None:
        evidence_path = verifier.EVIDENCE_PATH
        raw = evidence_path.read_text(encoding="utf-8")
        verifier.validate_evidence_record(raw)

        malformed = raw.replace('"classification": "INDETERMINATE"', '"classification": "SUPPORTED"', 1)
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_evidence_record(malformed)

        malformed = raw.replace('"status": "SKIPPED"', '"status": "MALFORMED"', 1)
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_evidence_record(malformed)

        malformed = raw.replace('"detail": "provider rejected', '"detail": "AWS_ACCESS_KEY_ID provider rejected', 1)
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_evidence_record(malformed)

        malformed = raw.replace(
            '"classification": "INDETERMINATE",',
            '"classification": "INDETERMINATE",\n  "classification": "INDETERMINATE",',
            1,
        )
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_evidence_record(malformed)

        malformed = raw.replace(
            '"operation": "CreateBucket with Object Lock enabled"',
            '"operation": "ListBuckets"',
            1,
        )
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_evidence_record(malformed)

        malformed = raw.replace(
            '"status": "INDETERMINATE"',
            '"status": "PASS"',
            1,
        )
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_evidence_record(malformed)

        malformed = raw.replace(
            '"captured_at": "2026-09-09T04:13:53Z"',
            '"captured_at": "2026-09-09 04:13:53"',
            1,
        )
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_evidence_record(malformed)


if __name__ == "__main__":
    unittest.main()
