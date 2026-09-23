#!/usr/bin/env python3
"""Hosted behavior and adversarial checks for the B-103 evidence lane.

These tests never import a live client or dispatch the staging workflow.
They exercise only the serialized artifact and tail-redaction boundaries.
"""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TENANT = "123e4567-e89b-42d3-a456-426614174000"
TOKEN = "corelink_pat_super_secret"
API_KEY = "nested-api-secret"
OPERATION = "b103-0123456789abcdef0123456789abcdef"


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


REDactor = load_module("redact_b103", ROOT / "scripts/redact_b103_worker_tail.py")
REPRODUCER = load_module("reproduce_b103", ROOT / "scripts/reproduce_b103_cargo_write.py")
RECEIPT = load_module("load_receipt", ROOT / "scripts/validate_load_target_receipt.py")


class B103CargoWriteContractTest(unittest.TestCase):
    def test_adversarial_nested_tail_values_are_redacted_but_operation_is_preserved(self) -> None:
        event = {
            "request": {
                "tenant_id": TENANT,
                "headers": {
                    "Authorization": f"Bearer {TOKEN}",
                    "X-Api-Key": API_KEY,
                    "X-Corelink-Operation": OPERATION,
                },
            },
            "nested": [
                {"identity": {"customer-id": TENANT, "email": "owner@example.com"}},
                {
                    "message": f"tenant={TENANT} token={TOKEN} ip=203.0.113.7 op={OPERATION}",
                    "opaque": API_KEY,
                },
            ],
        }
        safe = REDactor.redact(event, (TENANT, TOKEN, API_KEY))
        rendered = json.dumps(safe, sort_keys=True)
        for forbidden in (TENANT, TOKEN, API_KEY, "owner@example.com", "203.0.113.7"):
            self.assertNotIn(forbidden, rendered)
        self.assertIn(OPERATION, rendered)
        self.assertEqual(REDactor.operation_ids(safe), {OPERATION})

    def test_adversarial_tail_requires_every_response_operation_for_correlation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = root / "wire.json"
            evidence.write_text(json.dumps({"responses": [{"operation_id": OPERATION}]}), encoding="utf-8")
            env = dict(os.environ)
            accepted = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/redact_b103_worker_tail.py"),
                    "--require-operation-ids-from",
                    str(evidence),
                ],
                input=json.dumps({
                    "event": {"request": {"method": "PUT", "headers": {
                        "X-Corelink-Operation": OPERATION,
                        "authorization": f"Bearer {TOKEN}",
                        "tenant_id": TENANT,
                    }}},
                    "outcome": "ok",
                    "eventTimestamp": 123,
                    "logs": ["must be discarded"],
                }) + "\n",
                text=True,
                capture_output=True,
                env=env,
                check=False,
            )
            self.assertEqual(accepted.returncode, 0, accepted.stderr)
            self.assertIn(OPERATION, accepted.stdout)
            self.assertNotIn(TENANT, accepted.stdout)
            self.assertNotIn("authorization", accepted.stdout)
            self.assertNotIn("logs", accepted.stdout)

            rejected = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/redact_b103_worker_tail.py"),
                    "--require-operation-ids-from",
                    str(evidence),
                ],
                input=json.dumps({
                    "event": {"request": {"method": "PUT", "headers": {
                        "X-Corelink-Operation": "b103-ffffffffffffffffffffffffffffffff"
                    }}},
                    "outcome": "ok",
                }) + "\n",
                text=True,
                capture_output=True,
                env=env,
                check=False,
            )
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn("missing 1 B-103 operation IDs", rejected.stderr)

    def test_wire_response_keeps_a_non_secret_operation_id_and_redacts_tenant(self) -> None:
        original = REPRODUCER.urlopen

        class Response:
            status = 200
            headers = {"X-Request-Id": "response-7"}

            def read(self) -> bytes:
                return b""

            def __enter__(self):
                return self

            def __exit__(self, *_args):
                return False

        captured = []

        def fake_urlopen(request, timeout):
            captured.append((request, timeout))
            return Response()

        REPRODUCER.urlopen = fake_urlopen
        try:
            row = REPRODUCER.request("https://staging.corelink.humangr.com", TENANT, TOKEN, "PUT", "a" * 64, b"payload")
        finally:
            REPRODUCER.urlopen = original
        self.assertRegex(str(row["operation_id"]), r"^b103-[0-9a-f]{32}$")
        self.assertEqual(captured[0][0].get_header("X-corelink-operation"), row["operation_id"])
        self.assertNotEqual(row["operation_id"], TOKEN)

        safe_receipt = RECEIPT.artifact_receipt(
            {
                "schema": 1,
                "environment": "staging",
                "target": "https://staging.corelink.humangr.com",
                "tenant_id": TENANT,
                "deployment_sha": "a" * 40,
                "issued_at": "2030-01-01T00:00:00Z",
                "expires_at": "2030-01-01T01:00:00Z",
            },
            redact_tenant=True,
        )
        self.assertEqual(safe_receipt["tenant_id"], "[REDACTED]")
        self.assertNotIn(TENANT, json.dumps(safe_receipt, sort_keys=True))

    def test_put_only_arm_releases_220_independent_operations_together(self) -> None:
        original = REPRODUCER.urlopen
        captured = []
        lock = threading.Lock()

        class Response:
            status = 200
            headers = {}

            def read(self) -> bytes:
                return b""

            def __enter__(self):
                return self

            def __exit__(self, *_args):
                return False

        def fake_urlopen(request, timeout):
            with lock:
                captured.append((request, timeout, threading.get_ident()))
            return Response()

        REPRODUCER.urlopen = fake_urlopen
        try:
            arm = REPRODUCER.run_arm("https://example.test", TENANT, TOKEN, "put_only", 220)
        finally:
            REPRODUCER.urlopen = original
        self.assertEqual(arm["concurrency"], 220)
        self.assertEqual(arm["operations"], 220)
        self.assertEqual(arm["requests"], 220)
        self.assertEqual(arm["failed_requests"], 0)
        self.assertEqual(arm["harness_errors"], [])
        self.assertEqual(len(captured), 220)
        self.assertEqual(len({request.full_url for request, _timeout, _thread in captured}), 220)
        self.assertEqual(len({request.data for request, _timeout, _thread in captured}), 220)
        self.assertEqual(
            len({request.get_header("X-corelink-operation") for request, _timeout, _thread in captured}), 220
        )
        self.assertEqual(len({_thread for _request, _timeout, _thread in captured}), 220)

    def test_workflow_uses_topology_root_worker_and_never_uploads_raw_tail(self) -> None:
        workflow = (ROOT / ".github/workflows/b103-cargo-write-reproducer.yml").read_text(encoding="utf-8")
        topology = json.loads((ROOT / "infra/staging/topology.json").read_text(encoding="utf-8"))
        worker = topology["cloudflare"]["root_worker"]
        self.assertIn("pull_request:", workflow)
        self.assertIn("python3 -m unittest -v scripts.test_b103_cargo_write_contract", workflow)
        self.assertIn("github.ref == 'refs/heads/main'", workflow)
        self.assertIn("github.ref_protected", workflow)
        self.assertIn(f'B103_TAIL_WORKER: "{worker}"', workflow)
        self.assertIn('wrangler tail "${B103_TAIL_WORKER}" --format json', workflow)
        self.assertNotIn("--search", workflow)
        self.assertIn("--redact-tenant", workflow)
        self.assertIn("--require-operation-ids-from", workflow)
        self.assertIn("b103-cargo-write-worker-tail.redacted.jsonl", workflow)
        self.assertNotIn("worker-tail.raw.jsonl\n", workflow.split("path: |", 1)[1])
        self.assertEqual(REPRODUCER.CONCURRENCIES, (4, 16, 64, 220))


if __name__ == "__main__":
    unittest.main(verbosity=2)
