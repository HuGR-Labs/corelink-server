"""Pydantic response models for the CoreLink REST API v1.

Only the 3 MVP operations are modelled:
  - apiHealth     (operationId) — GET /api/health
  - signup        (operationId) — POST /v1/signup
  - patIssue      (operationId) — POST /v1/pats

Source: apps/docs/static/openapi-corelink-v1.yaml
"""

from __future__ import annotations

from enum import Enum
from typing import Any

from pydantic import BaseModel, Field

# ---------------------------------------------------------------------------
# Shared / reused enums
# ---------------------------------------------------------------------------


class HealthStatus(str, Enum):
    """Server health status per HealthResponse schema."""

    SERVING = "SERVING"
    NOT_SERVING = "NOT_SERVING"


class SignupOutcome(str, Enum):
    """Possible outcomes of an atomic signup operation."""

    Provisioned = "Provisioned"
    Deferred = "Deferred"
    Duplicate = "Duplicate"
    Rejected = "Rejected"


class BillingIntent(str, Enum):
    """Billing linkage state after signup."""

    Linked = "Linked"
    PendingBillingLink = "PendingBillingLink"
    Deferred = "Deferred"


class PrimaryRegion(str, Enum):
    """3-region canonical map per corelink-region / Lote 10.16."""

    Enam = "Enam"
    Sam = "Sam"
    Eu = "Eu"


# ---------------------------------------------------------------------------
# apiHealth — GET /api/health
# ---------------------------------------------------------------------------


class HealthResponse(BaseModel):
    """Response model for GET /api/health (operationId: apiHealth)."""

    status: HealthStatus
    version: str | None = None
    commit: str | None = None


# ---------------------------------------------------------------------------
# patIssue — POST /v1/pats
# ---------------------------------------------------------------------------


class PatIssueRequest(BaseModel):
    """Request body for POST /v1/pats (operationId: patIssue)."""

    label: str = Field(min_length=1, max_length=64)
    scopes: list[str] = Field(
        default=["cas.read", "cas.write", "ac.read", "ac.write"],
    )
    expires_at_ms: int | None = None


class PatMetadata(BaseModel):
    """PAT metadata — no raw token material."""

    pat_id: str
    label: str
    scopes: list[str]
    created_at_ms: int
    last_used_at_ms: int | None = None
    expires_at_ms: int | None = None
    revoked_at_ms: int | None = None
    prefix: str


class PatIssueResponse(PatMetadata):
    """Response for POST /v1/pats. shown_once_token is shown exactly once."""

    shown_once_token: str


# ---------------------------------------------------------------------------
# signup — POST /v1/signup
# ---------------------------------------------------------------------------


class SignupRequest(BaseModel):
    """Request body for POST /v1/signup (operationId: signup).

    Note: ``email_hash`` must be SHA-256 hex of the normalised email address.
    Raw email is NEVER transmitted (CTRL-PRIV-001).
    """

    clerk_event_id: str
    email_hash: str = Field(pattern=r"^[0-9a-f]{64}$")
    locale: str
    idempotency_key: str
    correlation_id: str


class PatShownOnce(PatMetadata):
    """PAT shown once during signup — raw token material included."""

    shown_once_token: str


class SignupResponse(BaseModel):
    """Response for POST /v1/signup."""

    signup_id: str
    tenant_id: str
    outcome: SignupOutcome
    billing_intent: BillingIntent | None = None
    first_pat: PatShownOnce
    primary_region: PrimaryRegion | None = None


# ---------------------------------------------------------------------------
# Error envelope
# ---------------------------------------------------------------------------


class ErrorDetail(BaseModel):
    """Inner error object within an ErrorEnvelope."""

    code: str
    message: str
    request_id: str | None = None
    details: dict[str, Any] | None = None


class ErrorEnvelope(BaseModel):
    """Standard error response shape for all 4xx/5xx responses."""

    error: ErrorDetail
