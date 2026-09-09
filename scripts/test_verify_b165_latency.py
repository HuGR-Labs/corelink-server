#!/usr/bin/env python3
import pathlib
import tempfile
import unittest

import verify_b165_latency as verifier


def _rows(
    served: bool = False,
    served_path_a: str = "/v1/cas/tenant-a/digest",
    served_path_b: str = "/v1/cas/tenant-b/digest",
) -> str:
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
            ("served", "served_a", served_path_a, "200"),
            ("served", "served_b", served_path_b, "200"),
        ]
    for population, surface, path, status in required:
        for sample in range(1, 4):
            rows.append(f"{population}\t{surface}\t{path}\t{sample}\t{status}\t0\t0.010\n")
    return "".join(rows)


def _timing_rows() -> str:
    rows = []
    for surface, path in (
        ("served_a", "/v1/cas/tenant-a/digest"),
        ("served_b", "/v1/cas/tenant-b/digest"),
    ):
        for sample in range(1, 4):
            rows.append(f"{surface}\t{path}\t{sample}\t0.010000\t8.000\t2.000\n")
    return "".join(rows)


class B165VerifierTests(unittest.TestCase):
    PAT_A = "sha256:" + "a" * 64
    PAT_B = "sha256:" + "b" * 64

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
        report = verifier.verify(
            self.write(_rows(served=True)),
            3,
            require_served=True,
            tenant_a="tenant-a",
            tenant_b="tenant-b",
            pat_fingerprint_a=self.PAT_A,
            pat_fingerprint_b=self.PAT_B,
            server_timing_path=self.write(_timing_rows()),
        )
        self.assertEqual(report["status"], "complete")
        self.assertTrue(report["closure_allowed"])
        self.assertEqual(len(report["served"]), 2)
        self.assertEqual(report["served_credentials"]["served_a"]["pat_fingerprint"], self.PAT_A)
        self.assertEqual(report["served_credentials"]["served_b"]["pat_fingerprint"], self.PAT_B)
        self.assertEqual(report["server_timing"][0]["server_median_ms"], 8.0)

    def test_require_served_rejects_missing_server_timing(self):
        with self.assertRaisesRegex(verifier.EvidenceError, "server-timing"):
            verifier.verify(
                self.write(_rows(served=True)),
                3,
                require_served=True,
                tenant_a="tenant-a",
                tenant_b="tenant-b",
                pat_fingerprint_a=self.PAT_A,
                pat_fingerprint_b=self.PAT_B,
            )

    def test_server_timing_rejects_inconsistent_transport_residual(self):
        timing = _timing_rows().replace("\t8.000\t2.000", "\t8.000\t1.000", 1)
        with self.assertRaisesRegex(verifier.EvidenceError, "transport residual"):
            verifier.verify(
                self.write(_rows(served=True)),
                3,
                require_served=True,
                tenant_a="tenant-a",
                tenant_b="tenant-b",
                pat_fingerprint_a=self.PAT_A,
                pat_fingerprint_b=self.PAT_B,
                server_timing_path=self.write(timing),
            )

    def test_server_timing_rejects_nan(self):
        timing = _timing_rows().replace("\t8.000\t2.000", "\t8.000\tnan", 1)
        with self.assertRaisesRegex(verifier.EvidenceError, "non-finite"):
            verifier.verify(
                self.write(_rows(served=True)), 3, require_served=True,
                tenant_a="tenant-a", tenant_b="tenant-b",
                pat_fingerprint_a=self.PAT_A, pat_fingerprint_b=self.PAT_B,
                server_timing_path=self.write(timing),
            )

    def test_require_served_rejects_missing_tenant_bindings(self):
        with self.assertRaises(verifier.EvidenceError):
            verifier.verify(self.write(_rows(served=True)), 3, require_served=True)

    def test_served_rows_without_flag_never_close(self):
        report = verifier.verify(self.write(_rows(served=True)), 3)
        self.assertEqual(report["status"], "partial/open")
        self.assertFalse(report["closure_allowed"])

    def test_require_served_rejects_missing_pat_fingerprints(self):
        with self.assertRaises(verifier.EvidenceError):
            verifier.verify(
                self.write(_rows(served=True)),
                3,
                require_served=True,
                tenant_a="tenant-a",
                tenant_b="tenant-b",
            )

    def test_require_served_rejects_same_pat_fingerprint(self):
        with self.assertRaises(verifier.EvidenceError):
            verifier.verify(
                self.write(_rows(served=True)),
                3,
                require_served=True,
                tenant_a="tenant-a",
                tenant_b="tenant-b",
                pat_fingerprint_a=self.PAT_A,
                pat_fingerprint_b=self.PAT_A,
            )

    def test_require_served_rejects_malformed_pat_fingerprint(self):
        with self.assertRaises(verifier.EvidenceError):
            verifier.verify(
                self.write(_rows(served=True)),
                3,
                require_served=True,
                tenant_a="tenant-a",
                tenant_b="tenant-b",
                pat_fingerprint_a="redacted-a",
                pat_fingerprint_b=self.PAT_B,
            )

    def test_require_served_rejects_same_tenant(self):
        with self.assertRaises(verifier.EvidenceError):
            verifier.verify(
                self.write(_rows(served=True)),
                3,
                require_served=True,
                tenant_a="tenant-a",
                tenant_b="tenant-a",
                pat_fingerprint_a=self.PAT_A,
                pat_fingerprint_b=self.PAT_B,
            )

    def test_require_served_rejects_reused_path(self):
        with self.assertRaises(verifier.EvidenceError):
            verifier.verify(
                self.write(
                    _rows(
                        served=True,
                        served_path_a="/v1/cas/tenant-a/tenant-b",
                        served_path_b="/v1/cas/tenant-a/tenant-b",
                    )
                ),
                3,
                require_served=True,
                tenant_a="tenant-a",
                tenant_b="tenant-b",
                pat_fingerprint_a=self.PAT_A,
                pat_fingerprint_b=self.PAT_B,
            )

    def test_require_served_rejects_noncanonical_tenant_substring(self):
        with self.assertRaises(verifier.EvidenceError):
            verifier.verify(
                self.write(
                    _rows(served=True, served_path_a="/v1/cas/prefix-tenant-a/digest")
                ),
                3,
                require_served=True,
                tenant_a="tenant-a",
                tenant_b="tenant-b",
                pat_fingerprint_a=self.PAT_A,
                pat_fingerprint_b=self.PAT_B,
            )

    def test_canonical_path_shape_parity_table(self):
        # Keep this table in lockstep with probe-wp-b165-latency.sh:
        # exactly one object segment is accepted for each route family.
        cases = (
            ("/v1/cas/tenant-a/digest", "tenant-a"),
            ("/cargo/tenant-a/digest", "tenant-a"),
            ("/v1/cas/tenant-a/nested/object", None),
            ("/cargo/tenant-a/nested/object", None),
        )
        for path, expected in cases:
            if expected is None:
                with self.assertRaises(verifier.EvidenceError):
                    verifier._canonical_tenant(path)
            else:
                self.assertEqual(verifier._canonical_tenant(path), expected)

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
        self.assertIn("Surface unexpected probe or verifier result", workflow)
        self.assertIn("--server-timing-tsv", workflow)
        harness = (pathlib.Path(__file__).resolve().parent / "probe-wp-b165-latency.sh").read_text(encoding="utf-8")
        self.assertIn("B165_TIMING_OUT", harness)
        self.assertIn("curl_s * 1000 - server_ms", harness)

    def test_padding_policy_keeps_404_padded_and_401_unpadded(self):
        report = verifier.verify_padding_policy(pathlib.Path(__file__).resolve().parent.parent)
        self.assertEqual(report["401"], "not_padded")
        self.assertEqual(report["404"], "padded")
        self.assertEqual(report["decision"], verifier.PADDING_DECISION)

    def test_padding_policy_rejects_401_padding_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            for relative in (
                "worker/src/index_auth_stage.ts",
                "worker/src/index_finish_stage.ts",
                "worker/src/index_special_misc.ts",
            ):
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                source = pathlib.Path(__file__).resolve().parent.parent / relative
                target.write_text(source.read_text(encoding="utf-8"), encoding="utf-8")
            auth = root / "worker/src/index_auth_stage.ts"
            auth.write_text(auth.read_text(encoding="utf-8") + "\napplyTimingPad();\n", encoding="utf-8")
            with self.assertRaisesRegex(verifier.EvidenceError, "401 authentication stage"):
                verifier.verify_padding_policy(root)

    def test_padding_policy_rejects_removed_404_boundary(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            for relative in (
                "worker/src/index_auth_stage.ts",
                "worker/src/index_finish_stage.ts",
                "worker/src/index_special_misc.ts",
            ):
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                source = pathlib.Path(__file__).resolve().parent.parent / relative
                target.write_text(source.read_text(encoding="utf-8"), encoding="utf-8")
            finish = root / "worker/src/index_finish_stage.ts"
            finish.write_text(
                finish.read_text(encoding="utf-8").replace(
                    "if (doResponse.status === 404)", "if (doResponse.status === 418)"
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(verifier.EvidenceError, "404"):
                verifier.verify_padding_policy(root)

    def test_padding_policy_rejects_commented_out_404_call(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            for relative in (
                "worker/src/index_auth_stage.ts", "worker/src/index_finish_stage.ts",
                "worker/src/index_special_misc.ts",
            ):
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                source = pathlib.Path(__file__).resolve().parent.parent / relative
                target.write_text(source.read_text(encoding="utf-8"), encoding="utf-8")
            finish = root / "worker/src/index_finish_stage.ts"
            finish.write_text(
                finish.read_text(encoding="utf-8").replace(
                    "      await applyTimingPad(", "      // await applyTimingPad("
                ), encoding="utf-8",
            )
            with self.assertRaisesRegex(verifier.EvidenceError, "404"):
                verifier.verify_padding_policy(root)

    def test_padding_policy_rejects_aliased_auth_padding(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            for relative in (
                "worker/src/index_auth_stage.ts", "worker/src/index_finish_stage.ts",
                "worker/src/index_special_misc.ts",
            ):
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                source = pathlib.Path(__file__).resolve().parent.parent / relative
                target.write_text(source.read_text(encoding="utf-8"), encoding="utf-8")
            auth = root / "worker/src/index_auth_stage.ts"
            auth.write_text(
                auth.read_text(encoding="utf-8")
                + '\nimport { applyTimingPad as p } from "./index_auth_timing.js";\np();\n',
                encoding="utf-8",
            )
            with self.assertRaisesRegex(verifier.EvidenceError, "401 authentication stage"):
                verifier.verify_padding_policy(root)


if __name__ == "__main__":
    unittest.main()
