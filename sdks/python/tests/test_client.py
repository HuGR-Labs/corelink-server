"""Test suite for CoreLinkClient — MVP operations.

Covers the 3 MVP operationIds from openapi-corelink-v1.yaml:
  - apiHealth  (GET /api/health)
  - patIssue   (POST /v1/pats)
  - signup     (POST /v1/signup)

Uses pytest-httpx to mock all HTTP traffic; no real network calls.
"""

from __future__ import annotations

import pytest
from pytest_httpx import HTTPXMock

from corelink import (
    CoreLinkAuthError,
    CoreLinkClient,
    CoreLinkRequestError,
    CoreLinkServerError,
    HealthStatus,
    PatIssueRequest,
    SignupRequest,
)

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

_TEST_PAT = "ct_test_00000000000000000000000000000000"
# Canonical flat prod host — the SDK default (the dotted
# `api.corelink.humangr.com` never resolved; `*.corelink.humangr.com` is dead).
_BASE_URL = "https://corelink-api.humangr.com"

_HEALTH_OK = {"status": "SERVING", "version": "0.1.0", "commit": "abc1234"}

_PAT_SHOWN_ONCE = {
    "pat_id": "11111111-1111-1111-1111-111111111111",
    "label": "ci-runner",
    "scopes": ["cas.read", "cas.write"],
    "created_at_ms": 1716739200000,
    "prefix": "ct_test_0",
    "shown_once_token": "ct_test_shown_once_material",
}

_SIGNUP_RESPONSE = {
    "signup_id": "22222222-2222-2222-2222-222222222222",
    "tenant_id": "33333333-3333-3333-3333-333333333333",
    "outcome": "Provisioned",
    "billing_intent": "Linked",
    "first_pat": _PAT_SHOWN_ONCE,
    "primary_region": "Enam",
}


@pytest.fixture()
def client() -> CoreLinkClient:
    """A CoreLinkClient pointed at the production URL with a test PAT."""
    return CoreLinkClient(pat=_TEST_PAT)


# ---------------------------------------------------------------------------
# Test 1: apiHealth — happy path
# ---------------------------------------------------------------------------


