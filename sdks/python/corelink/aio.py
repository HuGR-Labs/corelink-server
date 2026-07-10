"""CoreLink asynchronous CAS client.

This is the ``async with`` / ``await`` surface the how-to guides document::

    import asyncio
    from corelink import AsyncCoreLinkClient

    async def main() -> None:
        async with AsyncCoreLinkClient(tenant_id="acme-corp") as client:
            digest = await client.put(b"hello world")
            data = await client.get(digest)

    asyncio.run(main())

It is the async twin of :class:`corelink.client.CoreLinkClient` — same
BLAKE3-keyed wire contract, same exception taxonomy — built on
``httpx.AsyncClient``. Sync and async are separate classes (mirroring
``httpx.Client`` / ``httpx.AsyncClient``) because a single method cannot be both
directly callable and awaitable.
"""

from __future__ import annotations

import logging
import os
from collections.abc import AsyncIterator
from contextlib import AbstractAsyncContextManager
from types import TracebackType
from typing import IO

import httpx
from blake3 import blake3

from ._errors import raise_for_cas_status
from .cas import (
    DEFAULT_CHUNK_SIZE,
    StatResult,
    hash_stream,
    resolve_put_digest,
    validate_digest,
    verify_bytes,
)
from .exceptions import CoreLinkDigestMismatchError

_DEFAULT_BASE_URL = "https://corelink-api.humangr.com"
_DEFAULT_TIMEOUT = 30.0

_logger = logging.getLogger("corelink.client")


async def _aiter_file(fileobj: IO[bytes], chunk_size: int) -> AsyncIterator[bytes]:
    """Adapt a (blocking) binary file to the async byte-iterator ``httpx``'s
    async transport expects for a streaming upload body."""
    while True:
        chunk = fileobj.read(chunk_size)
        if not chunk:
            break
        yield chunk


class AsyncGetStream:
    """Async context manager yielding a blob's bytes in chunks and verifying the
    BLAKE3 root once the last chunk arrives (CTRL-CAS-002).

    Returned by :meth:`AsyncCoreLinkClient.get_stream`; iterate it inside an
    ``async with`` block. ``__aexit__`` raises :class:`CoreLinkDigestMismatchError`
    if the accumulated bytes do not hash to the requested digest (unless
    ``verify=False``).
    """

    def __init__(self, client: httpx.AsyncClient, url: str, digest: str, *, verify: bool) -> None:
        self._client = client
        self._url = url
        self._digest = digest
        self._verify = verify
        self._hasher = blake3()
        self._cm: AbstractAsyncContextManager[httpx.Response] | None = None
        self._response: httpx.Response | None = None
        self._complete = False

    async def __aenter__(self) -> AsyncGetStream:
        self._cm = self._client.stream("GET", self._url)
        self._response = await self._cm.__aenter__()
        raise_for_cas_status(self._response)
        return self

    async def __aiter__(self) -> AsyncIterator[bytes]:
        assert self._response is not None
        async for chunk in self._response.aiter_bytes():
            if self._verify:
                self._hasher.update(chunk)
            yield chunk
        self._complete = True

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        if self._cm is not None:
            await self._cm.__aexit__(exc_type, exc, tb)
        # Only verify on a clean, fully-consumed stream — never mask an in-flight
        # error, and never claim a mismatch when the caller broke out early.
        if exc is None and self._verify and self._complete:
            actual = self._hasher.hexdigest()
            if actual != self._digest:
                raise CoreLinkDigestMismatchError(
                    "streaming BLAKE3 verify failed: bytes do not match requested digest",
                    expected=self._digest,
                    actual=actual,
                )


class AsyncCoreLinkClient:
    """Asynchronous, tenant-scoped CoreLink CAS client (BLAKE3-keyed).

    Parameters
    ----------
    pat:
        Personal Access Token; falls back to ``CORELINK_PAT``.
    tenant_id:
        Tenant scope for all CAS operations (required for any CAS call).
    base_url:
        Server base URL. Defaults to ``https://corelink-api.humangr.com``.
    timeout:
        Request timeout in seconds (default 30).
    """

    def __init__(
        self,
        pat: str | None = None,
        *,
        tenant_id: str | None = None,
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
        self._tenant_id: str | None = tenant_id
        self._base_url: str = base_url.rstrip("/")
        self._http: httpx.AsyncClient = httpx.AsyncClient(
            base_url=self._base_url,
            headers={"Authorization": f"Bearer {self._pat}"},
            timeout=timeout,
        )

    async def __aenter__(self) -> AsyncCoreLinkClient:
        return self

    async def __aexit__(self, *_args: object) -> None:
        await self.aclose()

    async def aclose(self) -> None:
        """Close the underlying async HTTP connection pool."""
        await self._http.aclose()

    def _cas_url(self, digest: str) -> str:
        if not self._tenant_id:
            raise ValueError(
                "CAS operations require a tenant; construct the client with "
                "`tenant_id=`."
            )
        return f"/v1/cas/{self._tenant_id}/{validate_digest(digest)}"

    async def put(self, data: bytes, *, expected_digest: str | None = None) -> str:
        """Store *data* and return its 64-char BLAKE3 hex digest.

        See :meth:`corelink.client.CoreLinkClient.put`.
        """
        digest = resolve_put_digest(data, expected_digest)
        response = await self._http.put(self._cas_url(digest), content=data)
        raise_for_cas_status(response)
        return digest

    async def put_stream(
        self,
        fileobj: IO[bytes],
        *,
        chunk_size: int = DEFAULT_CHUNK_SIZE,
        expected_digest: str | None = None,
    ) -> str:
        """Stream a large blob from a seekable binary file without buffering it
        whole. See :meth:`corelink.client.CoreLinkClient.put_stream`.
        """
        if expected_digest is not None:
            digest = validate_digest(expected_digest)
        else:
            digest = hash_stream(fileobj, chunk_size)
        response = await self._http.put(
            self._cas_url(digest), content=_aiter_file(fileobj, chunk_size)
        )
        raise_for_cas_status(response)
        return digest

    async def get(self, digest: str, *, verify: bool = True) -> bytes:
        """Download a blob by BLAKE3 digest, verifying it client-side by default.

        See :meth:`corelink.client.CoreLinkClient.get`.
        """
        url = self._cas_url(digest)
        if not verify:
            _logger.warning("corelink.client.verify_disabled")
        response = await self._http.get(url)
        raise_for_cas_status(response)
        data = response.content
        if verify:
            verify_bytes(digest, data)
        return data

    def get_stream(self, digest: str, *, verify: bool = True) -> AsyncGetStream:
        """Return an :class:`AsyncGetStream` for a chunked, incrementally-verified
        download. Use inside ``async with`` and iterate it::

            async with client.get_stream(digest) as stream:
                async for chunk in stream:
                    ...
        """
        url = self._cas_url(digest)
        if not verify:
            _logger.warning("corelink.client.verify_disabled")
        return AsyncGetStream(self._http, url, validate_digest(digest), verify=verify)

    async def stat(self, digest: str) -> StatResult:
        """Return metadata for a blob without downloading it (HEAD).

        See :meth:`corelink.client.CoreLinkClient.stat`.
        """
        url = self._cas_url(digest)
        response = await self._http.head(url)
        if response.status_code in (404, 410):
            return StatResult(digest=digest, size_bytes=0, exists=False)
        raise_for_cas_status(response)
        size = int(response.headers.get("content-length", "0") or "0")
        return StatResult(digest=digest, size_bytes=size, exists=True)
