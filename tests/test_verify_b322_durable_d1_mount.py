from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b322_durable_d1_mount as verify


def source() -> str:
    return (verify.ROOT / verify.TARGET).read_text(encoding="utf-8")


def rejects(mutated: str) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.TARGET: mutated})


def test_current_tree_and_embedded_mutations_pass():
    verify.verify()
    verify.self_test()


def test_discarded_client_binding_fails_closed():
    rejects(source().replace("Some(client)", "Some(_)", 1))


def test_guarded_expect_regression_fails_closed():
    current = source()
    rejects(current.replace("Some(client)", "Some(_)", 1).replace(
        'info!("billing: DURABLE D1 webhook-DLQ store wired (migrations 0045+0094)");',
        'let client = d1_client.as_ref().expect("guarded by durable D1 mount");',
        1,
    ))


def test_expect_suppression_fails_closed():
    rejects("#[allow(clippy::expect_used)]\n" + source())
