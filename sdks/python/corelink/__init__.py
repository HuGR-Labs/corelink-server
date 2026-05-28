"""CoreLink Python SDK — MVP (v0.1.0a1).

Public surface:

    from corelink import CoreLinkClient
    from corelink.types import (
        HealthResponse, HealthStatus,
        PatIssueRequest, PatIssueResponse, PatMetadata,
        SignupRequest, SignupResponse,
    )
    from corelink.exceptions import (
        CoreLinkError, CoreLinkAuthError,
        CoreLinkRequestError, CoreLinkServerError,
    )

MVP operations (OpenAPI operationIds):
  - apiHealth  — GET /api/health
  - patIssue   — POST /v1/pats
  - signup     — POST /v1/signup
"""

from .client import CoreLinkClient
from .exceptions import (
    CoreLinkAuthError,
    CoreLinkError,
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
    # exceptions
    "CoreLinkError",
    "CoreLinkAuthError",
    "CoreLinkRequestError",
    "CoreLinkServerError",
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
