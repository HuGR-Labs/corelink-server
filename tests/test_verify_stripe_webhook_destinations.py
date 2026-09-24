"""Unit and adversarial coverage for the read-only Stripe destination verifier."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_stripe_webhook_destinations",
    ROOT / "scripts" / "verify_stripe_webhook_destinations.py",
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def v1_row(identifier: str, url: str, *, status: str = "enabled") -> dict[str, object]:
    return {
        "id": identifier,
        "status": status,
        "url": url,
        "enabled_events": ["customer.subscription.deleted"],
        "created": 1,
    }

def v2_row(identifier: str, url: str, *, status: str = "active") -> dict[str, object]:
    return {
        "id": identifier,
        "type": "webhook_endpoint",
        "status": status,
        "event_payload": "thin",
        "enabled_events": ["customer.subscription.updated"],
        "webhook_endpoint": {"url": url},
        "updated": "2026-09-22T00:00:00.000Z",
    }

def test_normalizes_v1_and_v2_into_one_safe_shape() -> None:
    v1 = MODULE.normalize_destination(v1_row("we_1234567890", "https://signup.humangr.com/stripe"), "v1")
    v2 = MODULE.normalize_destination(v2_row("ed_1234567890", "https://api.humangr.com/stripe"), "v2")
    assert (v1.api, v1.id, v1.url, v1.event_payload, v1.event_types) == (
        "v1",
        "we_1234567890",
        "https://signup.humangr.com/stripe",
        "snapshot",
        ("customer.subscription.deleted",),
    )
    assert (v2.api, v2.id, v2.url, v2.event_payload, v2.event_types) == (
        "v2",
        "ed_1234567890",
        "https://api.humangr.com/stripe",
        "thin",
        ("customer.subscription.updated",),
    )


def test_malformed_or_wrong_id_scheme_fails_closed() -> None:
    with pytest.raises(MODULE.VerificationError, match="unexpected id scheme"):
        MODULE.normalize_destination(v1_row("ed_wrong", "https://example.invalid"), "v1")
    with pytest.raises(MODULE.VerificationError, match="invalid enabled_events"):
        MODULE.normalize_destination({**v2_row("ed_1234567890", "https://example.invalid"), "enabled_events": None}, "v2")


def test_v1_and_v2_pagination_are_both_walked(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[list[str]] = []

    def fake_run(command: list[str], **_: object) -> subprocess.CompletedProcess[str]:
        calls.append(command)
        path = command[2]
        if path == "/v1/webhook_endpoints":
            if "starting_after=we_page_one" in command:
                payload = {"data": [v1_row("we_page_two", "https://two.humangr.com")], "has_more": False}
            else:
                payload = {"data": [v1_row("we_page_one", "https://one.humangr.com")], "has_more": True}
        else:
            if "page=v2_page_two" in command:
                payload = {"data": [v2_row("ed_page_two", "https://two.humangr.com")], "next_page_url": None}
            else:
                payload = {
                    "data": [v2_row("ed_page_one", "https://one.humangr.com")],
                    "next_page_url": "https://api.stripe.com/v2/core/event_destinations?page=v2_page_two",
                }
        return subprocess.CompletedProcess(command, 0, json.dumps(payload), "")

    monkeypatch.setattr(MODULE.subprocess, "run", fake_run)
    assert len(MODULE.list_v1("stripe", [])) == 2
    assert len(MODULE.list_v2("stripe", [])) == 2
    assert any("starting_after=we_page_one" in command for command in calls)
    assert any("page=v2_page_two" in command for command in calls)


@pytest.mark.parametrize("page", [{"data": []}, {"data": [], "has_more": "false"}])
def test_v1_missing_or_non_boolean_has_more_fails_closed(
    monkeypatch: pytest.MonkeyPatch, page: dict[str, object]
) -> None:
    monkeypatch.setattr(MODULE, "stripe_get", lambda *_: page)
    with pytest.raises(MODULE.VerificationError, match="v1 page has no boolean has_more"):
        MODULE.list_v1("stripe", [])


@pytest.mark.parametrize(
    ("next_url", "error"),
    [
        (
            "https://attacker.invalid/v2/core/event_destinations?page=next&include%5B0%5D=webhook_endpoint.url",
            "outside the expected API resource",
        ),
        (
            "https://api.stripe.com/v2/other?page=next&include%5B0%5D=webhook_endpoint.url",
            "outside the expected API resource",
        ),
        (
            "https://api.stripe.com/v2/core/event_destinations?page=next",
            "did not preserve the webhook URL include",
        ),
    ],
)
def test_v2_off_resource_or_incomplete_continuation_fails_closed(
    monkeypatch: pytest.MonkeyPatch, next_url: str, error: str
) -> None:
    monkeypatch.setattr(
        MODULE,
        "stripe_get",
        lambda *_: {"data": [], "next_page_url": next_url},
    )
    with pytest.raises(MODULE.VerificationError, match=error):
        MODULE.list_v2("stripe", [])


def test_v2_valid_continuation_preserves_the_page_token(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pages = iter(
        [
            {
                "data": [],
                "next_page_url": (
                    "https://api.stripe.com/v2/core/event_destinations?"
                    "page=cursor&include%5B0%5D=webhook_endpoint.url"
                ),
            },
            {"data": [], "next_page_url": None},
        ]
    )
    calls: list[list[str]] = []

    def fake_get(binary: str, path: str, flags: list[str]) -> dict[str, object]:
        calls.append(flags)
        return next(pages)

    monkeypatch.setattr(MODULE, "stripe_get", fake_get)
    assert MODULE.list_v2("stripe", []) == []
    assert "page=cursor" in calls[1]


def test_inventory_does_not_print_provider_destination_name(capsys: pytest.CaptureFixture[str]) -> None:
    row = MODULE.normalize_destination(
        {**v1_row("we_1234567890", "https://signup.humangr.com/stripe"), "name": "billing owner email"},
        "v1",
    )
    MODULE.print_inventory([row])
    assert "billing owner email" not in capsys.readouterr().out
