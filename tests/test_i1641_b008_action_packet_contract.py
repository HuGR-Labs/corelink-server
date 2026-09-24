from __future__ import annotations

import copy
import importlib.util
import json
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


class B008OwnerActionContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.packet = json.loads(MODULE.PACKET.read_text(encoding="utf-8"))
        self.item = next(item for item in self.packet["items"] if item["id"] == "B-008")

    def test_b008_admission_and_receipt_contract_passes(self) -> None:
        self.assertEqual(MODULE.check_data(self.packet, "B-008"), {"items": 29, "population": 29})

    def test_b008_owner_contract_mutations_fail_closed(self) -> None:
        mutations = {
            "action type": lambda item: item.__setitem__("action_type", "external_read_credential_and_delivery_review"),
            "protected staging prerequisite": lambda item: item["procedure"].__setitem__(
                1, item["procedure"][1].replace("required approval protection", "approval protection", 1)
            ),
            "staging service binding": lambda item: item["procedure"].__setitem__(
                1, item["procedure"][1].replace("SCHEDULED_DRILL_DELIVERY", "MISSING_BINDING", 1)
            ),
            "one authorized event cap": lambda item: item["procedure"].__setitem__(
                2, item["procedure"][2].replace(
                    "one non-production root-worker scheduled tick",
                    "repeated non-production root-worker scheduled ticks",
                    1,
                )
            ),
            "read/write key separation": lambda item: item["inputs_and_credentials_boundary"].__setitem__(
                "credentials", item["inputs_and_credentials_boundary"]["credentials"].replace(
                    "separate read-only PagerDuty API key", "PagerDuty API key", 1
                )
            ),
            "D1 and webhook schema": lambda item: item["evidence"].__setitem__(
                "required_fields", [field for field in item["evidence"]["required_fields"] if field != "d1_receipt"]
            ),
            "MTTA field": lambda item: item["evidence"].__setitem__(
                "item_schema", item["evidence"]["item_schema"].replace("mtta_ms, ", "", 1)
            ),
            "scheduler disabled after one tick": lambda item: item["evidence"].__setitem__(
                "item_schema", item["evidence"]["item_schema"].replace("scheduled_tick_disabled_at, ", "", 1)
            ),
            "no retry boundary": lambda item: item.__setitem__(
                "retry_and_rollback", item["retry_and_rollback"].replace("do not retry or duplicate", "retry once", 1)
            ),
            "human-delivery postcondition": lambda item: item.__setitem__(
                "expected_postcondition", item["expected_postcondition"].replace("human delivery", "incident acceptance", 1)
            ),
            "canonical manifest reference": lambda item: item["references"].remove(
                "evidence/i1641/pagerduty-contract-manifest.json"
            ),
        }
        for name, mutate in mutations.items():
            with self.subTest(name=name):
                candidate = copy.deepcopy(self.packet)
                item = next(row for row in candidate["items"] if row["id"] == "B-008")
                mutate(item)
                with self.assertRaises(MODULE.PacketError):
                    MODULE.check_data(candidate, "B-008")

    def test_b008_cannot_be_marked_done_without_external_receipt(self) -> None:
        candidate = copy.deepcopy(self.packet)
        item = next(row for row in candidate["items"] if row["id"] == "B-008")
        item["status"] = "done"
        with self.assertRaises(MODULE.PacketError):
            MODULE.check_data(candidate, "B-008")

    def test_staging_service_binding_is_source_bound(self) -> None:
        topology = json.loads((ROOT / MODULE.B008_STAGING_TOPOLOGY_PATH).read_text(encoding="utf-8"))
        MODULE._check_b008_staging_binding(topology)
        mutated = copy.deepcopy(topology)
        bindings = mutated["cloudflare"]["service_bindings"]
        binding = next(row for row in bindings if row.get("binding") == MODULE.B008_STAGING_SERVICE_BINDING)
        binding["service"] = "corelink-synthetic-pager"
        with self.assertRaises(MODULE.PacketError):
            MODULE._check_b008_staging_binding(mutated)


if __name__ == "__main__":
    unittest.main()
