import hashlib
import importlib.util
import json
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

LIVE_RECEIPT_SPEC = importlib.util.spec_from_file_location(
    "build_b046_object_lock_receipt", ROOT / "scripts/build_b046_object_lock_receipt.py"
)
assert LIVE_RECEIPT_SPEC and LIVE_RECEIPT_SPEC.loader
live_receipt = importlib.util.module_from_spec(LIVE_RECEIPT_SPEC)
sys.modules[LIVE_RECEIPT_SPEC.name] = live_receipt
LIVE_RECEIPT_SPEC.loader.exec_module(live_receipt)


def live_receipt_inputs() -> dict[str, str]:
    return {
        "GITHUB_REPOSITORY": "HuGR-dev/corelink-server",
        "GITHUB_REPOSITORY_ID": "1232040291",
        "GITHUB_SERVER_URL": "https://github.com",
        "GITHUB_RUN_ID": "123456789",
        "GITHUB_RUN_ATTEMPT": "2",
        "GITHUB_SHA": "a" * 40,
        "B046_CHECKED_OUT_SHA": "a" * 40,
        "AWS_ACCOUNT_ID": "123456789012",
        "B046_ACTUAL_ACCOUNT_ID": "123456789012",
        "AWS_REGION": "us-east-1",
        "B046_ACTUAL_REGION": "us-east-1",
        "JURISDICTION": "US-EAST",
        "AWS_BUCKET_PREFIX": "corelink-object-lock-probe-test",
        "B046_BUCKET": "corelink-object-lock-probe-test-123456789-2",
        "B046_KEY": "audit/probe-123456789/synthetic.txt",
        "B046_VERSION": "opaque-version-id",
        "B046_PUT_REQUEST_ID": "put-request-123",
        "B046_CLOUDTRAIL_PUT_EVENT_ID": "put-event-123",
        "B046_CLOUDTRAIL_DELETE_EVENT_ID": "delete-event-456",
        "B046_RETAIN_UNTIL": "2026-09-26T06:02:42+00:00",
        "APPROVAL_REFERENCE": "approval-1646",
        "COST_CEILING_USD_MICROS": "5000000",
        "COST_OWNER": "aws-cost-owner",
        "CLEANUP_OWNER": "aws-cleanup-owner",
    }


DIGEST_VALIDATION_OK = """\
Validating log files for trail between 2026-09-25T06:00:00Z and 2026-09-25T06:20:00Z
Digest file s3://redacted/CloudTrail-Digest/example valid
Log file s3://redacted/CloudTrail/example valid
1/1 digest files valid
1/1 log files valid
"""


