"""HTTP-status → SDK-exception mapping shared by the sync and async clients.

Operates on an already-materialized ``httpx.Response`` (status + parsed body),
so the same mapping serves both transports with no drift.
"""

from __future__ import annotations

from typing import Any

import httpx

from .exceptions import (
    CoreLinkAuthError,
    CoreLinkDigestMismatchError,
    CoreLinkNotFoundError,
    CoreLinkQuotaError,
    CoreLinkRequestError,
    CoreLinkServerError,
)


def _message_and_code(response: httpx.Response) -> tuple[str, str | None]:
    """Extract ``(message, error_code)`` from a CoreLink error envelope,
    falling back to the raw text for the plain-text CAS routes."""
    try:
        body: dict[str, Any] = response.json()
        error = body.get("error", {})
        return error.get("message", response.text), error.get("code")
    except Exception:
        return response.text or f"HTTP {response.status_code}", None


def raise_for_control_status(response: httpx.Response) -> None:
    """Raise for the control-plane (JSON-envelope) routes. No-op on 2xx.

    401 → auth; other 4xx → request (with taxonomy ``error_code``); 5xx → server.
    """
    if response.is_success:
        return
    message, error_code = _message_and_code(response)
    if response.status_code == 401:
        raise CoreLinkAuthError(message)
    if 400 <= response.status_code < 500:
        raise CoreLinkRequestError(
            message, status_code=response.status_code, error_code=error_code
        )
    raise CoreLinkServerError(message, status_code=response.status_code)


def raise_for_cas_status(response: httpx.Response) -> None:
    """Raise for the CAS data-plane routes (plain-text taxonomy). No-op on 2xx.

    Mirrors ``crates/corelink-container/src/routes/cas.rs`` ``map_err``:
    404/410 → not-found, 422 → digest-mismatch, 402 → quota; everything else
    delegates to :func:`raise_for_control_status`.
    """
    if response.is_success:
        return
    if response.status_code == 404:
        raise CoreLinkNotFoundError()
    if response.status_code == 410:
        raise CoreLinkNotFoundError("blob erased (410 Gone)")
    if response.status_code == 422:
        raise CoreLinkDigestMismatchError(
            "server rejected write: claimed digest != blake3(body)"
        )
    if response.status_code == 402:
        raise CoreLinkQuotaError()
    raise_for_control_status(response)
