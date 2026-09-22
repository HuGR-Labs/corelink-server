import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "scripts/verify_i1635_consumer_contract.py"
SPEC = importlib.util.spec_from_file_location("i1635_contract", SCRIPT)
contract = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(contract)

KEY_A = "a" * 64
KEY_B = "b" * 64


def record(key, tenant="tenant"):
    return {"tenant_id": tenant, "idem_key": key}


def outcome(index, key, name, reason=None):
    item = {"index": index, "idem_key": key, "outcome": name}
    if reason is not None:
        item["reason"] = reason
    return item


def response(items, accepted=0, deduped=0, rejected=0):
    return {
        "outcomes": items,
        "accepted": accepted,
        "deduped": deduped,
        "rejected": rejected,
        "total": accepted + deduped,
    }


class ConsumerContractTests(unittest.TestCase):
    def test_server_shaped_outcomes_and_http_statuses(self):
        cases = [
            (202, [record(KEY_A)], response([outcome(0, KEY_A, "accepted")], accepted=1)),
            (202, [record(KEY_A)], response([outcome(0, KEY_A, "deduped")], deduped=1)),
            (202, [record(KEY_A), record(KEY_B, "invalid")], response([
                outcome(0, KEY_A, "accepted"),
                outcome(1, KEY_B, "rejected", "bad_tenant_id"),
            ], accepted=1, rejected=1)),
            (409, [record(KEY_A)], response([outcome(0, KEY_A, "conflict", "payload_mismatch")])),
            (422, [record(KEY_A, "invalid")], response([
                outcome(0, KEY_A, "rejected", "bad_tenant_id"),
            ], rejected=1)),
            (422, [{"idem_key": KEY_A}], response([
                outcome(0, KEY_A, "rejected", "invalid_record"),
            ], rejected=1)),
        ]
        for status, request, ack in cases:
            with self.subTest(status=status, ack=ack):
                contract.validate(status, ack, request)
        contract.validate_no_body(503, None)

    def test_legacy_schema_is_rejected(self):
        legacy = {
            "schema_version": 1,
            "batch_id": "batch",
            "outcomes": [{"event_id": "event", "status": "accepted", "reason": "stored"}],
            "settlement_event_ids": ["event"],
        }
        with self.assertRaises(contract.ContractError):
            contract.validate(202, legacy, [record(KEY_A)])

    def test_unknown_keys_and_outcome_names_are_rejected(self):
        ack = response([outcome(0, KEY_A, "accepted")], accepted=1)
        ack["schema_version"] = 1
        with self.assertRaises(contract.ContractError):
            contract.validate(202, ack, [record(KEY_A)])
        ack = response([outcome(0, KEY_A, "accepted")], accepted=1)
        ack["outcomes"][0]["event_id"] = "fiction"
        with self.assertRaises(contract.ContractError):
            contract.validate(202, ack, [record(KEY_A)])
        ack = response([outcome(0, KEY_A, "conflicting", "payload_mismatch")])
        with self.assertRaises(contract.ContractError):
            contract.validate(409, ack, [record(KEY_A)])

    def test_index_key_reason_and_counter_mutations_are_rejected(self):
        request = [record(KEY_A), record(KEY_B, "invalid")]
        valid = response([
            outcome(0, KEY_A, "accepted"),
            outcome(1, KEY_B, "rejected", "bad_tenant_id"),
        ], accepted=1, rejected=1)
        mutations = []
        changed = {**valid, "outcomes": [dict(x) for x in valid["outcomes"]]}
        changed["outcomes"][0]["index"] = 1
        mutations.append(changed)
        changed = {**valid, "outcomes": [dict(x) for x in valid["outcomes"]]}
        changed["outcomes"][0]["idem_key"] = KEY_B
        mutations.append(changed)
        changed = {**valid, "outcomes": [dict(x) for x in valid["outcomes"]]}
        changed["outcomes"][0]["reason"] = "stored"
        mutations.append(changed)
        changed = {**valid, "outcomes": [dict(x) for x in valid["outcomes"]]}
        del changed["outcomes"][1]["reason"]
        mutations.append(changed)
        changed = {**valid, "outcomes": [dict(x) for x in valid["outcomes"]]}
        changed["outcomes"][1]["reason"] = "  "
        mutations.append(changed)
        changed = {**valid, "outcomes": [dict(x) for x in valid["outcomes"]]}
        changed["outcomes"][1]["reason"] = "not_a_server_reason"
        mutations.append(changed)
        changed = {**valid, "outcomes": [dict(x) for x in valid["outcomes"]]}
        changed["accepted"] = 2
        mutations.append(changed)
        changed = {**valid, "outcomes": [dict(x) for x in valid["outcomes"]]}
        changed["total"] = 0
        mutations.append(changed)
        for ack in mutations:
            with self.subTest(ack=ack), self.assertRaises(contract.ContractError):
                contract.validate(202, ack, request)

    def test_status_mismatch_and_bodyful_503_are_rejected(self):
        ack = response([outcome(0, KEY_A, "accepted")], accepted=1)
        with self.assertRaises(contract.ContractError):
            contract.validate(422, ack, [record(KEY_A)])
        with self.assertRaises(contract.ContractError):
            contract.validate_no_body(503, "{}")


if __name__ == "__main__":
    unittest.main()
