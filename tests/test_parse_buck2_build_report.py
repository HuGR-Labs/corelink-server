"""Focused B-113 tests for the pinned Buck2 report counter contract."""
from __future__ import annotations

import copy
import json
import tempfile
import unittest
from pathlib import Path

from scripts.parse_buck2_build_report import (
    PINNED_FIXTURE,
    ReportError,
    _fixture,
    load_report,
    parse_counters,
    self_test,
)


class Buck2BuildReportTests(unittest.TestCase):
    def test_pinned_report_fixture_uses_documented_nested_counters(self) -> None:
        self.assertEqual(
            parse_counters(load_report(PINNED_FIXTURE)),
            {"cache_hits": 4, "total_actions": 4},
        )

    def test_self_test_rejects_all_required_mutations(self) -> None:
        result = self_test()
        self.assertEqual(
            result["rejected_mutations"],
            [
                "absent build_metrics",
                "renamed remote_cache_hits",
                "string counter",
                "negative counter",
                "fractional counter",
                "non-finite counter",
                "inconsistent counters",
            ],
        )

    def test_each_counter_mutation_fails_closed(self) -> None:
        for mutation in (
            "absent",
            "renamed",
            "type",
            "negative",
            "fractional",
            "non-finite",
            "inconsistent",
        ):
            with self.subTest(mutation=mutation):
                report = copy.deepcopy(_fixture())
                metrics = report["build_metrics"]["metrics"]
                if mutation == "absent":
                    del report["build_metrics"]
                elif mutation == "renamed":
                    metrics["cache_hits"] = metrics.pop("remote_cache_hits")
                elif mutation == "type":
                    metrics["declared_actions"] = "4"
                elif mutation == "negative":
                    metrics["remote_cache_hits"] = -1
                elif mutation == "fractional":
                    metrics["remote_cache_hits"] = 2.5
                elif mutation == "non-finite":
                    metrics["remote_cache_hits"] = float("nan")
                elif mutation == "inconsistent":
                    metrics["remote_cache_hits"] = 5
                with self.assertRaises(ReportError):
                    parse_counters(report)

    def test_json_loader_rejects_non_finite_counter(self) -> None:
        report = _fixture()
        report["build_metrics"]["metrics"]["remote_cache_hits"] = float("nan")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            path.write_text(json.dumps(report), encoding="utf-8")
            with self.assertRaises(ReportError):
                load_report(path)


if __name__ == "__main__":
    unittest.main()
