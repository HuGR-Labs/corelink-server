#!/usr/bin/env python3
"""
beta_feedback_triage_test.py — pytest suite for the beta-feedback triage
harness (`scripts/beta-feedback-ingest.py`).

Coverage axes:
- §3 classification rubric edge cases (P0/P1/P2/P3 floors, regression
  inheritance, cross-tenant short-circuit, pilot-vs-internal floor).
- §2 SLA computation (deadlines match the documented hours per severity).
- §4 routing matrix sanity (every surface routes to a documented owner;
  unknown surface falls back to triage).
- End-to-end ingest path: validates / rejects malformed records, emits
  deterministic markdown.

The script under test lives at `scripts/beta-feedback-ingest.py` and is
loaded via importlib so the hyphenated filename is fine.

Run:
    python3 -m pytest tests/beta_feedback_triage_test.py -v
    # or, if pytest is unavailable:
    python3 tests/beta_feedback_triage_test.py
"""

from __future__ import annotations

import importlib.util
import io
import json
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
INGEST_PATH = REPO_ROOT / "scripts" / "beta-feedback-ingest.py"


def _load_ingest_module():
    spec = importlib.util.spec_from_file_location("beta_feedback_ingest", INGEST_PATH)
    assert spec is not None, f"could not load {INGEST_PATH}"
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    # Register before exec so @dataclass introspection finds the module
    # (Python 3.12+ dataclasses inspect sys.modules[cls.__module__]).
    sys.modules["beta_feedback_ingest"] = module
    spec.loader.exec_module(module)
    return module


bfi = _load_ingest_module()


# ---------------------------------------------------------------------------
# Fixtures — minimal valid record. Tests mutate copies.
# ---------------------------------------------------------------------------


def _base_record(**overrides) -> dict:
    record = {
        "id": "BFB-2026-05-16-0001",
        "received_at": "2026-05-16T12:00:00Z",
        "tenant_id": "t_pilot_acme",
        "reporter": "jane.doe@acme.example",
        "severity": "P2",
        "surface_area": "api",
        "title": "Slow bulk import",
        "repro_steps": "1. POST /v1/imports\n2. wait",
        "expected": "200 OK",
        "actual": "503 timeout",
        "tenant_impact": "degraded",
    }
    record.update(overrides)
    return record


# ---------------------------------------------------------------------------
# §3 — classification rubric
# ---------------------------------------------------------------------------


class ClassificationTests(unittest.TestCase):
    def test_full_outage_on_pilot_floors_at_p0(self):
        record = _base_record(severity="P2", tenant_impact="full_outage")
        self.assertEqual(bfi.classify(record), "P0")

    def test_full_outage_on_non_pilot_floors_at_p1(self):
        record = _base_record(
            tenant_id="t_internal_test",
            severity="P2",
            tenant_impact="full_outage",
        )
        self.assertEqual(bfi.classify(record), "P1")

    def test_full_outage_with_non_pilot_note_floors_at_p1(self):
        # Pilot-prefixed tenant_id but flagged as a non-pilot context.
        record = _base_record(
            tenant_id="t_pilot_acme",
            severity="P2",
            tenant_impact="full_outage",
            notes="non-pilot-tenant ran a replay against staging",
        )
        self.assertEqual(bfi.classify(record), "P1")

    def test_degraded_floors_at_p1(self):
        record = _base_record(severity="P3", tenant_impact="degraded")
        self.assertEqual(bfi.classify(record), "P1")

    def test_workable_floors_at_p2(self):
        record = _base_record(severity="P3", tenant_impact="workable")
        self.assertEqual(bfi.classify(record), "P2")

    def test_cosmetic_stays_p3(self):
        record = _base_record(severity="P3", tenant_impact="cosmetic")
        self.assertEqual(bfi.classify(record), "P3")

    def test_rubric_never_downgrades_supplied_severity(self):
        # Reporter says P0; impact says only "workable" — never downgrade.
        record = _base_record(severity="P0", tenant_impact="workable")
        self.assertEqual(bfi.classify(record), "P0")

    def test_cross_tenant_short_circuits_to_p0(self):
        record = _base_record(
            severity="P3",
            tenant_impact="workable",
            title="possible cross-tenant read in /v1/audits",
        )
        self.assertEqual(bfi.classify(record), "P0")

    def test_rls_bypass_in_actual_short_circuits_to_p0(self):
        record = _base_record(
            severity="P2",
            tenant_impact="degraded",
            actual="response leaked another tenant's row — RLS bypass",
        )
        self.assertEqual(bfi.classify(record), "P0")

    def test_regression_inherits_prior_severity(self):
        record = _base_record(
            severity="P3",
            tenant_impact="workable",
            notes="regression-of:BFB-2026-05-10-0042:P1 — same symptom",
        )
        self.assertEqual(bfi.classify(record), "P1")

    def test_invalid_severity_raises(self):
        record = _base_record(severity="critical")
        with self.assertRaises(ValueError):
            bfi.classify(record)

    def test_invalid_impact_raises(self):
        record = _base_record(tenant_impact="meh")
        with self.assertRaises(ValueError):
            bfi.classify(record)


