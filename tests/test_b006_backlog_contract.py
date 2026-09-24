from __future__ import annotations

import json
import re
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

import pytest

from scripts.collect_b006_metrics import KEYCHAIN_ACCOUNT, KEYCHAIN_SERVICE, RejectRedirect, SOURCE, collect
from scripts import collect_b006_provider_binding as provider_collector
from scripts.verify_b006_evidence import EvidenceError, validate_closure, validate_receipt
from scripts.verify_b006_provider_binding import EXPECTED_ACCOUNT_ID, EXPECTED_AUTH_EMAIL, EXPECTED_AUTH_TYPE, EXPECTED_DEPLOYMENT_ID, EXPECTED_SCRIPT_ETAG, EXPECTED_VERSION_ID, EXPECTED_VERSION_NUMBER, ProviderBindingError, validate_provider_binding
from scripts.verify_d03_graduation import GraduationError, _check_packets, _load_packets


ROOT = Path(__file__).resolve().parents[1]
BACKLOG = ROOT / "BACKLOG.md"
EVIDENCE = ROOT / "docs/validation/evidence/b006-capability-claim-unserved-2026-09-08.json"
PACKET = ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json"


def b006_block() -> str:
    for block in re.findall(r"```backlog\n(.*?)^```", BACKLOG.read_text(encoding="utf-8"), re.MULTILINE | re.DOTALL):
        if re.search(r"^id:\s*B-006\s*$", block, re.MULTILINE):
            return block
    raise AssertionError("B-006 backlog block is missing")


def test_b006_uses_dedicated_observability_header_and_redacts_labels() -> None:
    block = b006_block()
    assert "status: done" in block
    assert "https://corelink-spawn-worker.gmhelmold.workers.dev/internal/v1/metrics" in block
    assert "METRICS_OBSERVABILITY_KEY" in block
    assert "X-Corelink-Internal-Auth" in block
    assert "corelink-b006-probe/1.0" in block
    assert "wrong credential class" in block
    assert "All labels" in block
    assert "tenant identifiers" in block
    assert "customer identifiers" in block
    assert "capability_claim_unserved > 0" in block
    assert "HTTP 200" in block
    assert "capability_claim_unserved = 0" in block


def test_b006_does_not_publish_a_bearer_probe_command() -> None:
    block = b006_block()
    assert "curl --fail" not in block
    assert "CORELINK_PROD_TOKEN" in block
    packet = json.loads(PACKET.read_text(encoding="utf-8"))["packets"]["B-006"]
    command = packet["command"]
    assert packet["disposition"] == "DONE"
    assert "scripts/collect_b006_metrics.py" in command
    assert "scripts/verify_b006_evidence.py" in command
    assert "--auth-header X-Corelink-Internal-Auth" in command
    assert "--keychain-service 'CoreLink/METRICS_OBSERVABILITY_KEY'" in command
    assert "--keychain-account corelink-ops" in command
    assert "--timeout 10" in command
    assert "--max-bytes 1048576" in command
    assert "Authorization: Bearer" not in command


def test_b006_evidence_retains_redacted_403_and_no_guessed_counter() -> None:
    evidence = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    validate_receipt(evidence, mode="historical")
    assert evidence["schema"] == "corelink-b006-capability-metrics-v2"
    assert evidence["http_status"] == 403
    assert evidence["authenticated"] is False
    assert evidence["aggregate_only"] is False
    assert evidence["labels_included"] is False
    assert evidence["capability_claim_unserved"] is None


def test_b006_historical_fixture_ignores_age_while_live_receipts_remain_fresh() -> None:
    evidence = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    stale = {
        **evidence,
        "captured_at": "2020-01-01T00:00:00Z",
    }

    # The committed 403 receipt remains useful evidence of the redacted,
    # fail-closed contract after its observation window has expired.
    validate_receipt(stale, mode="historical")

    # The same data cannot stand in for a current production observation.
    with pytest.raises(EvidenceError, match="receipt timestamp is stale or from the future"):
        validate_receipt(stale, mode="live")

    future = {**evidence, "captured_at": "2999-01-01T00:00:00Z"}
    with pytest.raises(EvidenceError, match="receipt timestamp is stale or from the future"):
        validate_receipt(future, mode="historical")


