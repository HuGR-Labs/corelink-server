#!/usr/bin/env python3
"""Focused fail-closed tests for the bounded owner-action packet."""

from __future__ import annotations

import copy
import importlib.util
import json
import subprocess
import sys
import unittest
from unittest import mock
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_owner_action_packets", ROOT / "scripts/verify_owner_action_packets.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class OwnerActionPacketTests(unittest.TestCase):
    def setUp(self) -> None:
        self.data = json.loads(MODULE.PACKET.read_text(encoding="utf-8"))

    def test_exact_closed_population_passes(self) -> None:
        self.assertEqual(MODULE.check_data(self.data), {"items": 29, "population": 29})

    def test_b111_hermetic_cli_passes(self) -> None:
        result = subprocess.run(
            [sys.executable, "-S", "scripts/verify_owner_action_packets.py", "--id", "B-111"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("owner-action packet: PASS: 29 item(s)", result.stdout)

    def test_b013_reconciled_closure_matches_backlog(self) -> None:
        self.assertEqual(
            MODULE.check_data(self.data, "B-013"),
            {"items": 29, "population": 29},
        )
        item = next(entry for entry in self.data["items"] if entry["id"] == "B-013")
        self.assertEqual((item["owner"], item["status"]), ("tl", "done"))

        stale = copy.deepcopy(self.data)
        stale_item = next(entry for entry in stale["items"] if entry["id"] == "B-013")
        stale_item.update(owner="owner", status="open")
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(stale, "B-013")

    def test_b110_reconciled_closure_matches_backlog(self) -> None:
        self.assertEqual(
            MODULE.check_data(self.data, "B-110"),
            {"items": 29, "population": 29},
        )
        item = next(entry for entry in self.data["items"] if entry["id"] == "B-110")
        self.assertEqual((item["owner"], item["status"]), ("tl", "done"))

        stale = copy.deepcopy(self.data)
        stale_item = next(entry for entry in stale["items"] if entry["id"] == "B-110")
        stale_item.update(owner="owner", status="open")
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(stale, "B-110")

    def test_b110_exact_evidence_binding_rejects_packet_mutations(self) -> None:
        mutations = (
            ("path", "evidence/owner-actions/B-110/missing.json"),
            ("item_schema", "selected_option is anything"),
            ("required_fields", ["schema_version"]),
            ("procedure", ["RUN: noop", "RUN: noop"]),
            ("expected_postcondition", "B-110 is parked"),
        )
        for field, value in mutations:
            with self.subTest(field=field):
                mutated = copy.deepcopy(self.data)
                item = next(entry for entry in mutated["items"] if entry["id"] == "B-110")
                if field in {"path", "item_schema", "required_fields"}:
                    item["evidence"][field] = value
                else:
                    item[field] = value
                with self.assertRaises(MODULE.PacketError):
                    MODULE.check_data(mutated, "B-110")

    def test_b110_capacity_decision_evidence_mutations_fail_closed(self) -> None:
        for field, value in (
            ("selected_option", "owner_authorized_park"),
            ("runner_labels", ["mac"]),
            ("workflows", []),
        ):
            with self.subTest(field=field):
                record = copy.deepcopy(MODULE._read_b110_evidence())
                record[field] = value
                with mock.patch.object(MODULE, "_read_b110_evidence", return_value=record):
                    with self.assertRaises(MODULE.PacketError):
                        MODULE.check_data(self.data, "B-110")

    def test_b054_receipt_mutations_fail_closed(self) -> None:
        original = MODULE._read_json_evidence
        record = copy.deepcopy(original(MODULE.B054_EVIDENCE_PATH, MODULE.B054_EVIDENCE_REQUIRED_FIELDS, "B-054"))
        record["migration_receipts"]["blocker"] = ""
        def read_b054(path, fields, label):
            return record if path == MODULE.B054_EVIDENCE_PATH else original(path, fields, label)
        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_b054):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-054")

        record = copy.deepcopy(original(MODULE.B054_EVIDENCE_PATH, MODULE.B054_EVIDENCE_REQUIRED_FIELDS, "B-054"))
        record["witness_receipt"]["reference"] = "forged-reference"
        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_b054):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-054")

    def test_b083_receipt_mutations_fail_closed(self) -> None:
        original = MODULE._read_json_evidence
        record = copy.deepcopy(original(MODULE.B083_EVIDENCE_PATH, MODULE.B083_EVIDENCE_REQUIRED_FIELDS, "B-083"))
        record["activation"]["audit_event_reference"] = "forged-reference"
        def read_b083(path, fields, label):
            return record if path == MODULE.B083_EVIDENCE_PATH else original(path, fields, label)
        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_b083):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-083")

    def test_b097_read_only_capture_mutations_fail_closed(self) -> None:
        original = MODULE._read_json_evidence
        record = copy.deepcopy(original(MODULE.B097_EVIDENCE_PATH, MODULE.B097_EVIDENCE_REQUIRED_FIELDS, "B-097"))
        record["read_only_capture"]["support_case"] = "submitted"
        def read_b097(path, fields, label):
            return record if path == MODULE.B097_EVIDENCE_PATH else original(path, fields, label)
        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_b097):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-097")

        record = copy.deepcopy(original(MODULE.B097_EVIDENCE_PATH, MODULE.B097_EVIDENCE_REQUIRED_FIELDS, "B-097"))
        record["read_only_capture"]["active_tenant_metric"] = "available"
        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_b097):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-097")

    def test_every_load_bearing_field_mutation_fails(self) -> None:
        self.assertEqual(MODULE.mutation_self_test(self.data), 29 * (len(MODULE.ITEM_FIELDS) + 2) + 2)

    def test_backlog_owner_status_mismatch_fails_closed(self) -> None:
        contracts = MODULE._read_backlog_contracts()
        contracts["B-029"] = ("owner", "open")
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(self.data, backlog_contracts=contracts)

        contracts = MODULE._read_backlog_contracts()
        contracts["B-142"] = ("tl", "closed")
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(self.data, backlog_contracts=contracts)

    def test_missing_item_and_ambiguous_field_fail_closed(self) -> None:
        missing = copy.deepcopy(self.data)
        del missing["items"][0]["evidence"]
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(missing)

        ambiguous = copy.deepcopy(self.data)
        ambiguous["items"][0]["procedure"] = "UI: do the thing"
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(ambiguous)

    def test_unknown_and_duplicate_population_fail_closed(self) -> None:
        unknown = copy.deepcopy(self.data)
        unknown["population"][0] = "B-999"
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(unknown)

        duplicate = copy.deepcopy(self.data)
        duplicate["items"][1]["id"] = duplicate["items"][0]["id"]
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(duplicate)

    def test_b111_semantic_workflow_secret_mutation_fails_closed(self) -> None:
        contracts = MODULE._read_b111_workflow_contracts()
        mutated = copy.deepcopy(contracts)
        path = ".github/workflows/notarize-macos.yml"
        mutated[path]["declared"] = frozenset(
            set(mutated[path]["declared"]) - {"APPLE_NOTARIZATION_API_KEY"}
        )
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(self.data, workflow_contracts=mutated)

    def test_b111_release_chain_mutation_fails_closed(self) -> None:
        contracts = MODULE._read_b111_workflow_contracts()
        mutated = copy.deepcopy(contracts)
        jobs = mutated["release-chain"]["jobs"]
        jobs["notarize-macos"]["needs"] = ["release"]
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(self.data, workflow_contracts=mutated)

    def test_b111_obsolete_secret_name_fails_closed(self) -> None:
        mutated = copy.deepcopy(self.data)
        item = next(entry for entry in mutated["items"] if entry["id"] == "B-111")
        item["procedure"][0] = item["procedure"][0].replace(
            "APPLE_NOTARIZATION_API_KEY", "APPLE_NOTARIZATION_PASSWORD"
        )
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(mutated)


if __name__ == "__main__":
    unittest.main()
