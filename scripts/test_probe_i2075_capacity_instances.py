import importlib.util
import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = (ROOT / ".github/workflows/issue-2075-capacity-readonly.yml").read_text(encoding="utf-8")
SPEC = importlib.util.spec_from_file_location(
    "probe_i2075_capacity_instances", ROOT / "scripts/probe_i2075_capacity_instances.py"
)
assert SPEC is not None and SPEC.loader is not None
PROBE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROBE)


def application_rows() -> list[dict[str, str]]:
    rows = [
        {"id": f"00000000-0000-0000-0000-{index:012d}", "name": name, "image": "private-image-canary"}
        for index, name in enumerate(PROBE.APP_NAMES, start=1)
    ]
    rows.append({"id": "11111111-1111-1111-1111-111111111111", "name": "tenant_42", "image": "acct123"})
    return rows


class Issue2075CapacityReceiptTests(unittest.TestCase):
    def test_fixed_cli_reads_project_only_aggregate_counts(self) -> None:
        calls: list[list[str]] = []

        def run(args: list[str]) -> bytes:
            calls.append(args)
            if args == ["containers", "list", "--json"]:
                return json.dumps(application_rows()).encode()
            app_id = args[2]
            index = int(app_id[-1], 16)
            payload = [
                {"id": f"instance-canary-{index}", "name": "customer_name", "state": "running"},
                {"id": f"inactive-canary-{index}", "name": "tenant_42", "state": "stopped"},
            ]
            return json.dumps(payload).encode()

        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "receipt.json"
            with mock.patch.object(PROBE, "_run_wrangler", side_effect=run):
                receipt = PROBE.capture(
                    "0123456789abcdef0123456789abcdef",
                    "a" * 40,
                    output,
                )
            artifact = output.read_text(encoding="utf-8")

        self.assertEqual(len(calls), 1 + len(PROBE.APP_NAMES))
        self.assertEqual(calls[0], ["containers", "list", "--json"])
        self.assertTrue(all(call[0:2] == ["containers", "instances"] and call[-1] == "--json" for call in calls[1:]))
        instances = receipt["observed_production_instances"]
        self.assertEqual(instances["running_instances"], len(PROBE.APP_NAMES))
        self.assertEqual(
            instances["running_instance_resource_allocation_estimate"]["vcpu"],
            len(PROBE.APP_NAMES) * PROBE.INSTANCE_SHAPE["vcpu"],
        )
        self.assertIsNone(receipt["concurrency"]["active_tenant_region_assignments"])
        self.assertIsNone(receipt["concurrency"]["deduplicated_tenant_count"])
        self.assertEqual(receipt["concurrency"]["status"], "unavailable")
        self.assertEqual(receipt["account_limits"]["vcpu"], 1500)
        self.assertFalse(receipt["account_limits"]["account_specific_entitlement_verified"])
        for canary in (
            "tenant_42",
            "acct123",
            "customer_name",
            "private-image-canary",
            "instance-canary",
            "inactive-canary",
            "0123456789abcdef0123456789abcdef",
        ):
            self.assertNotIn(canary, artifact)
        for application_name in PROBE.APP_NAMES:
            self.assertNotIn(application_name, artifact)

    def test_unknown_state_fails_closed_without_writing_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "receipt.json"
            def run(args: list[str]) -> bytes:
                if args == ["containers", "list", "--json"]:
                    return json.dumps(application_rows()).encode()
                return b'[{"id":"id-canary","name":"tenant_42","state":"future-provider-state"}]'

            with mock.patch.object(PROBE, "_run_wrangler", side_effect=run):
                with self.assertRaisesRegex(PROBE.ProbeError, "unknown"):
                    PROBE.capture("0123456789abcdef0123456789abcdef", "a" * 40, output)
            self.assertFalse(output.exists())

    def test_missing_or_duplicate_allowlisted_application_fails_closed(self) -> None:
        rows = application_rows()
        with self.assertRaisesRegex(PROBE.ProbeError, "did not resolve exactly"):
            PROBE._application_ids(rows[:-2])
        with self.assertRaisesRegex(PROBE.ProbeError, "duplicate"):
            PROBE._application_ids(rows + [rows[0]])

    def test_command_errors_are_not_echoed_to_logs(self) -> None:
        stdout, stderr = io.StringIO(), io.StringIO()
        with (
            mock.patch.object(PROBE, "capture", side_effect=PROBE.ProbeError("read-only provider command failed")),
            mock.patch.object(PROBE.os, "environ", {"CLOUDFLARE_CAPACITY_READ_TOKEN": "secret-canary"}),
            mock.patch.object(sys, "argv", ["probe", "--output", "/tmp/unused-receipt.json"]),
            redirect_stdout(stdout),
            redirect_stderr(stderr),
        ):
            self.assertEqual(PROBE.main(), 2)
        log = stdout.getvalue() + stderr.getvalue()
        self.assertIn("FAIL-CLOSED", log)
        self.assertNotIn("secret-canary", log)
        self.assertNotIn("tenant_42", log)

    def test_refresh_workflow_is_exact_main_manual_and_receipt_only(self) -> None:
        for expected in (
            "workflow_dispatch:",
            "run-2075-read-only",
            "refs/heads/main",
            "production-capacity-read",
            "CLOUDFLARE_CAPACITY_READ_TOKEN",
            "CLOUDFLARE_ACCOUNT_ID",
            "containers list --json",
            "containers instances",
            "persist-credentials: false",
            "secrets.CF_ACCOUNT_ID",
            "issue-2075-capacity-read-only-receipt.json",
        ):
            self.assertIn(expected, WORKFLOW)
        for forbidden in ("pull_request:", "schedule:", "tee ", "POST", "PATCH", "DELETE"):
            self.assertNotIn(forbidden, WORKFLOW)


if __name__ == "__main__":
    unittest.main()