def test_b006_committed_closure_is_static_but_live_validation_stays_fresh() -> None:
    metrics = json.loads((ROOT / "artifacts/d03/B006-capability-metrics.json").read_text(encoding="utf-8"))
    provider = json.loads((ROOT / "artifacts/d03/B006-provider-binding.json").read_text(encoding="utf-8"))

    validate_closure(metrics, provider, mode="historical")
    with pytest.raises(EvidenceError, match="receipt timestamp is stale or from the future"):
        validate_closure(metrics, provider)

    # The D03 repository verifier consumes these committed receipts statically.
    _check_packets(_load_packets(PACKET.read_text(encoding="utf-8")), ROOT)


def test_b006_rejects_unauthenticated_zero_and_receipt_boundary_mutations() -> None:
    evidence = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    mutations = (
        {"capability_claim_unserved": 0},
        {"authenticated": True},
        {"aggregate_only": True},
        {"labels_included": True},
        {"keychain_service": "CoreLink/OTHER_KEY"},
        {"keychain_account": "alternate-operator"},
        {"source": "https://evil.example/metrics"},
        {"captured_at": "2020-01-01T00:00:00Z"},
        {"secret": "must-never-be-retained"},
    )
    for mutation in mutations:
        candidate = {**evidence, **mutation}
        with pytest.raises(EvidenceError):
            validate_receipt(candidate)


def test_b006_rejects_redirects_before_a_credential_can_leave_origin() -> None:
    request = urllib.request.Request(SOURCE, method="GET")
    with pytest.raises(urllib.error.HTTPError) as raised:
        RejectRedirect().redirect_request(request, None, 302, "Found", {}, "https://evil.example/steal")
    assert raised.value.code == 302
    assert raised.value.url == SOURCE


def test_b006_collector_pins_url_and_keychain_identity_without_requesting() -> None:
    wrong_keychain = collect(SOURCE, "CoreLink/OTHER_KEY", KEYCHAIN_ACCOUNT, 1, 1024)
    assert wrong_keychain["request_attempted"] is False
    assert wrong_keychain["http_status"] is None
    wrong_url = collect("https://evil.example/metrics", KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, 1, 1024)
    assert wrong_url["request_attempted"] is False
    assert wrong_url["http_status"] is None


def test_b006_positive_closure_requires_both_fresh_zero_and_provider_binding() -> None:
    stamp = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    metrics = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    metrics.update(
        captured_at=stamp,
        verdict="INDETERMINATE",
        reason="authenticated aggregate snapshot retained; separate Wrangler deployment evidence is required for closure",
        http_status=200,
        authenticated=True,
        aggregate_only=True,
        capability_claim_unserved=0,
    )
    provider = json.loads((ROOT / "artifacts/d03/B006-provider-binding.json").read_text(encoding="utf-8"))
    assert "source_sha" not in provider
    assert provider["script_etag"] == EXPECTED_SCRIPT_ETAG
    provider["captured_at"] = stamp
    validate_closure(metrics, provider)


def test_b006_closure_rejects_cross_binding_freshness_and_status_mutations() -> None:
    stamp = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    metrics = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    metrics.update(
        captured_at=stamp,
        verdict="INDETERMINATE",
        reason="authenticated aggregate snapshot retained; separate Wrangler deployment evidence is required for closure",
        http_status=200,
        authenticated=True,
        aggregate_only=True,
        capability_claim_unserved=0,
    )
    provider = json.loads((ROOT / "artifacts/d03/B006-provider-binding.json").read_text(encoding="utf-8"))
    provider["captured_at"] = stamp
    mutations = (
        (metrics, {"http_status": 403, "reason": "authenticated aggregate snapshot retained; separate Wrangler deployment evidence is required for closure"}),
        (metrics, {"http_status": 403, "reason": "Keychain item unavailable; no production request was attempted", "request_attempted": False}),
        (metrics, {"request_attempted": False}),
        (provider, {"version_id": "1418af47-d71a-488f-89a6-cbb9173402bd"}),
        (provider, {"script_etag": "0" * 64}),
        (provider, {"metrics_source": "https://evil.example/metrics"}),
        (provider, {"rollout_percentage": 50}),
        (provider, {"secret": "must-never-be-retained"}),
        (provider, {"auth_identity": {"account_id": "wrong", "email": EXPECTED_AUTH_EMAIL, "auth_type": EXPECTED_AUTH_TYPE, "logged_in": True}}),
        (provider, {"wrangler_version": "4.130.0"}),
        (provider, {"wrangler_package_integrity": "sha512-substituted"}),
        (provider, {"wrangler_lock_source": "package.json"}),
        (provider, {"deployment_api_success": False}),
        (provider, {"captured_at": "2020-01-01T00:00:00Z"}),
        (provider, {"captured_at": "2999-01-01T00:00:00Z"}),
    )
    for original, mutation in mutations:
        candidate = {**original, **mutation}
        with pytest.raises((EvidenceError, ProviderBindingError)):
            if original is metrics:
                validate_closure(candidate, provider)
            else:
                validate_closure(metrics, candidate)

    missing_digest = {**provider}
    missing_digest.pop("script_etag")
    with pytest.raises((EvidenceError, ProviderBindingError)):
        validate_closure(metrics, missing_digest)

    unauthenticated = {**provider, "authenticated": False}
    with pytest.raises((EvidenceError, ProviderBindingError)):
        validate_closure(metrics, unauthenticated)