class B046ObjectLockProbeTests(unittest.TestCase):
    def test_protected_receipt_binds_redacted_target_and_owner_controls(self) -> None:
        inputs = live_receipt_inputs()
        receipt = live_receipt.build_receipt(
            inputs, digest_validation_output=DIGEST_VALIDATION_OK
        )
        unsigned = {key: value for key, value in receipt.items() if key != "receipt_sha256"}
        digest = hashlib.sha256(
            json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode("utf-8")
        ).hexdigest()

        self.assertEqual(receipt["schema"], "corelink.b046.s3-object-lock-protected-proof.v1")
        self.assertEqual(receipt["receipt_sha256"], digest)
        self.assertEqual(
            receipt["workflow_url"],
            "https://github.com/HuGR-dev/corelink-server/actions/runs/123456789",
        )
        self.assertEqual(
            receipt["target"]["account_id_sha256"],
            hashlib.sha256(inputs["AWS_ACCOUNT_ID"].encode()).hexdigest(),
        )
        self.assertEqual(
            receipt["target"]["bucket_name_sha256"],
            hashlib.sha256(inputs["B046_BUCKET"].encode()).hexdigest(),
        )
        self.assertEqual(
            receipt["target"]["object_key_sha256"],
            hashlib.sha256(inputs["B046_KEY"].encode()).hexdigest(),
        )
        self.assertEqual(
            receipt["target"]["object_version_sha256"],
            hashlib.sha256(inputs["B046_VERSION"].encode()).hexdigest(),
        )
        self.assertEqual(receipt["target"]["region"], "us-east-1")
        self.assertTrue(receipt["target"]["region_matches_configured_target"])
        self.assertTrue(receipt["bucket"]["versioning_enabled"])
        self.assertEqual(receipt["object"]["retain_until"], "2026-09-26T06:02:42Z")
        self.assertEqual(receipt["object"]["put_data_event_id"], "put-event-123")
        self.assertEqual(receipt["object"]["delete_denial_data_event_id"], "delete-event-456")
        self.assertEqual(receipt["cloudtrail_digest_validation"], "passed")
        self.assertEqual(
            receipt["cloudtrail_digest_output_sha256"],
            hashlib.sha256(DIGEST_VALIDATION_OK.encode("utf-8")).hexdigest(),
        )
        self.assertEqual(receipt["cost_ceiling_usd_micros"], 5_000_000)
        self.assertEqual(receipt["cost_owner"], "aws-cost-owner")
        self.assertEqual(receipt["cleanup_owner"], "aws-cleanup-owner")
        encoded = json.dumps(receipt)
        for private_identifier in (
            inputs["AWS_ACCOUNT_ID"],
            inputs["B046_BUCKET"],
            inputs["B046_KEY"],
            inputs["B046_VERSION"],
        ):
            self.assertNotIn(private_identifier, encoded)

    def test_protected_receipt_rejects_target_drift_and_unapproved_cost(self) -> None:
        for field, value in (
            ("B046_ACTUAL_ACCOUNT_ID", "999999999999"),
            ("B046_ACTUAL_REGION", "eu-west-1"),
            ("B046_CHECKED_OUT_SHA", "b" * 40),
            ("COST_CEILING_USD_MICROS", "5000001"),
            ("B046_BUCKET", "unapproved-bucket"),
        ):
            with self.subTest(field=field):
                inputs = live_receipt_inputs()
                inputs[field] = value
                with self.assertRaises(live_receipt.ReceiptError):
                    live_receipt.build_receipt(
                        inputs, digest_validation_output=DIGEST_VALIDATION_OK
                    )

    def test_protected_receipt_rejects_missing_or_invalid_digest_validation(self) -> None:
        for output in (
            "",
            "1/2 digest files valid\n1/1 log files valid\n",
            "1/1 digest files valid\n0/0 log files valid\n",
            "1/1 digest files valid\n1/1 log files valid\nDigest file INVALID: signature verification failed\n",
            "1/1 digest files valid\n1/1 log files valid\n1/2 backfill digest files INVALID\n",
        ):
            with self.subTest(output=output):
                with self.assertRaises(live_receipt.ReceiptError):
                    live_receipt.build_receipt(
                        live_receipt_inputs(), digest_validation_output=output
                    )

    def test_protected_workflow_reads_back_versioning_and_uses_receipt_builder(self) -> None:
        workflow = (ROOT / ".github/workflows/aws-s3-object-lock-live-proof.yml").read_text(
            encoding="utf-8"
        )

        self.assertIn("aws s3api get-bucket-versioning --bucket \"$bucket\"", workflow)
        self.assertIn("jq -e '.Status == \"Enabled\"'", workflow)
        self.assertIn("scripts/build_b046_object_lock_receipt.py", workflow)
        self.assertIn("ref: ${{ github.sha }}", workflow)
        self.assertIn('--start-time "$digest_validation_start"', workflow)
        self.assertIn('--end-time "$digest_validation_end" --verbose', workflow)
        self.assertIn('--digest-validation-output "$RUNNER_TEMP/digest-validation.txt"', workflow)
        self.assertIn("B046_ACTUAL_ACCOUNT_ID=\"$actual_account\"", workflow)
        self.assertIn("B046_ACTUAL_REGION=\"$actual_region\"", workflow)
        self.assertIn("B046_CHECKED_OUT_SHA=\"$(git rev-parse HEAD)\"", workflow)

    def test_cleanup_expires_retention_before_releasing_exact_version_legal_hold(self) -> None:
        workflow = (ROOT / ".github/workflows/aws-s3-object-lock-live-proof.yml").read_text(
            encoding="utf-8"
        )
        cleanup = workflow.split("      - name: Clean up the exact expired synthetic probe", 1)[1]
        cleanup = cleanup.split("      - name: Publish redacted proof and cleanup obligation", 1)[0]

        retention_read = 'aws s3api get-object-retention --bucket "$bucket" --key "$expected_key" --version-id "$version"'
        expiry_check = 'test "$(date -u -d "$expiry" +%s)" -le "$(date -u +%s)"'
        hold_release = 'aws s3api put-object-legal-hold --bucket "$bucket" --key "$expected_key" --version-id "$version"'
        delete_version = 'aws s3api delete-object --bucket "$bucket" --key "$expected_key" --version-id "$version"'

        self.assertIn('[[ "$bucket" =~ ^${AWS_BUCKET_PREFIX}-[0-9]+-[1-9][0-9]*$ ]]', cleanup)
        self.assertIn('expected_key="audit/probe-${SOURCE_RUN_ID}/synthetic.txt"', cleanup)
        self.assertIn('length == 1', cleanup)
        self.assertLess(cleanup.index(retention_read), cleanup.index(expiry_check))
        self.assertLess(cleanup.index(expiry_check), cleanup.index(hold_release))
        self.assertLess(cleanup.index(hold_release), cleanup.index(delete_version))
        self.assertIn("--legal-hold '{\"Status\":\"OFF\"}'", cleanup)
        self.assertIn("jq -e '.LegalHold.Status == \"OFF\"'", cleanup)

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
            '"attempted": false,\n    "resources_created": false,',
            '"attempted": true,\n    "resources_created": true,',
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

        malformed = raw.replace('"schema_version": 1', '"schema_version": true', 1)
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_evidence_record(malformed)

        malformed = raw.replace(
            '"captured_at": "2026-09-09T04:13:53Z"',
            '"captured_at": "2026-9-9T4:13:53Z"',
            1,
        )
        with self.assertRaises(verifier.ProbeError):
            verifier.validate_evidence_record(malformed)


if __name__ == "__main__":
    unittest.main()
