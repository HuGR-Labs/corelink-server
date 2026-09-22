import importlib.util
import io
import json
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
PROBE = (ROOT / "scripts/probe_i1656_capacity_readonly.py").read_text(encoding="utf-8")
WORKFLOW = (ROOT / ".github/workflows/issue-1656-capacity-refresh.yml").read_text(encoding="utf-8")
PROVIDER_ERROR_FIXTURES = ROOT / "tests/fixtures/issue-2044-capacity-schema/provider-error-payloads.json"
PROBE_SPEC = importlib.util.spec_from_file_location(
    "probe_i1656_capacity_readonly", ROOT / "scripts/probe_i1656_capacity_readonly.py"
)
assert PROBE_SPEC is not None and PROBE_SPEC.loader is not None
PROBE_MODULE = importlib.util.module_from_spec(PROBE_SPEC)
PROBE_SPEC.loader.exec_module(PROBE_MODULE)


class Issue1656CapacityRefreshContractTests(unittest.TestCase):
    def test_probe_has_fixed_get_only_endpoint_and_dedicated_token(self) -> None:
        self.assertIn('method="GET"', PROBE)
        self.assertIn("CANONICAL_ACCOUNT_ID", PROBE)
        self.assertIn("CLOUDFLARE_CAPACITY_READ_TOKEN", PROBE)
        self.assertNotIn("--account", PROBE)
        self.assertNotIn("--url", PROBE)
        self.assertNotIn("data=", PROBE)


    def test_workflow_is_protected_manual_main_only_and_redacted(self) -> None:
        self.assertIn("workflow_dispatch:", WORKFLOW)
        self.assertIn("run-1656-read-only", WORKFLOW)
        self.assertIn("production-capacity-read", WORKFLOW)
        self.assertIn("refs/heads/main", WORKFLOW)
        self.assertIn("CLOUDFLARE_CAPACITY_READ_TOKEN", WORKFLOW)
        self.assertIn("persist-credentials: false", WORKFLOW)
        self.assertNotIn("PATCH", WORKFLOW)
        self.assertNotIn("POST", WORKFLOW)
        self.assertNotIn("DELETE", WORKFLOW)

    def test_nonfinite_json_capacity_values_fail_closed(self) -> None:
        for encoded in ("NaN", "Infinity", "-Infinity"):
            with self.subTest(encoded=encoded):
                source = json.loads(f'{{"total_vcpu": {encoded}}}')
                with self.assertRaisesRegex(PROBE_MODULE.ProbeError, "not finite"):
                    PROBE_MODULE._number(source, ("total_vcpu",))

    def test_topology_contains_only_paths_and_json_types(self) -> None:
        payload = {
            "limits": {"total_vcpu": 1500, "enabled": True},
            "locations": [{"name": "secret-location-value"}],
            "nullable": None,
        }
        topology = PROBE_MODULE._key_type_topology(payload)
        self.assertEqual(
            topology,
            {
                "": "object",
                "/limits": "object",
                "/limits/enabled": "boolean",
                "/limits/total_vcpu": "number",
                "/locations": "array",
                "/nullable": "null",
            },
        )
        self.assertNotIn("secret-location-value", json.dumps(topology))

    def test_dynamic_keys_are_redacted_and_cannot_collide_with_pointer_paths(self) -> None:
        payload = {
            "result": {
                "a": {"b": 1},
                "a.b": "object-key-canary",
                "01234567890123456789": "numeric-key-canary",
                "token-canary-key": "secret-value-canary",
            }
        }
        topology = PROBE_MODULE._key_type_topology(payload)
        serialized = json.dumps(topology, sort_keys=True)
        self.assertEqual(topology["/result/a/b"], "number")
        self.assertEqual(topology["/result/~dynamic-key-1"], "string")
        self.assertEqual(topology["/result/~dynamic-key-2"], "string")
        self.assertEqual(topology["/result/~dynamic-key-3"], "string")
        for canary in ("a.b", "01234567890123456789", "token-canary-key", "object-key-canary", "numeric-key-canary", "secret-value-canary"):
            self.assertNotIn(canary, serialized)

    def test_run_topology_keeps_value_and_object_key_canaries_out_of_receipt_and_logs(self) -> None:
        payload = {
            "success": True,
            "errors": [],
            "result": {"limits": {"total_vcpu": 1500}, "credential-canary-key": "value-canary"},
        }
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "topology.json"
            logs = io.StringIO()
            with redirect_stdout(logs), redirect_stderr(logs), mock.patch.object(
                PROBE_MODULE, "_request", return_value=(payload, "ignored-response-hash")
            ):
                self.assertEqual(PROBE_MODULE.run_topology("token-canary", output), 0)
            recorded = output.read_text(encoding="utf-8")
        self.assertIn("/result/limits/total_vcpu", recorded)
        for canary in ("value-canary", "credential-canary-key", "token-canary", "ignored-response-hash"):
            self.assertNotIn(canary, recorded)
            self.assertNotIn(canary, logs.getvalue())

    def test_provider_error_fixtures_fail_closed(self) -> None:
        fixtures = json.loads(PROVIDER_ERROR_FIXTURES.read_text(encoding="utf-8"))
        for fixture in fixtures:
            with self.subTest(fixture=fixture["name"]):
                with self.assertRaises(PROBE_MODULE.ProbeError):
                    PROBE_MODULE._validate_provider_envelope(fixture["payload"])

    def test_topology_rejects_excessive_depth_and_nodes(self) -> None:
        value: object = {"child": {}}
        for _ in range(PROBE_MODULE.MAX_TOPOLOGY_DEPTH + 1):
            value = {"child": value}
        with self.assertRaisesRegex(PROBE_MODULE.ProbeError, "bounded depth"):
            PROBE_MODULE._key_type_topology(value)

        with self.assertRaisesRegex(PROBE_MODULE.ProbeError, "bounded node"):
            PROBE_MODULE._key_type_topology({str(index): index for index in range(PROBE_MODULE.MAX_TOPOLOGY_NODES)})
