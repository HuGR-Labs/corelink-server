"""Credentialless contract tests for the B-103 hosted reproducer."""

from __future__ import annotations

import pathlib
import subprocess
import sys
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]


class B103ContractTests(unittest.TestCase):
    def test_load_is_bounded_to_220_puts_and_webdav_to_64(self) -> None:
        source = (ROOT / "scripts/reproduce_b103_cargo_write.py").read_text(encoding="utf-8")
        self.assertIn("CONCURRENCIES = (4, 16, 64, 220)", source)
        self.assertIn('"put_only_max_concurrency": 220', source)
        self.assertIn('"webdav_max_concurrency": 64', source)
        self.assertIn("levels = CONCURRENCIES if mode == \"put_only\" else CONCURRENCIES[:-1]", source)
        self.assertIn("threading.Barrier(concurrency + 1)", source)
        self.assertIn('"comparison": "put_only_64_rps_gt_put_only_4_rps"', source)

    def test_receipt_redacts_tenant_identity(self) -> None:
        source = (ROOT / "scripts/reproduce_b103_cargo_write.py").read_text(encoding="utf-8")
        self.assertIn('"tenant_id": "redacted"', source)
        self.assertIn('"tenant_id_sha256": sha256(args.tenant.encode("ascii"))', source)

    def test_hosted_contract_captures_same_window_unfiltered_tail(self) -> None:
        workflow = (ROOT / ".github/workflows/b103-cargo-write-reproducer.yml").read_text(encoding="utf-8")
        self.assertIn("pull_request:", workflow)
        self.assertIn("runs-on: ubuntu-24.04", workflow)
        self.assertIn("wrangler tail corelink --format json", workflow)
        self.assertNotIn("wrangler tail corelink --format json --search", workflow)
        self.assertIn("scripts/redact_b103_worker_tail.py", workflow)
        self.assertIn("b103-cargo-write-worker-tail.redacted.jsonl", workflow)
        self.assertIn("npm exec --yes --package=wrangler@4.111.0", workflow)

    def test_unfiltered_tail_redactor_masks_sensitive_values_and_preserves_event(self) -> None:
        tenant = "11111111-1111-4111-8111-111111111111"
        sample = (
            '{"event":"request","tenant_id":"' + tenant + '",'
            '"message":"Bearer abc123 contact owner@example.com",'
            '"request_id":"req-7"}\n'
        )
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/redact_b103_worker_tail.py"), "--tenant", tenant],
            input=sample,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('"event":"request"', result.stdout)
        self.assertIn('"request_id":"req-7"', result.stdout)
        self.assertNotIn(tenant, result.stdout)
        self.assertNotIn("abc123", result.stdout)
        self.assertNotIn("owner@example.com", result.stdout)
        self.assertIn("[REDACTED]", result.stdout)

    def test_unfiltered_tail_redactor_fails_on_non_json_or_empty_capture(self) -> None:
        redactor = str(ROOT / "scripts/redact_b103_worker_tail.py")
        for sample in ("not json\n", "\n"):
            result = subprocess.run(
                [sys.executable, redactor], input=sample, text=True, capture_output=True, check=False
            )
            self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
