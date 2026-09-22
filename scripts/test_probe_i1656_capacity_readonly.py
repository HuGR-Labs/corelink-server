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
PROBE = (ROOT / "scripts/probe_i1656_capacity_readonly.py").read_text(encoding="utf-8")
WORKFLOW = (ROOT / ".github/workflows/issue-1656-capacity-refresh.yml").read_text(encoding="utf-8")
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

    def test_workflow_is_protected_manual_main_only_and_logs_are_bounded(self) -> None:
        self.assertIn("workflow_dispatch:", WORKFLOW)
        self.assertIn("run-1656-read-only", WORKFLOW)
        self.assertIn("production-capacity-read", WORKFLOW)
        self.assertIn("refs/heads/main", WORKFLOW)
        self.assertIn("CLOUDFLARE_CAPACITY_READ_TOKEN", WORKFLOW)
        self.assertIn("persist-credentials: false", WORKFLOW)
        self.assertIn("issue-2044-capacity-schema", WORKFLOW)
        self.assertNotIn("PATCH", WORKFLOW)
        self.assertNotIn("POST", WORKFLOW)
        self.assertNotIn("DELETE", WORKFLOW)

    def test_only_fixed_path_type_predicates_are_returned(self) -> None:
        payload = {
            "success": True,
            "errors": [],
            "total_vcpu": 1500,
            "vcpu_per_deployment": 4,
            "total_memory_mib": 6291456,
            "usage": None,
        }
        predicates = PROBE_MODULE._schema_projection(payload)
        self.assertEqual(predicates["api_success"], "boolean")
        self.assertEqual(predicates["api_errors"], "array")
        self.assertEqual(predicates["account_total_vcpu"], "number")
        self.assertEqual(predicates["account_usage"], "null")
        self.assertEqual(predicates["result_total_vcpu"], "missing")
        self.assertEqual(set(predicates), {label for label, _path in PROBE_MODULE.SCHEMA_PREDICATES})
        self.assertNotIn("1500", json.dumps(predicates))
        self.assertNotIn("true", json.dumps(predicates))

    def test_artifact_and_log_exclude_provider_key_and_value_canaries(self) -> None:
        payload = {
            "success": True,
            "errors": [],
            "result": {
                "total_vcpu": 1500,
                "vcpu_per_deployment": 4,
                "total_memory_mib": 6291456,
                "usage": None,
            },
            "tenant_42": {"acct123": {"customer_name": "customer_name-value-canary"}},
        }
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "receipt.json"
            stdout, stderr = io.StringIO(), io.StringIO()
            with (
                mock.patch.object(PROBE_MODULE, "_request", return_value=payload),
                mock.patch.object(sys, "argv", ["probe", "--topology-output", str(output)]),
                redirect_stdout(stdout),
                redirect_stderr(stderr),
            ):
                self.assertEqual(PROBE_MODULE.main(), 0)
            artifact = output.read_text(encoding="utf-8")
            receipt = json.loads(artifact)
            log = stdout.getvalue() + stderr.getvalue()

        for canary in ("tenant_42", "acct123", "customer_name", "customer_name-value-canary", "1500", "6291456"):
            self.assertNotIn(canary, artifact)
            self.assertNotIn(canary, log)
        self.assertEqual(receipt["capacity_schema_predicates"]["api_success"], "boolean")
        self.assertEqual(receipt["capacity_schema_predicates"]["api_errors"], "array")
        self.assertEqual(receipt["capacity_schema_predicates"]["result_total_vcpu"], "number")
        self.assertEqual(
            receipt["shape_counts"],
            {"max_depth": 3, "node_count": 11, "unknown_object_key_count": 3},
        )

    def test_envelope_requires_success_true_and_empty_errors_without_echoing(self) -> None:
        for payload in (
            {"success": False, "errors": [{"message": "customer_name-value-canary"}]},
            {"success": True, "errors": [{"message": "customer_name-value-canary"}]},
            {"success": True},
        ):
            with self.subTest(payload=payload):
                with self.assertRaises(PROBE_MODULE.ProbeError) as raised:
                    PROBE_MODULE._validate_provider_envelope(payload)
                self.assertNotIn("customer_name-value-canary", str(raised.exception))

    def test_depth_and_node_limits_fail_closed_without_names(self) -> None:
        too_deep: object = "leaf"
        for _ in range(PROBE_MODULE.MAX_SCHEMA_DEPTH + 1):
            too_deep = {"tenant_42": too_deep}
        with self.assertRaisesRegex(PROBE_MODULE.ProbeError, "bounded depth"):
            PROBE_MODULE._bounded_shape_counts({"success": True, "errors": [], "opaque": too_deep})

        too_many_nodes = {"success": True, "errors": [], "opaque": list(range(PROBE_MODULE.MAX_SCHEMA_NODES))}
        with self.assertRaisesRegex(PROBE_MODULE.ProbeError, "bounded node"):
            PROBE_MODULE._bounded_shape_counts(too_many_nodes)

    def test_nonfinite_values_are_not_serialized(self) -> None:
        payload = json.loads('{"success": true, "errors": [], "total_vcpu": NaN}')
        predicates = PROBE_MODULE._schema_projection(payload)
        self.assertEqual(predicates["account_total_vcpu"], "number")
        self.assertNotIn("NaN", json.dumps(predicates))


if __name__ == "__main__":
    unittest.main()