def test_get_health_returns_serving(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    """GET /api/health → 200 SERVING is parsed into HealthResponse."""
    httpx_mock.add_response(
        method="GET",
        url=f"{_BASE_URL}/api/health",
        json=_HEALTH_OK,
        status_code=200,
    )
    result = client.get_health()
    assert result.status == HealthStatus.SERVING
    assert result.version == "0.1.0"
    assert result.commit == "abc1234"


# ---------------------------------------------------------------------------
# Test 2: apiHealth — server error surfaces as CoreLinkServerError
# ---------------------------------------------------------------------------


def test_get_health_server_error_raises(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    """GET /api/health → 503 raises CoreLinkServerError."""
    httpx_mock.add_response(
        method="GET",
        url=f"{_BASE_URL}/api/health",
        status_code=503,
        text="service unavailable",
    )
    with pytest.raises(CoreLinkServerError) as exc_info:
        client.get_health()
    assert exc_info.value.status_code == 503


# ---------------------------------------------------------------------------
# Test 3: patIssue — happy path
# ---------------------------------------------------------------------------


def test_issue_pat_returns_shown_once_token(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    """POST /v1/pats → 201 returns PatIssueResponse with shown_once_token."""
    httpx_mock.add_response(
        method="POST",
        url=f"{_BASE_URL}/v1/pats",
        json=_PAT_SHOWN_ONCE,
        status_code=201,
    )
    req = PatIssueRequest(label="ci-runner", scopes=["cas.read", "cas.write"])
    result = client.issue_pat(req)
    assert result.shown_once_token == "ct_test_shown_once_material"
    assert result.label == "ci-runner"


# ---------------------------------------------------------------------------
# Test 4: patIssue — 401 raises CoreLinkAuthError
# ---------------------------------------------------------------------------


def test_issue_pat_auth_error_raises(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    """POST /v1/pats → 401 raises CoreLinkAuthError."""
    httpx_mock.add_response(
        method="POST",
        url=f"{_BASE_URL}/v1/pats",
        json={"error": {"code": "COR_UNAUTHORIZED", "message": "Invalid PAT"}},
        status_code=401,
    )
    req = PatIssueRequest(label="bad-token")
    with pytest.raises(CoreLinkAuthError):
        client.issue_pat(req)


# ---------------------------------------------------------------------------
# Test 5: signup — happy path
# ---------------------------------------------------------------------------


def test_signup_provisioned_outcome(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    """POST /v1/signup → 201 returns SignupResponse with Provisioned outcome."""
    httpx_mock.add_response(
        method="POST",
        url=f"{_BASE_URL}/v1/signup",
        json=_SIGNUP_RESPONSE,
        status_code=201,
    )
    req = SignupRequest(
        clerk_event_id="evt_abc123",
        email_hash="a" * 64,
        locale="en-US",
        idempotency_key="idem-test-001",
        correlation_id="evt_abc123",
    )
    result = client.signup(req)
    assert result.outcome.value == "Provisioned"
    assert result.tenant_id == "33333333-3333-3333-3333-333333333333"
    assert result.first_pat.shown_once_token == "ct_test_shown_once_material"


# ---------------------------------------------------------------------------
# Test 6: signup — 409 Conflict raises CoreLinkRequestError
# ---------------------------------------------------------------------------


def test_signup_conflict_raises_request_error(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    """POST /v1/signup → 409 raises CoreLinkRequestError with error_code."""
    httpx_mock.add_response(
        method="POST",
        url=f"{_BASE_URL}/v1/signup",
        json={"error": {"code": "COR_TENANT_ALREADY_EXISTS", "message": "Duplicate"}},
        status_code=409,
    )
    req = SignupRequest(
        clerk_event_id="evt_dup",
        email_hash="b" * 64,
        locale="en-US",
        idempotency_key="idem-test-dup",
        correlation_id="evt_dup",
    )
    with pytest.raises(CoreLinkRequestError) as exc_info:
        client.signup(req)
    assert exc_info.value.status_code == 409
    assert exc_info.value.error_code == "COR_TENANT_ALREADY_EXISTS"


# ---------------------------------------------------------------------------
# Test 7: missing PAT raises ValueError at construction time
# ---------------------------------------------------------------------------


def test_client_missing_pat_raises_value_error(monkeypatch: pytest.MonkeyPatch) -> None:
    """CoreLinkClient raises ValueError when no PAT is provided."""
    monkeypatch.delenv("CORELINK_PAT", raising=False)
    with pytest.raises(ValueError, match="PAT must be supplied"):
        CoreLinkClient()


# ---------------------------------------------------------------------------
# Test 8: context manager closes cleanly
# ---------------------------------------------------------------------------


def test_client_context_manager(httpx_mock: HTTPXMock) -> None:
    """CoreLinkClient used as a context manager closes without error."""
    httpx_mock.add_response(
        method="GET",
        url=f"{_BASE_URL}/api/health",
        json=_HEALTH_OK,
        status_code=200,
    )
    with CoreLinkClient(pat=_TEST_PAT) as cl:
        health = cl.get_health()
    assert health.status == HealthStatus.SERVING


# ---------------------------------------------------------------------------
# Test 9 (adversarial): signup — 422 Unprocessable raises CoreLinkRequestError
# ---------------------------------------------------------------------------


def test_negative_signup_unprocessable_entity(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    """Adversarial: POST /v1/signup → 422 surfaces as CoreLinkRequestError."""
    httpx_mock.add_response(
        method="POST",
        url=f"{_BASE_URL}/v1/signup",
        json={
            "error": {
                "code": "COR_VALIDATION_FAILED",
                "message": "email_hash must be 64 hex chars",
            }
        },
        status_code=422,
    )
    req = SignupRequest(
        clerk_event_id="evt_bad",
        email_hash="c" * 64,
        locale="en-US",
        idempotency_key="idem-bad",
        correlation_id="evt_bad",
    )
    with pytest.raises(CoreLinkRequestError) as exc_info:
        client.signup(req)
    assert exc_info.value.status_code == 422
    assert exc_info.value.error_code == "COR_VALIDATION_FAILED"
