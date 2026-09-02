#!/usr/bin/env python3
import argparse
import datetime as dt
import pathlib
import sys
import unittest
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import b152_actions_diagnostic as diag


UTC = dt.timezone.utc


def run(run_id, created, conclusion="failure"):
    return {"id": run_id, "created_at": created, "name": "wf", "conclusion": conclusion}


class B152DiagnosticTests(unittest.TestCase):
    def test_exact_600_second_duration_and_step_checkout_classification(self):
        job = {
            "id": 22,
            "name": "lane",
            "conclusion": "failure",
            "created_at": "2026-08-31T03:57:00Z",
            "started_at": "2026-08-31T04:00:00Z",
            "completed_at": "2026-08-31T04:10:00Z",
            "runner_name": "runner",
            "steps": [
                {"name": "actions/checkout", "status": "completed"},
                {"name": "Install", "status": "in_progress"},
            ],
        }
        item = diag.classify_job(job, run(1, "2026-08-31T04:00:00Z"), 594, 615)
        self.assertEqual(item["duration_seconds"], 600)
        self.assertEqual(item["queue_duration_seconds"], 180)
        self.assertTrue(item["in_window"])
        self.assertEqual(item["step_in_progress"], ["Install"])
        self.assertFalse(item["checkout_incomplete"])

    def test_duration_window_is_inclusive_at_each_boundary(self):
        job = {
            "id": 24,
            "name": "lane",
            "created_at": "2026-08-31T03:59:00Z",
            "started_at": "2026-08-31T04:00:00Z",
            "completed_at": "2026-08-31T04:09:54Z",
            "steps": [],
        }
        self.assertTrue(diag.classify_job(job, run(1, "2026-08-31T04:00:00Z"), 594, 615)["in_window"])
        job["completed_at"] = "2026-08-31T04:10:15Z"
        self.assertTrue(diag.classify_job(job, run(1, "2026-08-31T04:00:00Z"), 594, 615)["in_window"])
        job["completed_at"] = "2026-08-31T04:09:53Z"
        self.assertFalse(diag.classify_job(job, run(1, "2026-08-31T04:00:00Z"), 594, 615)["in_window"])

    def test_fractional_or_negative_durations_fail_closed_before_truncation(self):
        job = {
            "id": 25,
            "name": "lane",
            "created_at": "2026-08-31T03:59:00Z",
            "started_at": "2026-08-31T04:00:00Z",
            "completed_at": "2026-08-31T04:10:00.999Z",
            "steps": [],
        }
        with self.assertRaises(diag.EvidenceUnavailable):
            diag.classify_job(job, run(1, "2026-08-31T04:00:00Z"), 594, 615)
        job["completed_at"] = "2026-08-31T03:59:59.900Z"
        with self.assertRaises(diag.EvidenceUnavailable):
            diag.classify_job(job, run(1, "2026-08-31T04:00:00Z"), 594, 615)
        job["completed_at"] = "2026-08-31T04:10:00Z"
        job["created_at"] = "2026-08-31T03:59:59.500Z"
        with self.assertRaises(diag.EvidenceUnavailable):
            diag.classify_job(job, run(1, "2026-08-31T04:00:00Z"), 594, 615)

    def test_iso_preserves_fractional_cli_window_bounds(self):
        value = diag.parse_time("2026-08-31T04:00:00.123456Z")
        self.assertEqual(diag.iso(value), "2026-08-31T04:00:00.123456Z")

    def test_all_accepted_timestamp_spellings_preserve_six_digits(self):
        variants = (
            "2026-08-31T04:00:00.123456Z",
            "20260831T040000,123456+0000",
            "2026-08-31X04:00:00.123456+00:00",
            "2026-08-31 04:00:00,123456+0000",
        )
        for value in variants:
            with self.subTest(value=value):
                self.assertEqual(diag.iso(diag.parse_time(value)), "2026-08-31T04:00:00.123456Z")

    def test_timestamps_with_more_than_six_fractional_digits_fail_closed(self):
        variants = (
            "2026-08-31T04:00:00.1234567Z",
            "20260831T040000,1234567+0000",
            "2026-08-31X04:00:00.1234567+00:00",
            "2026-08-31 04:00:00,1234567+0000",
        )
        for value in variants:
            with self.subTest(value=value), self.assertRaises(argparse.ArgumentTypeError):
                diag.parse_time(value)
        malformed_run = run(1, "20260831X000000,1234567+0000")
        with patch.object(diag, "run_gh", return_value={"total_count": 1, "workflow_runs": [malformed_run]}):
            with self.assertRaises(diag.EvidenceUnavailable):
                diag.collect_runs(
                    "o/r", dt.datetime(2026, 8, 31, tzinfo=UTC),
                    dt.datetime(2026, 9, 1, tzinfo=UTC),
                )

    def test_incomplete_checkout_is_load_bearing(self):
        job = {
            "id": 23,
            "name": "lane",
            "conclusion": "failure",
            "created_at": "2026-08-31T03:59:00Z",
            "started_at": "2026-08-31T04:00:00Z",
            "completed_at": "2026-08-31T04:10:00Z",
            "steps": [{"name": "Checkout", "status": "in_progress"}],
        }
        item = diag.classify_job(job, run(1, "2026-08-31T04:00:00Z"), 594, 615)
        self.assertTrue(item["checkout_incomplete"])

    def test_unknown_step_status_fails_closed_for_checkout_and_non_checkout_steps(self):
        job = {
            "id": 26,
            "name": "lane",
            "created_at": "2026-08-31T03:59:00Z",
            "started_at": "2026-08-31T04:00:00Z",
            "completed_at": "2026-08-31T04:10:00Z",
            "steps": [],
        }
        for name in ("Checkout", "Build"):
            job["steps"] = [{"name": name, "status": "unknown_status"}]
            with self.subTest(name=name), self.assertRaises(diag.EvidenceUnavailable):
                diag.classify_job(job, run(1, "2026-08-31T04:00:00Z"), 594, 615)

    def test_truncated_page_splits_and_applies_half_open_bounds(self):
        start = dt.datetime(2026, 8, 31, tzinfo=UTC)
        end = start + dt.timedelta(hours=2)
        calls = []

        def fake(repo, endpoint):
            calls.append(endpoint)
            if "page=1" in endpoint and "T00%3A00%3A00Z..2026-08-31T02%3A00%3A00Z" in endpoint:
                return {"total_count": 1001, "workflow_runs": []}
            return {"total_count": 1, "workflow_runs": [run(7, "2026-08-31T01:00:00Z")]}

        with patch.object(diag, "run_gh", side_effect=fake):
            found = diag.collect_runs("o/r", start, end)
        self.assertEqual([item["id"] for item in found], [7])
        self.assertGreaterEqual(len(calls), 3)

    def test_exactly_1000_runs_splits_to_avoid_the_result_cap(self):
        start = dt.datetime(2026, 8, 31, tzinfo=UTC)
        end = start + dt.timedelta(hours=2)
        calls = []

        def fake(repo, endpoint):
            calls.append(endpoint)
            if "T00%3A00%3A00Z..2026-08-31T02%3A00%3A00Z" in endpoint:
                return {"total_count": 1000, "workflow_runs": []}
            return {"total_count": 0, "workflow_runs": []}

        with patch.object(diag, "run_gh", side_effect=fake):
            self.assertEqual(diag.collect_runs("o/r", start, end), [])
        self.assertGreaterEqual(len(calls), 3)

    def test_short_run_page_with_remaining_total_fails_closed(self):
        with patch.object(diag, "run_gh", return_value={"total_count": 2, "workflow_runs": [run(1, "2026-08-31T00:00:00Z")]}):
            with self.assertRaises(diag.EvidenceUnavailable):
                diag.collect_runs(
                    "o/r", dt.datetime(2026, 8, 31, tzinfo=UTC),
                    dt.datetime(2026, 9, 1, tzinfo=UTC),
                )

    def test_short_job_page_with_remaining_total_fails_closed(self):
        with patch.object(diag, "run_gh", return_value={"total_count": 2, "jobs": [{"id": 1}]}):
            with self.assertRaises(diag.EvidenceUnavailable):
                diag.collect_jobs("o/r", {"id": 9})

    def test_duplicate_job_ids_fail_closed(self):
        with patch.object(diag, "run_gh", return_value={"total_count": 2, "jobs": [{"id": 1}, {"id": 1}]}):
            with self.assertRaises(diag.EvidenceUnavailable):
                diag.collect_jobs("o/r", {"id": 9})

    def test_malformed_total_count_fails_closed(self):
        with patch.object(diag, "run_gh", return_value={"total_count": "1000", "workflow_runs": []}):
            with self.assertRaises(diag.EvidenceUnavailable):
                diag.collect_runs(
                    "o/r", dt.datetime(2026, 8, 31, tzinfo=UTC),
                    dt.datetime(2026, 9, 1, tzinfo=UTC),
                )

    def test_malformed_page_item_fails_closed(self):
        with patch.object(diag, "run_gh", return_value={"total_count": 1, "workflow_runs": ["not-a-run"]}):
            with self.assertRaises(diag.EvidenceUnavailable):
                diag.collect_runs(
                    "o/r", dt.datetime(2026, 8, 31, tzinfo=UTC),
                    dt.datetime(2026, 9, 1, tzinfo=UTC),
                )
        with patch.object(diag, "run_gh", return_value={"total_count": 1, "jobs": ["not-a-job"]}):
            with self.assertRaises(diag.EvidenceUnavailable):
                diag.collect_jobs("o/r", {"id": 9})

    def test_exactly_1000_jobs_probes_page_11(self):
        calls = []

        def fake(repo, endpoint):
            calls.append(endpoint)
            page = int(endpoint.split("&page=")[1].split("&")[0])
            return {
                "total_count": 1000,
                "jobs": [{"id": page * 100 + index} for index in range(100)] if page <= 10 else [],
            }

        with patch.object(diag, "run_gh", side_effect=fake):
            jobs = diag.collect_jobs("o/r", {"id": 9})
        self.assertEqual(len(jobs), 1000)
        self.assertEqual(len(calls), 11)
        self.assertIn("page=11", calls[-1])

    def test_unavailable_log_is_structured_indeterminate(self):
        failed = SimpleNamespace(returncode=1, stderr="HTTP 404 BlobNotFound")
        with patch.object(diag.subprocess, "run", return_value=failed):
            logs = diag.fetch_logs("o/r", [{"job_id": 42}])
        self.assertEqual(logs[0]["status"], "indeterminate")
        self.assertEqual(logs[0]["causal"], "indeterminate")
        self.assertEqual(logs[0]["job_id"], 42)
        self.assertEqual(logs[0]["error"], "log retrieval failed")

    def test_missing_gh_is_sanitized_indeterminate_not_a_traceback(self):
        with patch.object(diag.subprocess, "run", side_effect=FileNotFoundError("secret path")), patch("sys.stderr") as stderr:
            self.assertEqual(diag.main([
                "--start", "2026-08-31T00:00:00Z",
                "--end", "2026-08-31T01:00:00Z",
            ]), 2)
        rendered = "".join(call.args[0] for call in stderr.write.call_args_list)
        self.assertIn("INDETERMINATE: gh api could not be started", rendered)
        self.assertNotIn("secret path", rendered)

    def test_missing_gh_while_fetching_logs_is_sanitized_indeterminate(self):
        report = {"run_ids": [], "failed_jobs": [], "window_jobs": [{"job_id": 42}]}
        with patch.object(diag, "collect_evidence", return_value=report), patch.object(diag.subprocess, "run", side_effect=FileNotFoundError("secret path")), patch("sys.stderr") as stderr:
            self.assertEqual(diag.main([
                "--start", "2026-08-31T00:00:00Z",
                "--end", "2026-08-31T01:00:00Z",
                "--fetch-logs",
            ]), 2)
        rendered = "".join(call.args[0] for call in stderr.write.call_args_list)
        self.assertIn("INDETERMINATE: gh api could not be started while fetching logs", rendered)
        self.assertNotIn("secret path", rendered)

    def test_output_write_error_is_sanitized_indeterminate_not_a_traceback(self):
        report = {"run_ids": [], "failed_jobs": [], "window_jobs": []}
        with patch.object(diag, "collect_evidence", return_value=report), patch.object(pathlib.Path, "write_text", side_effect=OSError("secret path")), patch("sys.stderr") as stderr:
            self.assertEqual(diag.main([
                "--start", "2026-08-31T00:00:00Z",
                "--end", "2026-08-31T01:00:00Z",
                "--output", "/private/tmp/secret-report.json",
            ]), 2)
        rendered = "".join(call.args[0] for call in stderr.write.call_args_list)
        self.assertIn("INDETERMINATE: report output could not be written", rendered)
        self.assertNotIn("secret path", rendered)

    def test_available_logs_do_not_establish_a_cause(self):
        report = {"run_ids": [7], "failed_jobs": [], "window_jobs": [{"job_id": 42}]}
        available = [{"job_id": 42, "available": True, "status": "available", "causal": False, "error": None}]
        with patch.object(diag, "collect_evidence", return_value=report), patch.object(diag, "fetch_logs", return_value=available), patch("sys.stdout") as stdout:
            self.assertEqual(diag.main([
                "--start", "2026-08-31T00:00:00Z",
                "--end", "2026-08-31T01:00:00Z",
                "--fetch-logs",
            ]), 0)
        rendered = "".join(call.args[0] for call in stdout.write.call_args_list)
        self.assertIn('\"status\": \"not_established\"', rendered)
        self.assertIn('\"causal\": false', rendered)

    def test_known_run_control_does_not_accept_missing_run(self):
        with patch.object(diag, "collect_evidence", return_value={
            "failed_jobs": [], "window_jobs": []
        }), patch.object(diag, "collect_runs", return_value=[]):
            self.assertEqual(diag.main([
                "--start", "2026-08-31T00:00:00Z",
                "--end", "2026-08-31T01:00:00Z",
                "--known-run", "7",
            ]), 2)


if __name__ == "__main__":
    unittest.main()
