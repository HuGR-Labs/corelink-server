#!/usr/bin/env python3
import pathlib
import tempfile
import unittest

import verify_b165_latency as verifier


def _rows(served: bool = False) -> str:
    rows = []
    required = [
        ("refusal", "v1", "/v1/cas/x/y", "401"),
        ("refusal", "npm", "/npm/x", "401"),
        ("refusal", "pip", "/pip/simple/x", "401"),
        ("refusal", "v2", "/v2/x/manifests/latest", "401"),
        ("control", "health", "/health", "200"),
    ]
    if served:
        required += [
            ("served", "served_a", "/v1/cas/tenant-a/digest", "200"),
            ("served", "served_b", "/v1/cas/tenant-b/digest", "200"),
        ]
    for population, surface, path, status in required:
        for sample in range(1, 4):
            rows.append(f"{population}\t{surface}\t{path}\t{sample}\t{status}\t0\t0.010\n")
    return "".join(rows)


class B165VerifierTests(unittest.TestCase):
    def write(self, content: str) -> pathlib.Path:
        handle = tempfile.NamedTemporaryFile("w", encoding="utf-8", delete=False)
        handle.write(content)
        handle.close()
        self.addCleanup(pathlib.Path(handle.name).unlink)
        return pathlib.Path(handle.name)

    def test_refusal_only_is_explicitly_partial_and_non_closing(self):
        report = verifier.verify(self.write(_rows()), 3)
        self.assertEqual(report["status"], "partial/open")
        self.assertFalse(report["closure_allowed"])

    def test_require_served_rejects_refusal_only(self):
        with self.assertRaises(verifier.EvidenceError):
            verifier.verify(self.write(_rows()), 3, require_served=True)

    def test_complete_requires_both_served_populations(self):
        report = verifier.verify(self.write(_rows(served=True)), 3, require_served=True)
        self.assertEqual(report["status"], "complete")
        self.assertTrue(report["closure_allowed"])
        self.assertEqual(len(report["served"]), 2)

    def test_missing_sample_fails_closed(self):
        content = _rows().replace("refusal\tv2\t/v2/x/manifests/latest\t3\t401\t0\t0.010\n", "")
        with self.assertRaises(verifier.EvidenceError):
            verifier.verify(self.write(content), 3)

    def test_monitor_is_bounded_and_keeps_partial_result_open(self):
        workflow = (pathlib.Path(__file__).resolve().parent.parent / ".github/workflows/b165-latency-monitor.yml").read_text(encoding="utf-8")
        self.assertIn("cron: '17 */6 * * *'", workflow)
        self.assertIn("timeout-minutes: 10", workflow)
        self.assertIn("B165_MODE=refusal", workflow)
        self.assertIn("--samples 10", workflow)
        self.assertIn("retention-days: 30", workflow)
        self.assertIn("Exit 2 is the expected partial/open result", workflow)


if __name__ == "__main__":
    unittest.main()
