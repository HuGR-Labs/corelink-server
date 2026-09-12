from __future__ import annotations

import copy
import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/collect_b071_gc_observation.py"
SPEC = importlib.util.spec_from_file_location("collect_b071", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
collector = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(collector)

TENANT = "ee30f7ba-fc25-4d71-939e-ebe130b4c6a3"
RUN = "11111111-2222-4333-8444-555555555555"
SCOPE = {
    "tenant_id": TENANT,
    "region": "sam",
    "run_id": RUN,
    "bucket": "corelink-cas-prod-sam",
}


def report(**updates: object) -> str:
    value: dict[str, object] = {
        "schema_version": 1,
        "mode": "dry_run",
        "observation_only": True,
        "run_id": RUN,
        "tenant_id": TENANT,
        "region": "sam",
        "candidates_scanned": 4,
        "reclaimable_count": 1,
        "reclaimable_bytes": 4096,
        "delete_count": 0,
        "deleted_bytes": 0,
        "skipped_grace_pending": 1,
        "skipped_refcount_non_zero": 1,
        "already_resolved": 1,
        "duration_ms": 20,
        "observed_at_ms": 1_800_000_000_000,
        "phase_budget_ms": 1_800_000,
        "max_candidates": 250,
    }
    value.update(updates)
    return "diagnostic\n" + collector.REPORT_PREFIX + json.dumps(value) + "\n"


class ScopeManifestTests(unittest.TestCase):
    def write_manifest(self, directory: str, scopes: list[dict[str, str]]) -> Path:
        path = Path(directory) / "scopes.json"
        path.write_text(
            json.dumps({"schema_version": 1, "scopes": scopes}), encoding="utf-8"
        )
        return path

    def test_manifest_requires_unique_explicit_provisioned_scopes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self.write_manifest(directory, [SCOPE])
            self.assertEqual(collector.load_scopes(path), [SCOPE])

            path = self.write_manifest(
                directory,
                [SCOPE, {**SCOPE, "run_id": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"}],
            )
            with self.assertRaisesRegex(
                collector.ObservationError, "duplicate tenant/region"
            ):
                collector.load_scopes(path)

            path = self.write_manifest(directory, [{**SCOPE, "region": "syd"}])
            with self.assertRaisesRegex(
                collector.ObservationError, "region must be one of"
            ):
                collector.load_scopes(path)

    def test_manifest_rejects_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            target = self.write_manifest(directory, [SCOPE])
            link = Path(directory) / "link.json"
            link.symlink_to(target)
            with self.assertRaisesRegex(collector.ObservationError, "non-symlink"):
                collector.load_scopes(link)


class ReportTests(unittest.TestCase):
    def test_report_is_exactly_scoped_bounded_balanced_and_zero_delete(self) -> None:
        parsed = collector._parse_report(report(), SCOPE)
        self.assertEqual(parsed["reclaimable_bytes"], 4096)

        mutations = (
            {"tenant_id": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"},
            {"region": "iad"},
            {"run_id": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"},
            {"mode": "live_delete"},
            {"schema_version": 2},
            {"observation_only": False},
            {"delete_count": 1},
            {"candidates_scanned": 251},
            {"already_resolved": 0},
            {"duration_ms": 1_800_001},
        )
        for mutation in mutations:
            with (
                self.subTest(mutation=mutation),
                self.assertRaises(collector.ObservationError),
            ):
                collector._parse_report(report(**mutation), SCOPE)

        with self.assertRaisesRegex(collector.ObservationError, "unexpected schema"):
            collector._parse_report(report(unexpected=True), SCOPE)

    def test_report_rejects_absent_or_duplicated_structured_record(self) -> None:
        with self.assertRaisesRegex(collector.ObservationError, "exactly one"):
            collector._parse_report("diagnostic only\n", SCOPE)
        with self.assertRaisesRegex(collector.ObservationError, "exactly one"):
            collector._parse_report(report() + report(), SCOPE)


class CollectionTests(unittest.TestCase):
    @mock.patch.dict(
        os.environ,
        {
            "CLOUDFLARE_ACCOUNT_ID": "account",
            "D1_DATABASE_ID": "database",
            "CF_API_TOKEN": "read-token",
            "R2_TDK_HEX": "00" * 32,
            "GC_LIVE_DELETE_CONFIRM": "I_UNDERSTAND",
            "R2_S3_SECRET_ACCESS_KEY": "must-not-cross-boundary",
        },
        clear=True,
    )
    def test_child_env_strips_destructive_controls_if_allowlist_expands(self) -> None:
        mutated_passthrough = collector.PASSTHROUGH_ENV + (
            "GC_LIVE_DELETE_CONFIRM",
            "R2_S3_SECRET_ACCESS_KEY",
        )
        with mock.patch.object(collector, "PASSTHROUGH_ENV", mutated_passthrough):
            child_env = collector._child_env(SCOPE)
        collector._validate_child_env(child_env)
        self.assertNotIn("GC_LIVE_DELETE_CONFIRM", child_env)
        self.assertNotIn("R2_S3_SECRET_ACCESS_KEY", child_env)

    @mock.patch.dict(
        os.environ,
        {
            "CLOUDFLARE_ACCOUNT_ID": "account",
            "D1_DATABASE_ID": "database",
            "CF_API_TOKEN": "read-token",
            "R2_TDK_HEX": "00" * 32,
        },
        clear=True,
    )
    @mock.patch.object(collector.subprocess, "run")
    def test_collect_rejects_confirmation_reintroduced_at_child_boundary(
        self, run: mock.Mock
    ) -> None:
        original = collector._child_env

        def compromised(scope: dict[str, str]) -> dict[str, str]:
            child_env = original(scope)
            child_env["GC_LIVE_DELETE_CONFIRM"] = "I_UNDERSTAND"
            return child_env

        with mock.patch.object(collector, "_child_env", side_effect=compromised):
            with self.assertRaisesRegex(
                collector.ObservationError, "destructive environment crossed"
            ):
                collector.collect(
                    scopes=[SCOPE],
                    binary=Path("/usr/bin/true"),
                    image_digest="sha256:" + "a" * 64,
                    operator="operator@example.com",
                    timeout=30,
                )
        run.assert_not_called()

    @mock.patch.dict(
        os.environ,
        {
            "CLOUDFLARE_ACCOUNT_ID": "account",
            "D1_DATABASE_ID": "database",
            "CF_API_TOKEN": "read-token",
            "R2_TDK_HEX": "00" * 32,
            "GC_LIVE_DELETE": "true",
            "GC_LIVE_DELETE_CONFIRM": "I_UNDERSTAND",
            "R2_S3_SECRET_ACCESS_KEY": "must-not-cross-boundary",
        },
        clear=True,
    )
    @mock.patch.object(collector.subprocess, "run")
    def test_collection_forces_read_only_env_and_emits_pending_package(
        self, run: mock.Mock
    ) -> None:
        run.return_value = subprocess.CompletedProcess(
            ["/usr/bin/true"], 0, report(), ""
        )
        package = collector.collect(
            scopes=[SCOPE],
            binary=Path("/usr/bin/true"),
            image_digest="sha256:" + "a" * 64,
            operator="operator@example.com",
            timeout=30,
        )
        child_env = run.call_args.kwargs["env"]
        self.assertEqual(child_env["GC_LIVE_DELETE"], "false")
        self.assertEqual(child_env["GC_OBSERVATION_ONLY"], "true")
        self.assertNotIn("GC_LIVE_DELETE_CONFIRM", child_env)
        self.assertNotIn("R2_S3_SECRET_ACCESS_KEY", child_env)
        self.assertEqual(package["tenant_region_population"], 1)
        self.assertEqual(package["approval"]["status"], "PENDING_OWNER_REVIEW")
        self.assertFalse(package["approval"]["live_delete_authorized"])
        collector.verify_package(package, expected_approval="pending")

    @mock.patch.dict(os.environ, {}, clear=True)
    def test_collection_requires_explicit_d1_environment(self) -> None:
        with self.assertRaisesRegex(collector.ObservationError, "missing required"):
            collector.collect(
                scopes=[SCOPE],
                binary=Path("/usr/bin/true"),
                image_digest="sha256:" + "a" * 64,
                operator="operator@example.com",
                timeout=30,
            )


class PackageVerificationTests(unittest.TestCase):
    def setUp(self) -> None:
        with (
            mock.patch.dict(
                os.environ,
                {
                    "CLOUDFLARE_ACCOUNT_ID": "account",
                    "D1_DATABASE_ID": "database",
                    "CF_API_TOKEN": "read-token",
                    "R2_TDK_HEX": "00" * 32,
                },
                clear=True,
            ),
            mock.patch.object(
                collector.subprocess,
                "run",
                return_value=subprocess.CompletedProcess(
                    ["/usr/bin/true"], 0, report(), ""
                ),
            ),
        ):
            self.package = collector.collect(
                scopes=[SCOPE],
                binary=Path("/usr/bin/true"),
                image_digest="registry.example/image@sha256:" + "b" * 64,
                operator="operator@example.com",
                timeout=30,
            )

    def test_aggregate_and_live_delete_mutations_fail(self) -> None:
        for path, value in (
            (("reclaimable_bytes",), 4095),
            (("tenant_region_population",), 2),
            (("tenant_region_population",), True),
            (("live_delete_flag",), True),
            (("runs", 0, "delete_count"), 1),
            (("runs", 0, "phase_budget", "duration_ms"), 1_800_001),
            (("approval", "live_delete_authorized"), True),
        ):
            mutated = copy.deepcopy(self.package)
            cursor = mutated
            for component in path[:-1]:
                cursor = cursor[component]
            cursor[path[-1]] = value
            with self.subTest(path=path), self.assertRaises(collector.ObservationError):
                collector.verify_package(mutated)

    def test_timestamps_must_be_utc_and_monotone(self) -> None:
        for path, value in (
            (("captured_at",), "not-a-time"),
            (("runs", 0, "started_at"), "2099-01-01T00:00:00Z"),
            (("runs", 0, "completed_at"), "2099-01-01T00:00:00+01:00"),
        ):
            mutated = copy.deepcopy(self.package)
            cursor = mutated
            for component in path[:-1]:
                cursor = cursor[component]
            cursor[path[-1]] = value
            with self.subTest(path=path), self.assertRaises(collector.ObservationError):
                collector.verify_package(mutated)

    def test_decision_requires_reviewer_and_never_authorizes_delete(self) -> None:
        approved = copy.deepcopy(self.package)
        approved["approval"] = {
            "status": "APPROVED",
            "reviewer": "owner@example.com",
            "decided_at": "2026-09-09T12:00:00Z",
            "live_delete_authorized": False,
        }
        collector.verify_package(approved, expected_approval="approved")
        approved["approval"]["reviewer"] = None
        with self.assertRaises(collector.ObservationError):
            collector.verify_package(approved)

    def test_atomic_writer_uses_mode_0600_and_rejects_symlink_destination(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "evidence.json"
            collector.write_atomic(output, self.package)
            self.assertEqual(output.stat().st_mode & 0o777, 0o600)
            collector.verify_package(json.loads(output.read_text(encoding="utf-8")))
            output.unlink()
            target = Path(directory) / "target"
            target.write_text("preserve", encoding="utf-8")
            output.symlink_to(target)
            with self.assertRaisesRegex(collector.ObservationError, "non-symlink"):
                collector.write_atomic(output, self.package)
            self.assertEqual(target.read_text(encoding="utf-8"), "preserve")


if __name__ == "__main__":
    unittest.main()
