from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b321_alert_delivery_tests as verify


def source() -> str:
    return (verify.ROOT / verify.TARGET).read_text(encoding="utf-8")


def rejects(mutated: str) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.TARGET: mutated})


def test_current_tree_and_embedded_mutations_pass():
    verify.verify()
    verify.self_test()


def test_alert_result_binding_deletion_fails_closed():
    current = source()
    rejects(current.replace("let result = alerter.alert(payload()).await;", "let result = Ok(());", 1))


def test_recovery_success_assertion_deletion_fails_closed():
    current = source()
    first = current.find("result.is_ok()")
    second = current.find("result.is_ok()", first + 1)
    assert second > first
    rejects(current[:second] + "true" + current[second + len("result.is_ok()") :])


def test_unwrap_suppression_fails_closed():
    rejects("#[allow(clippy::unwrap_used)]\n" + source())
