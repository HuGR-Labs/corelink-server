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


class CoreLinkNotFoundError(CoreLinkError):
    """Raised when a CAS blob does not exist (HTTP 404) or was erased (410).

    Surfaced by :meth:`CoreLinkClient.get` / :meth:`AsyncCoreLinkClient.get`
    when the requested digest is absent from the tenant's CAS keyspace.
    """

    def __init__(self, message: str = "blob not found in tenant CAS") -> None:
        super().__init__(message)


class CoreLinkDigestMismatchError(CoreLinkError):
    """Raised on a BLAKE3 integrity failure.

    Two provenances:

    * **Server-side** (HTTP 422) — a ``put`` / ``put_stream`` whose claimed
      digest did not equal ``blake3(body)`` as recomputed by the server (a
      forged or stale ``expected_digest``).
    * **Client-side** — a ``get`` with default-on verification whose returned
      bytes did not hash to the requested digest (corruption on the wire).

    Attributes
    ----------
    expected:
        The digest that was requested / claimed, when known.
    actual:
        The digest actually computed over the bytes, when known (client-side
        verification only).
    """

    def __init__(
        self,
        message: str = "BLAKE3 digest mismatch",
        *,
        expected: str | None = None,
        actual: str | None = None,
    ) -> None:
        super().__init__(message)
        self.expected = expected
        self.actual = actual


class CoreLinkQuotaError(CoreLinkError):
    """Raised when the tenant is over a storage / spend ceiling (HTTP 402).

    Attributes
    ----------
    status_code:
        The HTTP status code returned by the server (402).
    """

    def __init__(
        self,
        message: str = "tenant over quota — inspect `corelink stat`",
        *,
        status_code: int = 402,
    ) -> None:
        super().__init__(message)
        self.status_code = status_code
