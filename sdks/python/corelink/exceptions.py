"""CoreLink SDK exceptions.

All SDK errors inherit from :class:`CoreLinkError` so callers can catch
a single base type.
"""

from __future__ import annotations


class CoreLinkError(Exception):
    """Base class for all CoreLink SDK errors."""


class CoreLinkAuthError(CoreLinkError):
    """Raised when the server returns HTTP 401 (missing / invalid PAT)."""

    def __init__(self, message: str = "Authentication failed — check CORELINK_PAT") -> None:
        super().__init__(message)


class CoreLinkRequestError(CoreLinkError):
    """Raised for 4xx client errors (excluding 401).

    Attributes
    ----------
    status_code:
        The HTTP status code returned by the server.
    error_code:
        The stable machine-readable code from the CoreLink error taxonomy
        (e.g. ``COR_ABUSE_DETECTED``), or ``None`` if not present in the
        response body.
    """

    def __init__(
        self,
        message: str,
        *,
        status_code: int,
        error_code: str | None = None,
    ) -> None:
        super().__init__(message)
        self.status_code = status_code
        self.error_code = error_code


class CoreLinkServerError(CoreLinkError):
    """Raised for 5xx server errors.

    Attributes
    ----------
    status_code:
        The HTTP status code returned by the server.
    """

    def __init__(self, message: str = "CoreLink server error", *, status_code: int = 500) -> None:
        super().__init__(message)
        self.status_code = status_code