def test_b006_provider_collector_uses_provider_digest_and_auth_response(monkeypatch: pytest.MonkeyPatch) -> None:
    identity = ({"loggedIn": True, "authType": EXPECTED_AUTH_TYPE, "email": EXPECTED_AUTH_EMAIL, "accounts": [{"id": EXPECTED_ACCOUNT_ID}]}, True)
    deployments = ([{"id": EXPECTED_DEPLOYMENT_ID, "versions": [{"version_id": EXPECTED_VERSION_ID, "percentage": 100}]}], True)
    version = ({"id": EXPECTED_VERSION_ID, "number": EXPECTED_VERSION_NUMBER, "resources": {"script": {"etag": EXPECTED_SCRIPT_ETAG}}}, True)
    responses = iter((identity, deployments, version))
    monkeypatch.setattr(provider_collector, "_wrangler_json", lambda *args: next(responses))
    receipt = provider_collector.collect(lambda *args: next(responses))
    assert receipt["authenticated"] is True
    assert receipt["script_etag"] == EXPECTED_SCRIPT_ETAG

    for bad_response in (
        ({"id": EXPECTED_VERSION_ID, "number": EXPECTED_VERSION_NUMBER, "resources": {"script": {}}}, True),
        ({"id": EXPECTED_VERSION_ID, "number": EXPECTED_VERSION_NUMBER, "resources": {"script": {"etag": "0" * 64}}}, True),
        (version[0], False),
    ):
        responses = iter((identity, deployments, bad_response))
        monkeypatch.setattr(provider_collector, "_wrangler_json", lambda *args: next(responses))
        with pytest.raises(ValueError):
            provider_collector.collect(lambda *args: next(responses))

    wrong_identity = ({"loggedIn": True, "authType": EXPECTED_AUTH_TYPE, "email": "other@example.invalid", "accounts": [{"id": EXPECTED_ACCOUNT_ID}]}, True)
    responses = iter((wrong_identity, deployments, version))
    monkeypatch.setattr(provider_collector, "_wrangler_json", lambda *args: next(responses))
    with pytest.raises(ValueError):
        provider_collector.collect(lambda *args: next(responses))


def test_b006_provider_collector_rejects_substituted_tarball_before_execution(monkeypatch: pytest.MonkeyPatch) -> None:
    class FakeResponse:
        def __enter__(self) -> "FakeResponse":
            return self

        def __exit__(self, *_args: object) -> None:
            return None

        def read(self, _limit: int) -> bytes:
            return b"substituted package bytes"

    monkeypatch.setattr(provider_collector.urllib.request, "urlopen", lambda *_args, **_kwargs: FakeResponse())
    with pytest.raises(ValueError, match="integrity"):
        with provider_collector._verified_wrangler():
            raise AssertionError("substituted package must never execute")


def test_b006_d03_packet_cannot_reopen_after_authenticated_zero_closure() -> None:
    packet = _load_packets(PACKET.read_text(encoding="utf-8"))
    packet["packets"]["B-006"]["disposition"] = "REOPENED"
    with pytest.raises(GraduationError, match="B-006"):
        _check_packets(packet, ROOT)
