#!/usr/bin/env python3
"""Hermetic adversarial tests for the B-062 live rollout gate."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
VERIFY = ROOT / "scripts" / "verify-b062-prod-pins.sh"


FAKE_CURL = r'''#!/usr/bin/env python3
import json
import os
import sys

url = sys.argv[-1]
trace = os.environ.get("B062_TEST_TRACE")
if trace:
    with open(trace, "a", encoding="utf-8") as handle:
        handle.write(url + "\n")

if url.endswith("/_health/container"):
    if os.environ.get("B062_TEST_DEEP_TRANSPORT") == "1":
        raise SystemExit(7)
    sys.stdout.write(os.environ.get("B062_TEST_DEEP_STATUS", "200"))
    raise SystemExit(0)

apps = {
    "a033572c-0803-4866-b3a3-61f4812843b1": ("corelink-prod-corelinkserver-prod", "corelink-prod-corelinkserver-prod"),
    "a0337243-13cb-46ef-adb1-781294294404": ("corelink-prod-sam-corelinkserver-prod-sam", "corelink-prod-sam-corelinkserver-prod"),
    "a03faa1b-70c4-40eb-aa38-9f70d43de992": ("corelink-prod-lhr-corelinkserver-prod-lhr", "corelink-prod-lhr-corelinkserver-prod"),
    "a033417c-db96-467b-a65c-83951d1fa5d1": ("corelink-prod-nrt-corelinkserver-prod-nrt", "corelink-prod-nrt-corelinkserver-prod"),
    "a030dd8d-c24c-41bb-bf03-a1fb3542eac0": ("corelink-prod-syd-corelinkserver-prod-syd", "corelink-prod-syd-corelinkserver-prod"),
}
app_id = url.rsplit("/", 1)[-1]
name, image_name = apps[app_id]
health = {"healthy": 1, "active": int(os.environ.get("B062_TEST_ACTIVE", "1")), "failed": 0}
if os.environ.get("B062_TEST_MISSING_ACTIVE") == "1":
    health.pop("active")
print(json.dumps({
    "success": True,
    "result": {
        "name": name,
        "configuration": {"image": "registry.cloudflare.com/6a1fc1c626fc2628823e60b9db01f5cd/" + image_name + ":b90b245df-r1"},
        "health": {"instances": health},
        "instances": 1,
        "version": 1,
        "updated_at": "2026-09-09T00:00:00Z",
    },
}))
'''


class B062ProdPinsTests(unittest.TestCase):
    def run_gate(self, **overrides: str) -> tuple[subprocess.CompletedProcess[str], list[str]]:
        with tempfile.TemporaryDirectory() as td:
            temp = Path(td)
            curl = temp / "curl"
            curl.write_text(FAKE_CURL, encoding="utf-8")
            curl.chmod(0o755)
            trace = temp / "trace"
            env = os.environ.copy()
            env.update(
                {
                    "PATH": f"{temp}:{env['PATH']}",
                    "CLOUDFLARE_ACCOUNT_ID": "test-account",
                    "CLOUDFLARE_CONTAINERS_API_TOKEN": "test-token",
                    "MAIN_REF": "64e57a2ccb218f475b44a260b64af95e0bc7df2c",
                    "DEEP_HEALTH_TIMEOUT_SECONDS": "1",
                    "B062_TEST_TRACE": str(trace),
                }
            )
            env.update(overrides)
            result = subprocess.run(
                ["bash", str(VERIFY)], cwd=ROOT, env=env, text=True,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
            )
            calls = trace.read_text(encoding="utf-8").splitlines() if trace.exists() else []
            return result, calls

    def test_all_five_active_and_deep_healthy_pass(self) -> None:
        result, calls = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("PASS: all five live applications", result.stdout)
        self.assertEqual(sum(url.endswith("/_health/container") for url in calls), 5)
        self.assertEqual(len(calls), 10)

    def test_reserved_pool_healthy_but_zero_active_fails(self) -> None:
        result, _ = self.run_gate(B062_TEST_ACTIVE="0")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("healthy=1 active=0 failed=0 desired=1", result.stdout)

    def test_missing_active_fails_closed(self) -> None:
        result, _ = self.run_gate(B062_TEST_MISSING_ACTIVE="1")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("active=0", result.stdout)

    def test_deep_health_503_fails(self) -> None:
        result, _ = self.run_gate(B062_TEST_DEEP_STATUS="503")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("deep health returned HTTP 503", result.stdout)

    def test_deep_health_transport_failure_fails(self) -> None:
        result, _ = self.run_gate(B062_TEST_DEEP_TRANSPORT="1")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("deep health GET transport failed", result.stdout)

    def test_timeout_is_bounded_before_network(self) -> None:
        result, calls = self.run_gate(DEEP_HEALTH_TIMEOUT_SECONDS="121")
        self.assertEqual(result.returncode, 2, result.stdout)
        self.assertIn("must be between 1 and 120", result.stdout)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
