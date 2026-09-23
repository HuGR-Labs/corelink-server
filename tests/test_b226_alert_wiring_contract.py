"""Adversarial focal contract tests for B-226."""

from __future__ import annotations

import re
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_b226_alert_wiring as verifier  # noqa: E402


def _sources() -> tuple[str, str, str]:
    return (
        (ROOT / verifier.ALERTER).read_text(encoding="utf-8"),
        (ROOT / verifier.CHANNEL).read_text(encoding="utf-8"),
        (ROOT / verifier.CONFIG).read_text(encoding="utf-8"),
    )


def test_b226_contract_is_green() -> None:
    verifier.verify(ROOT)


@pytest.mark.parametrize(
    ("label", "needle", "replacement"),
    (
        (
            "drop dashboard dispatch",
            "self\n            .dispatch_channel(AlertChannel::Dashboard, envelope)\n            .await",
            "self.dispatch_channel(AlertChannel::Email, envelope).await",
        ),
        (
            "bypass provider",
            "self.transport.deliver(channel, envelope.clone()).await",
            "self.transport.disabled(channel, envelope.clone()).await",
        ),
        (
            "claim failed recovery success",
            "Err(RevocationError::AlertDeliveryFailed {\n                kms_key_id: kms_key_id.to_string(),",
            "Ok(())",
        ),
    ),
)
def test_b226_mutations_are_red(label: str, needle: str, replacement: str) -> None:
    alerter, channel, config = _sources()
    mutant = alerter.replace(needle, replacement, 1)
    assert mutant != alerter, label
    with pytest.raises(verifier.ContractError):
        verifier.verify_sources(mutant, channel, config)


def test_b226_channel_display_mutation_is_red() -> None:
    alerter, channel, config = _sources()
    mutant = channel.replace(
        "impl std::fmt::Display for AlertChannel",
        "impl std::fmt::Display for RemovedAlertChannel",
        1,
    )
    assert mutant != channel
    with pytest.raises(verifier.ContractError):
        verifier.verify_sources(alerter, mutant, config)


def test_b226_thiserror_channel_field_mutation_is_red() -> None:
    alerter, channel, config = _sources()
    mutant, replacements = re.subn(
        r"(NotConfigured\s*\{.*?)(channel:\s*AlertChannel,)",
        r"\1removed_channel: AlertChannel,",
        channel,
        count=1,
        flags=re.DOTALL,
    )
    assert replacements == 1
    with pytest.raises(verifier.ContractError):
        verifier.verify_sources(alerter, mutant, config)


def test_b226_channel_removal_is_red_even_when_comment_bait_repeats_the_variant() -> None:
    alerter, channel, config = _sources()
    mutant = channel.replace("    Dashboard,", "    // Dashboard,", 1)
    assert mutant != channel
    with pytest.raises(verifier.ContractError):
        verifier.verify_sources(alerter, mutant, config)


def test_b226_dispatch_comment_bait_is_not_executable_wiring() -> None:
    alerter, channel, config = _sources()
    needle = "self\n            .dispatch_channel(AlertChannel::Dashboard, envelope)\n            .await"
    mutant = alerter.replace(needle, "// " + needle, 1)
    assert mutant != alerter
    with pytest.raises(verifier.ContractError):
        verifier.verify_sources(mutant, channel, config)


def test_b226_dispatch_string_bait_is_not_executable_wiring() -> None:
    alerter, channel, config = _sources()
    needle = "self.dispatch_channel(AlertChannel::Email, envelope).await"
    bait = needle.replace("\\", "\\\\").replace("\n", "\\n").replace('"', '\\"')
    mutant = alerter.replace(needle, 'let _bait = "' + bait + '";', 1)
    assert mutant != alerter
    with pytest.raises(verifier.ContractError):
        verifier.verify_sources(mutant, channel, config)


def test_b226_receipt_removal_is_red() -> None:
    alerter, channel, config = _sources()
    mutant = alerter.replace(
        "Ok(receipt) => self.receipt_outcome(receipt)",
        'Ok(_receipt) => ChannelOutcome::Success("accepted".to_string())',
        1,
    )
    assert mutant != alerter
    with pytest.raises(verifier.ContractError):
        verifier.verify_sources(mutant, channel, config)