# ---------------------------------------------------------------------------
# §2 — SLA computation
# ---------------------------------------------------------------------------


class SlaTests(unittest.TestCase):
    def test_p0_ack_within_4_hours(self):
        record = _base_record(received_at="2026-05-16T12:00:00Z")
        sla = bfi.compute_sla(record, "P0")
        self.assertEqual(sla["ack_deadline_at"], "2026-05-16T16:00:00Z")
        self.assertEqual(sla["ack_sla_hours"], "4")

    def test_p1_ack_within_24_hours(self):
        record = _base_record(received_at="2026-05-16T12:00:00Z")
        sla = bfi.compute_sla(record, "P1")
        self.assertEqual(sla["ack_deadline_at"], "2026-05-17T12:00:00Z")
        self.assertEqual(sla["ack_sla_hours"], "24")

    def test_p2_ack_within_7_days(self):
        record = _base_record(received_at="2026-05-16T12:00:00Z")
        sla = bfi.compute_sla(record, "P2")
        self.assertEqual(sla["ack_deadline_at"], "2026-05-23T12:00:00Z")

    def test_p3_ack_within_30_days(self):
        record = _base_record(received_at="2026-05-16T12:00:00Z")
        sla = bfi.compute_sla(record, "P3")
        self.assertEqual(sla["ack_deadline_at"], "2026-06-15T12:00:00Z")

    def test_p0_resolve_within_24_hours(self):
        record = _base_record(received_at="2026-05-16T12:00:00Z")
        sla = bfi.compute_sla(record, "P0")
        self.assertEqual(sla["resolve_deadline_at"], "2026-05-17T12:00:00Z")

    def test_sla_doc_table_matches_constants(self):
        # Belt-and-suspenders: the constants in the script MUST match the
        # documented §2 SLA table. If anyone changes one without the
        # other, this test trips.
        self.assertEqual(bfi.ACK_SLA_HOURS, {"P0": 4, "P1": 24, "P2": 168, "P3": 720})
        self.assertEqual(
            bfi.RESOLVE_SLA_HOURS,
            {"P0": 24, "P1": 168, "P2": 720, "P3": 2160},
        )

    def test_compute_sla_rejects_naive_timestamp(self):
        record = _base_record(received_at="2026-05-16T12:00:00")  # no tz
        with self.assertRaises(ValueError):
            bfi.compute_sla(record, "P1")


# ---------------------------------------------------------------------------
# §4 — routing matrix
# ---------------------------------------------------------------------------


class RoutingTests(unittest.TestCase):
    def test_every_surface_routes_to_documented_owner(self):
        for surface in bfi.VALID_SURFACES:
            with self.subTest(surface=surface):
                record = _base_record(surface_area=surface)
                routing = bfi.route(record)
                self.assertIn("owner", routing)
                self.assertIn("secondary", routing)
                self.assertIn("label", routing)
                self.assertTrue(routing["owner"])
                self.assertTrue(routing["label"].startswith("area/"))

    def test_unknown_surface_falls_back_to_triage(self):
        record = _base_record(surface_area="other")
        record["surface_area"] = "made-up-surface"
        routing = bfi.route(record)
        self.assertEqual(routing["label"], "area/triage")

    def test_missing_surface_falls_back_to_triage(self):
        record = _base_record()
        record.pop("surface_area")
        routing = bfi.route(record)
        self.assertEqual(routing["label"], "area/triage")

    def test_routing_matrix_is_complete_against_surfaces(self):
        # Every documented VALID_SURFACE must have a routing entry.
        for surface in bfi.VALID_SURFACES:
            self.assertIn(surface, bfi.ROUTING_MATRIX)

    def test_auth_routes_to_identity_team(self):
        record = _base_record(surface_area="auth")
        self.assertEqual(bfi.route(record)["owner"], "Identity & AuthZ")

    def test_billing_routes_to_billing_team(self):
        record = _base_record(surface_area="billing")
        self.assertEqual(bfi.route(record)["owner"], "Billing / Stripe")


# ---------------------------------------------------------------------------
# Wave assignment
# ---------------------------------------------------------------------------


class WaveAssignmentTests(unittest.TestCase):
    def test_p0_preempts_current_wave(self):
        record = _base_record()
        self.assertEqual(bfi.assign_wave(record, "P0", 23), "wave-23")

    def test_p1_lands_next_wave(self):
        record = _base_record()
        self.assertEqual(bfi.assign_wave(record, "P1", 23), "wave-24")

    def test_p2_lands_wave_plus_two(self):
        record = _base_record()
        self.assertEqual(bfi.assign_wave(record, "P2", 23), "wave-25")

    def test_p3_lands_wave_plus_three(self):
        record = _base_record()
        self.assertEqual(bfi.assign_wave(record, "P3", 23), "wave-26")

    def test_explicit_wave_target_wins(self):
        record = _base_record(wave_target="wave-99")
        self.assertEqual(bfi.assign_wave(record, "P3", 23), "wave-99")


# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------


class ValidationTests(unittest.TestCase):
    def test_missing_required_field_flags(self):
        record = _base_record()
        record.pop("repro_steps")
        errors = bfi.validate(record)
        self.assertTrue(any("repro_steps" in e for e in errors))

    def test_bad_enum_value_flags(self):
        record = _base_record(severity="P9")
        errors = bfi.validate(record)
        self.assertTrue(any("severity" in e for e in errors))

    def test_bad_timestamp_flags(self):
        record = _base_record(received_at="not-a-date")
        errors = bfi.validate(record)
        self.assertTrue(any("received_at" in e for e in errors))

    def test_well_formed_record_passes(self):
        record = _base_record()
        self.assertEqual(bfi.validate(record), [])


# ---------------------------------------------------------------------------
# End-to-end ingest + markdown
# ---------------------------------------------------------------------------


class IngestTests(unittest.TestCase):
    def test_ingest_accepts_valid_record_and_rejects_garbage(self):
        good = json.dumps(_base_record(severity="P2", tenant_impact="degraded"))
        bad_json = "{not-json"
        missing_fields = json.dumps({"id": "BFB-2026-05-16-0099"})
        lines = [good, bad_json, missing_fields]
        report = bfi.ingest(lines, current_wave=23)
        self.assertEqual(report.accepted_count, 1)
        self.assertEqual(report.rejected_count, 2)

    def test_ingest_classifies_and_assigns_wave(self):
        rec = _base_record(severity="P3", tenant_impact="full_outage")
        report = bfi.ingest([json.dumps(rec)], current_wave=23)
        self.assertEqual(report.accepted_count, 1)
        processed = report.processed[0]
        self.assertEqual(processed.severity, "P0")
        self.assertEqual(processed.wave, "wave-23")  # P0 preempts

    def test_ingest_is_deterministic(self):
        rec_a = _base_record(
            id="BFB-2026-05-16-0001",
            severity="P2",
            tenant_impact="degraded",
            received_at="2026-05-16T10:00:00Z",
        )
        rec_b = _base_record(
            id="BFB-2026-05-16-0002",
            severity="P2",
            tenant_impact="degraded",
            received_at="2026-05-16T11:00:00Z",
        )
        lines = [json.dumps(rec_a), json.dumps(rec_b)]
        report1 = bfi.ingest(lines, current_wave=23)
        report2 = bfi.ingest(lines, current_wave=23)
        md1 = bfi.emit_markdown(report1, 23)
        md2 = bfi.emit_markdown(report2, 23)
        self.assertEqual(md1, md2)

    def test_emit_markdown_groups_by_severity(self):
        rec_p0 = _base_record(
            id="BFB-2026-05-16-0001",
            severity="P0",
            tenant_impact="full_outage",
        )
        rec_p2 = _base_record(
            id="BFB-2026-05-16-0002",
            severity="P2",
            tenant_impact="workable",
            received_at="2026-05-16T13:00:00Z",
        )
        report = bfi.ingest([json.dumps(rec_p0), json.dumps(rec_p2)], current_wave=23)
        md = bfi.emit_markdown(report, 23)
        # P0 section appears before P2 section.
        self.assertLess(md.index("## P0"), md.index("## P2"))
        self.assertIn("BFB-2026-05-16-0001", md)
        self.assertIn("BFB-2026-05-16-0002", md)

    def test_emit_markdown_includes_rejection_section(self):
        report = bfi.ingest(["{not-json"], current_wave=23)
        md = bfi.emit_markdown(report, 23)
        self.assertIn("Rejected records", md)

    def test_main_exit_code_nonzero_on_rejections(self):
        # Drive main() with stdin redirected through monkeypatching argv.
        ndjson = "{not-json\n"
        orig_stdin, orig_stdout = sys.stdin, sys.stdout
        try:
            sys.stdin = io.StringIO(ndjson)
            sys.stdout = io.StringIO()
            rc = bfi.main(["--current-wave", "23"])
        finally:
            sys.stdin, sys.stdout = orig_stdin, orig_stdout
        self.assertEqual(rc, 1)

    def test_main_schema_dump_returns_zero(self):
        orig_stdout = sys.stdout
        try:
            sys.stdout = io.StringIO()
            rc = bfi.main(["--schema"])
            out = sys.stdout.getvalue()
        finally:
            sys.stdout = orig_stdout
        self.assertEqual(rc, 0)
        parsed = json.loads(out)
        self.assertEqual(parsed["title"], "BetaFeedbackIntake")


if __name__ == "__main__":
    unittest.main(verbosity=2)
