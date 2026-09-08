#!/usr/bin/env python3
"""Focused fail-closed tests for the bounded owner-action packet."""

from __future__ import annotations

import copy
import importlib.util
import json
import subprocess
import sys
import unittest
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
