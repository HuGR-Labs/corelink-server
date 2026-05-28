"""CoreLink synchronous HTTP client.

Implements the 3 MVP operations derived from openapi-corelink-v1.yaml:

  Operation      | operationId | Method | Path
  -------------- | ----------- | ------ | ---------------------
  Health probe   | apiHealth   | GET    | /api/health
  Issue PAT      | patIssue    | POST   | /v1/pats
  Tenant signup  | signup      | POST   | /v1/signup

Auth: Bearer PAT via ``Authorization: Bearer <token>`` header
      (BearerPAT security scheme).

This client is sync-only (MVP scope). Async support is a future WP.
"""

from __future__ import annotations

import os
from typing import Any

import httpx

from .exceptions import CoreLinkAuthError, CoreLinkRequestError, CoreLinkServerError
from .types import (
    HealthResponse,
    PatIssueRequest,
    PatIssueResponse,
    SignupRequest,
    SignupResponse,
)

_DEFAULT_BASE_URL = "https://api.corelink.humangr.com"
_DEFAULT_TIMEOUT = 30.0


class CoreLinkClient:
    """Synchronous CoreLink REST API client.

    Parameters
    ----------
    pat:
        Personal Access Token.  If *None*, the value of the
        ``CORELINK_PAT`` environment variable is used.  Raises
        :class:`ValueError` if neither is provided.
    base_url:
        Override the server base URL (useful for staging / local dev).
        Defaults to ``https://api.corelink.humangr.com``.
    timeout:
        Request timeout in seconds.  Defaults to 30.

    Examples
    --------
    >>> client = CoreLinkClient(pat="ct_xxx")
    >>> health = client.get_health()
    >>> health.status.value
    'SERVING'
    """

    def __init__(
        self,
        pat: str | None = None,
        *,
        base_url: str = _DEFAULT_BASE_URL,
        timeout: float = _DEFAULT_TIMEOUT,
    ) -> None:
        resolved_pat = pat or os.environ.get("CORELINK_PAT")
        if not resolved_pat:
            raise ValueError(
                "CoreLink PAT must be supplied via `pat=` argument or "
                "the CORELINK_PAT environment variable."
            )
        self._pat: str = resolved_pat
        self._base_url: str = base_url.rstrip("/")
        self._http: httpx.Client = httpx.Client(
            base_url=self._base_url,
            headers={"Authorization": f"Bearer {self._pat}"},
            timeout=timeout,
        )

    # ------------------------------------------------------------------
    # Context-manager support
    # ------------------------------------------------------------------

    def __enter__(self) -> CoreLinkClient:
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()

    def close(self) -> None:
        """Close the underlying HTTP connection pool."""
        self._http.close()

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _raise_for_status(self, response: httpx.Response) -> None:
        """Raise an appropriate SDK exception based on HTTP status.

        Does nothing for 2xx responses.
        """
        if response.is_success:
            return

        error_code: str | None = None
        try:
            body: dict[str, Any] = response.json()
            error_code = body.get("error", {}).get("code")
            message: str = body.get("error", {}).get("message", response.text)
        except Exception:
            message = response.text or f"HTTP {response.status_code}"

        if response.status_code == 401:
            raise CoreLinkAuthError(message)
        if 400 <= response.status_code < 500:
            raise CoreLinkRequestError(
                message,
                status_code=response.status_code,
                error_code=error_code,
            )
        raise CoreLinkServerError(message, status_code=response.status_code)

    # ------------------------------------------------------------------
    # MVP operations
    # ------------------------------------------------------------------

    def get_health(self) -> HealthResponse:
        """Liveness / readiness probe.

        operationId: **apiHealth** — ``GET /api/health``

        Returns
        -------
        HealthResponse
            Server health status.  Does **not** require authentication.

        Raises
        ------
        CoreLinkServerError
            If the server returns a 5xx response.
        """
        response = self._http.get("/api/health")
        self._raise_for_status(response)
        return HealthResponse.model_validate(response.json())

    def issue_pat(self, request: PatIssueRequest) -> PatIssueResponse:
        """Issue a new Personal Access Token (shown once).

        operationId: **patIssue** — ``POST /v1/pats``

        Parameters
        ----------
        request:
            PAT creation parameters (label + optional scopes / expiry).

        Returns
        -------
        PatIssueResponse
            Newly issued PAT metadata **including** the raw token material
            (``shown_once_token``).  Store it securely — it will not be
            retrievable again.

        Raises
        ------
        CoreLinkAuthError
            If the PAT is missing or invalid.
        CoreLinkRequestError
            For 4xx errors (e.g. 422 Unprocessable).
        CoreLinkServerError
            For 5xx server errors.
        """
        response = self._http.post(
            "/v1/pats",
            json=request.model_dump(exclude_none=True),
        )
        self._raise_for_status(response)
        return PatIssueResponse.model_validate(response.json())

    def signup(
        self,
        request: SignupRequest,
        *,
        idempotency_key: str | None = None,
    ) -> SignupResponse:
        """Provision a new tenant from a verified Clerk identity.

        operationId: **signup** — ``POST /v1/signup``

        This is the atomic onboarding orchestration endpoint: tenant row +
        DPA acceptance + first PAT issuance in a single D1 transaction
        (INV-ONBOARD-ATOMIC-PROVISIONING).

        Parameters
        ----------
        request:
            Signup payload.  ``email_hash`` **must** be the SHA-256 hex of
            the normalised email address — raw email is never transmitted
            (CTRL-PRIV-001).
        idempotency_key:
            Optional override for the ``Idempotency-Key`` header.  If
            *None*, ``request.idempotency_key`` is used.

        Returns
        -------
        SignupResponse
            Signup outcome including the first PAT (shown once) and
            assigned tenant ID.

        Raises
        ------
        CoreLinkAuthError
            If the PAT is missing or invalid.
        CoreLinkRequestError
            For 4xx errors (e.g. 409 Conflict on duplicate signup,
            422 Unprocessable).
        CoreLinkServerError
            For 5xx server errors.
        """
        idem_key = idempotency_key or request.idempotency_key
        response = self._http.post(
            "/v1/signup",
            json=request.model_dump(exclude_none=True),
            headers={
                "Idempotency-Key": idem_key,
                "X-Correlation-Id": request.correlation_id,
            },
        )
        self._raise_for_status(response)
        return SignupResponse.model_validate(response.json())
