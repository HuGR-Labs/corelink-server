"""Mutation coverage for the frozen D03 graduation register."""

from scripts.verify_d03_graduation import self_test, verify_document


def test_closed_population_and_inverted_guards() -> None:
    result = verify_document(run_guards=True, run_gates=False)
    assert result == {"original": 42, "graduated": 40, "done": 2, "parked": 38}


def test_register_mutations_are_red() -> None:
    self_test()
