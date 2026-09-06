"""Mutation-backed static contract for B-258 checkout promo coverage."""

from __future__ import annotations

from pathlib import Path

import pytest

import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_b258_checkout_promo as verifier  # noqa: E402


def _sources() -> tuple[str, str]:
    return (
        (ROOT / verifier.RUNTIME).read_text(encoding="utf-8"),
        (ROOT / verifier.TEST).read_text(encoding="utf-8"),
    )


def test_b258_contract_is_green() -> None:
    verifier.verify(ROOT)


def _baited_mutation(source: str, needle: str, replacement: str) -> str:
    mutant = source.replace(needle, replacement, 1)
    assert mutant != source
    normal_bait = needle.replace("\\", "\\\\").replace('"', '\\"')
    return (
        mutant
        + "\n// bait must not count: "
        + needle
        + "\nconst B258_NORMAL_BAIT: &str = \""
        + normal_bait
        + "\";"
        + "\nconst B258_BAIT: &str = r#\""
        + needle
        + "\"#;\n"
    )


@pytest.mark.parametrize(
    ("label", "needle", "replacement"),
    (
        ("missing customer mock", '.and(path("/v1/customers"))', '.and(path("/v1/missing"))'),
        ("wrong checkout endpoint", '.and(path("/v1/checkout/sessions"))', '.and(path("/v1/checkout"))'),
        ("selecting wrong request", 'String::from_utf8(received[1].body.clone())', 'String::from_utf8(received[0].body.clone())'),
    ),
)
def test_b258_mutations_are_red(label: str, needle: str, replacement: str) -> None:
    runtime, test = _sources()
    mutant = _baited_mutation(test, needle, replacement)
    with pytest.raises(verifier.ContractError):
        verifier.verify_sources(runtime, mutant)
