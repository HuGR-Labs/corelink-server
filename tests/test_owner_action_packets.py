#!/usr/bin/env python3
"""Focused fail-closed tests for the bounded owner-action packet."""

from __future__ import annotations

import copy
import importlib.util
import json
import subprocess
import sys
import tempfile
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

    def test_b089_packet_references_resolve_and_fail_on_stale_path(self) -> None:
        item = next(entry for entry in self.data["items"] if entry["id"] == "B-089")
        MODULE._check_b089_surface_contract(item)
        for field in ("references", "procedure"):
            with self.subTest(field=field):
                mutated = copy.deepcopy(self.data)
                row = next(entry for entry in mutated["items"] if entry["id"] == "B-089")
                if field == "references":
                    row[field][2] = "apps/docs/src/pages/terms.tsx"
                else:
                    row[field][1] = row[field][1].replace(
                        "apps/docs/src/pages/legal/terms.tsx", "apps/docs/src/pages/terms.tsx"
                    )
                with self.assertRaises(MODULE.PacketError):
                    MODULE.check_data(mutated, "B-089")

    def test_b089_known_cross_document_drift_cannot_change_silently(self) -> None:
        item = next(entry for entry in self.data["items"] if entry["id"] == "B-089")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in MODULE.B089_SURFACES:
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((ROOT / name).read_bytes())
            MODULE._check_b089_surface_contract(item, root)

            mutations = (
                (MODULE.B089_SURFACES[0], "| **Starter** |", "| **Solo** |"),
                (MODULE.B089_SURFACES[1], "Pro-tier customers are entitled to a", "Pro-tier customers are not entitled to a"),
                (MODULE.B089_SURFACES[2], '"solo",', '"business",'),
                (MODULE.B089_SURFACES[2], '  pro: {\n    id: "pro",', '  pro: {\n    slaCredits: true,\n    id: "pro",'),
            )
            for name, old, new in mutations:
                with self.subTest(source=name, mutation=old):
                    target = root / name
                    original = target.read_text(encoding="utf-8")
                    self.assertIn(old, original)
                    target.write_text(original.replace(old, new, 1), encoding="utf-8")
                    try:
                        with self.assertRaises(MODULE.PacketError):
                            MODULE._check_b089_surface_contract(item, root)
                    finally:
                        target.write_text(original, encoding="utf-8")
            (root / MODULE.B089_SURFACES[1]).unlink()
            with self.assertRaises(MODULE.PacketError):
                MODULE._check_b089_surface_contract(item, root)

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

        for field, value in (
            ("algorithm_version", "E1/keyed"),
            ("verification", "BLOCKED"),
        ):
            mutated = copy.deepcopy(original(MODULE.B054_EVIDENCE_PATH, MODULE.B054_EVIDENCE_REQUIRED_FIELDS, "B-054"))
            mutated["legacy_epoch"][field] = value
            def read_mutated(path, fields, label, receipt=mutated):
                return receipt if path == MODULE.B054_EVIDENCE_PATH else original(path, fields, label)
            with self.subTest(field=field), mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_mutated):
                with self.assertRaises(MODULE.PacketError):
                    MODULE.check_data(self.data, "B-054")

        mutated = copy.deepcopy(original(MODULE.B054_EVIDENCE_PATH, MODULE.B054_EVIDENCE_REQUIRED_FIELDS, "B-054"))
        mutated["keyed_epoch"]["status"] = "NOT_EXECUTED"
        def read_keyed(path, fields, label):
            return mutated if path == MODULE.B054_EVIDENCE_PATH else original(path, fields, label)
        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_keyed):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-054")

        mutated = copy.deepcopy(original(MODULE.B054_EVIDENCE_PATH, MODULE.B054_EVIDENCE_REQUIRED_FIELDS, "B-054"))
        mutated["witness_receipt"]["status"] = "NOT_EXECUTED"
        def read_witness(path, fields, label):
            return mutated if path == MODULE.B054_EVIDENCE_PATH else original(path, fields, label)
        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_witness):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-054")

        mutated = copy.deepcopy(original(MODULE.B054_EVIDENCE_PATH, MODULE.B054_EVIDENCE_REQUIRED_FIELDS, "B-054"))
        mutated["repository_checks"][0]["command"] = "backlog B-054 verification shell"
        def read_command(path, fields, label):
            return mutated if path == MODULE.B054_EVIDENCE_PATH else original(path, fields, label)
        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_command):
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

        for field, value in (("check_access", "PASS"), ("tenant_redacted", "tenant-redacted")):
            mutated = copy.deepcopy(original(MODULE.B083_EVIDENCE_PATH, MODULE.B083_EVIDENCE_REQUIRED_FIELDS, "B-083"))
            mutated[field] = value
            def read_mutated(path, fields, label, receipt=mutated):
                return receipt if path == MODULE.B083_EVIDENCE_PATH else original(path, fields, label)
            with self.subTest(field=field), mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_mutated):
                with self.assertRaises(MODULE.PacketError):
                    MODULE.check_data(self.data, "B-083")

        for field, value in (
            ("command", "backlog B-083 verification shell"),
            ("detail", "activation remains fail-closed"),
        ):
            mutated = copy.deepcopy(original(MODULE.B083_EVIDENCE_PATH, MODULE.B083_EVIDENCE_REQUIRED_FIELDS, "B-083"))
            mutated["repository_checks"][1][field] = value
            def read_repository_check(path, fields, label, receipt=mutated):
                return receipt if path == MODULE.B083_EVIDENCE_PATH else original(path, fields, label)
            with self.subTest(repository_check_field=field), mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_repository_check):
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

        mutated = copy.deepcopy(original(MODULE.B097_EVIDENCE_PATH, MODULE.B097_EVIDENCE_REQUIRED_FIELDS, "B-097"))
        mutated["read_only_capture"]["activity_readback"]["interpretation"] = "active tenants confirmed"
        def read_interpretation(path, fields, label):
            return mutated if path == MODULE.B097_EVIDENCE_PATH else original(path, fields, label)
        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_interpretation):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-097")

        record = copy.deepcopy(original(MODULE.B097_EVIDENCE_PATH, MODULE.B097_EVIDENCE_REQUIRED_FIELDS, "B-097"))
        record["read_only_capture"]["active_tenant_metric"] = "available"
        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_b097):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-097")

    def test_b086_unresolved_receipt_mutations_fail_closed(self) -> None:
        original = MODULE._read_json_evidence
        record = copy.deepcopy(original(MODULE.B086_EVIDENCE_PATH, MODULE.B086_EVIDENCE_REQUIRED_FIELDS, "B-086"))
        record["decision"] = "jurisdictional_d1"

        def read_decision(path, fields, label):
            return record if path == MODULE.B086_EVIDENCE_PATH else original(path, fields, label)

        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_decision):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-086")

        record = copy.deepcopy(original(MODULE.B086_EVIDENCE_PATH, MODULE.B086_EVIDENCE_REQUIRED_FIELDS, "B-086"))
        record["mutations_performed"] = ["created jurisdictional D1"]

        def read_mutation(path, fields, label):
            return record if path == MODULE.B086_EVIDENCE_PATH else original(path, fields, label)

        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_mutation):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-086")

        record = copy.deepcopy(original(MODULE.B086_EVIDENCE_PATH, MODULE.B086_EVIDENCE_REQUIRED_FIELDS, "B-086"))
        record["source_sha256"]["wrangler.toml"] = "0" * 64

        def read_source_hash(path, fields, label):
            return record if path == MODULE.B086_EVIDENCE_PATH else original(path, fields, label)

        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_source_hash):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-086")

        record = copy.deepcopy(original(MODULE.B086_EVIDENCE_PATH, MODULE.B086_EVIDENCE_REQUIRED_FIELDS, "B-086"))
        record["capture_commit"] = "0" * 40

        def read_capture_commit(path, fields, label):
            return record if path == MODULE.B086_EVIDENCE_PATH else original(path, fields, label)

        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_capture_commit):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-086")

    def test_b154_unresolved_receipt_mutations_fail_closed(self) -> None:
        original = MODULE._read_json_evidence
        record = copy.deepcopy(original(MODULE.B154_EVIDENCE_PATH, MODULE.B154_EVIDENCE_REQUIRED_FIELDS, "B-154"))
        record["notices"][0]["status"] = "EXECUTED"

        def read_notice(path, fields, label):
            return record if path == MODULE.B154_EVIDENCE_PATH else original(path, fields, label)

        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_notice):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-154")

        record = copy.deepcopy(original(MODULE.B154_EVIDENCE_PATH, MODULE.B154_EVIDENCE_REQUIRED_FIELDS, "B-154"))
        record["capability_evidence"]["byok_kill_switch"]["status"] = "PASS"

        def read_capability(path, fields, label):
            return record if path == MODULE.B154_EVIDENCE_PATH else original(path, fields, label)

        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_capability):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-154")

        record = copy.deepcopy(original(MODULE.B154_EVIDENCE_PATH, MODULE.B154_EVIDENCE_REQUIRED_FIELDS, "B-154"))
        record["surfaces"][0]["evidence_reference"] = "legal/dpa/v1.0.0.en-US.md:110; sha256 " + "0" * 64

        def read_reference(path, fields, label):
            return record if path == MODULE.B154_EVIDENCE_PATH else original(path, fields, label)

        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_reference):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-154")

        record = copy.deepcopy(original(MODULE.B154_EVIDENCE_PATH, MODULE.B154_EVIDENCE_REQUIRED_FIELDS, "B-154"))
        record["signed_documents"][0]["sha256"] = "0" * 64

        def read_signed_hash(path, fields, label):
            return record if path == MODULE.B154_EVIDENCE_PATH else original(path, fields, label)

        with mock.patch.object(MODULE, "_read_json_evidence", side_effect=read_signed_hash):
            with self.assertRaises(MODULE.PacketError):
                MODULE.check_data(self.data, "B-154")

    def test_base_sha_provenance_disclaimer_is_mandatory(self) -> None:
        missing = copy.deepcopy(self.data)
        missing["non_claim"] = "No claims."
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(missing)

        wrong_reference = copy.deepcopy(self.data)
        wrong_reference["base_sha"] = "0" * 40
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(wrong_reference)

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
