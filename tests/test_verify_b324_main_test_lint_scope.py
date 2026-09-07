from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b324_main_test_lint_scope as verify


def source() -> str:
    return (verify.ROOT / verify.TARGET).read_text(encoding="utf-8")


def rejects(mutated: str) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.TARGET: mutated})


def test_current_tree_and_embedded_mutations_pass():
    verify.verify()
    verify.self_test()


def test_duplicate_allowance_fails_closed():
    rejects(verify.ALLOWANCE + "\n" + source())


def test_allowance_deletion_fails_closed():
    rejects(source().replace(verify.ALLOWANCE + "\n", "", 1))
