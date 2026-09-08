from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b325_byok_wiring_test_results as verify


def source() -> str:
    return (verify.ROOT / verify.TARGET).read_text(encoding="utf-8")


def rejects(mutated: str) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.TARGET: mutated})


def test_current_tree_and_embedded_mutations_pass():
    verify.verify()
    verify.self_test()


def test_expect_reintroduction_fails_closed():
    rejects(source().replace("DekCache::new(300)?", 'DekCache::new(300).expect("valid")', 1))


def test_missing_result_return_fails_closed():
    rejects(source().replace("-> TestResult", "", 1))


def test_helper_result_erasure_fails_closed():
    rejects(source().replace("Result<RevocationDetector, BYOKError>", "RevocationDetector", 1))
