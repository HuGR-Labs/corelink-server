from pathlib import Path

from scripts import verify_f008_npm_storage_cap as contract


ROOT = Path(__file__).resolve().parents[1]


def test_f008_npm_storage_cap_contract_and_mutations():
    contract.self_test(ROOT)
