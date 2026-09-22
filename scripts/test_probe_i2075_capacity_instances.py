import importlib.util
import io
import json
import subprocess
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
    def test_fixed_cli_reads_five_allowlisted_apps_and_retains_aggregate_counts(self) -> None:
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
                receipt = PROBE.capture("0123456789abcdef0123456789abcdef", "a" * 40, output)
            artifact = output.read_text(encoding="utf-8")

        self.assertEqual(calls, [["containers", "list", "--json"]] + [
            ["containers", "instances", f"00000000-0000-0000-0000-{index:012d}", "--json"]
            for index in range(1, len(PROBE.APP_NAMES) + 1)
        ])
        instances = receipt["observed_production_instances"]
        self.assertEqual(instances["running_instances"], len(PROBE.APP_NAMES))
        self.assertEqual(len(instances["instance_states_by_application_in_allowlist_order"]), 5)
        self.assertEqual(
            instances["running_instance_resource_allocation_estimate"]["vcpu"],
            len(PROBE.APP_NAMES) * PROBE.INSTANCE_SHAPE["vcpu"],
        )
        self.assertEqual(receipt["account_limits"]["vcpu"], 1500)
        self.assertEqual(receipt["account_limits"]["memory_tib"], 6)
        self.assertEqual(receipt["account_limits"]["disk_tb"], 30)
        self.assertFalse(receipt["account_limits"]["account_specific_entitlement_verified"])
        self.assertEqual(receipt["configured_production_ceiling"]["max_instances_total"], 1000)
        self.assertEqual(receipt["configured_production_ceiling"]["reserved_vcpu"], 250)
        self.assertEqual(receipt["configured_production_ceiling"]["reserved_memory_gib"], 1000)
        self.assertEqual(receipt["configured_production_ceiling"]["reserved_disk_gb"], 4000)
        self.assertIsNone(receipt["concurrency"]["active_tenant_region_assignments"])
        self.assertIsNone(receipt["concurrency"]["deduplicated_tenant_count"])
        self.assertEqual(receipt["concurrency"]["status"], "unavailable")
        for canary in (
            "tenant_42",
            "acct123",
            "customer_name",
            "private-image-canary",
            "instance-canary",
            "inactive-canary",
            "0123456789abcdef0123456789abcdef",
            "00000000-0000-0000-0000-",
            "11111111-1111-1111-1111-111111111111",
        ):
            self.assertNotIn(canary, artifact)
        for application_name in PROBE.APP_NAMES:
            self.assertNotIn(application_name, artifact)

    def test_unknown_state_and_more_than_declared_instances_fail_closed(self) -> None:
        with self.assertRaisesRegex(PROBE.ProbeError, "unknown"):
            PROBE._state_counts([{"id": "id-canary", "state": "future-provider-state"}])
        with self.assertRaisesRegex(PROBE.ProbeError, "declared application ceiling"):
            PROBE._state_counts([{"state": "running"}] * (PROBE.MAX_INSTANCES_PER_APP + 1))

    def test_oversized_stdout_is_stopped_at_the_stream_limit(self) -> None:
        real_popen = subprocess.Popen
        commands: list[list[str]] = []

        def oversized_child(command: list[str], **kwargs: object) -> subprocess.Popen[bytes]:
            commands.append(command)
            return real_popen(
                [sys.executable, "-c", "import sys; sys.stdout.buffer.write(b'x' * 1100000)"],
                **kwargs,
            )

        with mock.patch.object(PROBE.subprocess, "Popen", side_effect=oversized_child):
            with self.assertRaisesRegex(PROBE.ProbeError, "bounded size"):
                PROBE._run_wrangler(["containers", "list", "--json"])
        self.assertEqual(commands, [["pnpm", "exec", "wrangler", "containers", "list", "--json"]])

    def test_missing_or_duplicate_allowlisted_application_fails_closed(self) -> None:
        rows = application_rows()
        with self.assertRaisesRegex(PROBE.ProbeError, "did not resolve exactly"):
            PROBE._application_ids(rows[:-2])
        with self.assertRaisesRegex(PROBE.ProbeError, "duplicate"):
            PROBE._application_ids(rows + [rows[0]])

    def test_provider_command_failure_is_not_echoed_to_logs(self) -> None:
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
            "containers instances <APPLICATION_ID> --json",
            "persist-credentials: false",
            "secrets.CF_ACCOUNT_ID",
            "issue-2075-capacity-read-only-receipt.json",
        ):
            self.assertIn(expected, WORKFLOW)
        for forbidden in ("pull_request:", "push:", "schedule:", "tee ", "POST", "PATCH", "DELETE"):
            self.assertNotIn(forbidden, WORKFLOW)


if __name__ == "__main__":
    unittest.main()
