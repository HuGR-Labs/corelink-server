"""CoreLink Python SDK — v0.1.0a1.

Public surface:

    from corelink import CoreLinkClient, AsyncCoreLinkClient, StatResult
    from corelink.types import (
        HealthResponse, HealthStatus,
        PatIssueRequest, PatIssueResponse, PatMetadata,
        SignupRequest, SignupResponse,
    )
    from corelink.exceptions import (
        CoreLinkError, CoreLinkAuthError,
        CoreLinkRequestError, CoreLinkServerError,
        CoreLinkNotFoundError, CoreLinkDigestMismatchError, CoreLinkQuotaError,
    )

Control-plane operations (OpenAPI operationIds):
  - apiHealth  — GET /api/health
  - patIssue   — POST /v1/pats
  - signup     — POST /v1/signup

CAS data-plane operations (native, BLAKE3-keyed, tenant-scoped):
  - put / put_stream — PUT  /v1/cas/{tenant}/{digest}
  - get / get_stream — GET  /v1/cas/{tenant}/{digest}
  - stat             — HEAD /v1/cas/{tenant}/{digest}

The sync surface is :class:`CoreLinkClient`; the ``async with`` / ``await``
surface is :class:`AsyncCoreLinkClient`.
"""

from .aio import AsyncCoreLinkClient
from .cas import StatResult
from .client import CoreLinkClient
from .exceptions import (
    CoreLinkAuthError,
    CoreLinkDigestMismatchError,
    CoreLinkError,
    CoreLinkNotFoundError,
    CoreLinkQuotaError,
    CoreLinkRequestError,
    CoreLinkServerError,
)
from .types import (
    HealthResponse,
    HealthStatus,
    PatIssueRequest,
    PatIssueResponse,
    PatMetadata,
    SignupRequest,
    SignupResponse,
)

__all__ = [
    "CoreLinkClient",
    "AsyncCoreLinkClient",
    "StatResult",
    # exceptions
    "CoreLinkError",
    "CoreLinkAuthError",
    "CoreLinkRequestError",
    "CoreLinkServerError",
    "CoreLinkNotFoundError",
    "CoreLinkDigestMismatchError",
    "CoreLinkQuotaError",
    # types
    "HealthResponse",
    "HealthStatus",
    "PatIssueRequest",
    "PatIssueResponse",
    "PatMetadata",
    "SignupRequest",
    "SignupResponse",
]

__version__ = "0.1.0a1"
