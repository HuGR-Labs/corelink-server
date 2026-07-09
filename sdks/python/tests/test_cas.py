"""Test suite for the CoreLink CAS data-plane surface (BLAKE3-keyed).

Covers the documented how-to methods against the native routes
(``crates/corelink-container/src/routes/cas.rs``):

  put / put_stream — PUT  /v1/cas/{tenant}/{digest}
  get / get_stream — GET  /v1/cas/{tenant}/{digest}
  stat             — HEAD /v1/cas/{tenant}/{digest}

Both the sync (:class:`CoreLinkClient`) and async
(:class:`AsyncCoreLinkClient`) surfaces are exercised. All HTTP is mocked with
pytest-httpx — no real network.
"""

from __future__ import annotations

import io

import pytest
from blake3 import blake3
from pytest_httpx import HTTPXMock

from corelink import (
    AsyncCoreLinkClient,
    CoreLinkClient,
    CoreLinkDigestMismatchError,
    CoreLinkNotFoundError,
    CoreLinkQuotaError,
)

_TEST_PAT = "ct_test_00000000000000000000000000000000"
_BASE_URL = "https://corelink-api.humangr.com"
_TENANT = "acme-corp"

_BLOB = b"the quick brown fox"
_DIGEST = blake3(_BLOB).hexdigest()
_CAS_URL = f"{_BASE_URL}/v1/cas/{_TENANT}/{_DIGEST}"


@pytest.fixture()
def client() -> CoreLinkClient:
    return CoreLinkClient(pat=_TEST_PAT, tenant_id=_TENANT)


# ---------------------------------------------------------------------------
# put — computes BLAKE3 and PUTs to /v1/cas/{tenant}/{digest}
# ---------------------------------------------------------------------------


def test_put_computes_blake3_and_returns_digest(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    httpx_mock.add_response(method="PUT", url=_CAS_URL, status_code=201, text=_DIGEST)
    digest = client.put(_BLOB)
    assert digest == _DIGEST
    req = httpx_mock.get_request()
    assert req is not None
    assert req.method == "PUT"
    assert req.content == _BLOB
    assert req.headers["Authorization"] == f"Bearer {_TEST_PAT}"


def test_put_idempotent_200_still_returns_digest(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    # 200 (idempotent re-write) is a success, not an error.
    httpx_mock.add_response(method="PUT", url=_CAS_URL, status_code=200, text=_DIGEST)
    assert client.put(_BLOB) == _DIGEST


def test_put_expected_digest_skips_recomputation(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    httpx_mock.add_response(method="PUT", url=_CAS_URL, status_code=201, text=_DIGEST)
    # Supplying the correct digest keys the object under the same URL.
    assert client.put(_BLOB, expected_digest=_DIGEST) == _DIGEST


def test_put_bad_expected_digest_format_raises_value_error(
    client: CoreLinkClient,
) -> None:
    with pytest.raises(ValueError, match="invalid BLAKE3 digest"):
        client.put(_BLOB, expected_digest="not-a-digest")


def test_put_server_hash_mismatch_raises_digest_mismatch(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    # A stale/forged expected_digest the server rejects with 422.
    wrong = "0" * 64
    httpx_mock.add_response(
        method="PUT",
        url=f"{_BASE_URL}/v1/cas/{_TENANT}/{wrong}",
        status_code=422,
        text="content hash mismatch",
    )
    with pytest.raises(CoreLinkDigestMismatchError):
        client.put(_BLOB, expected_digest=wrong)


def test_put_over_quota_raises_quota_error(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    httpx_mock.add_response(
        method="PUT", url=_CAS_URL, status_code=402, text="storage quota exceeded"
    )
    with pytest.raises(CoreLinkQuotaError) as exc:
        client.put(_BLOB)
    assert exc.value.status_code == 402


# ---------------------------------------------------------------------------
# put_stream — streams a file, hashing in a rewound pass
# ---------------------------------------------------------------------------


def test_put_stream_hashes_and_uploads(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    httpx_mock.add_response(method="PUT", url=_CAS_URL, status_code=201, text=_DIGEST)
    digest = client.put_stream(io.BytesIO(_BLOB), chunk_size=4)
    assert digest == _DIGEST
    req = httpx_mock.get_request()
    assert req is not None and req.content == _BLOB


def test_put_stream_non_seekable_without_expected_digest_raises(
    client: CoreLinkClient,
) -> None:
    class _NonSeekable(io.RawIOBase):
        def readable(self) -> bool:
            return True

        def seekable(self) -> bool:
            return False

    with pytest.raises(ValueError, match="seekable"):
        client.put_stream(_NonSeekable())  # type: ignore[arg-type]


# ---------------------------------------------------------------------------
# get — downloads and verifies BLAKE3 client-side
# ---------------------------------------------------------------------------


def test_get_returns_verified_bytes(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    httpx_mock.add_response(method="GET", url=_CAS_URL, status_code=200, content=_BLOB)
    assert client.get(_DIGEST) == _BLOB


def test_get_client_verify_catches_corruption(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    # Server returns bytes that do NOT hash to the requested digest.
    httpx_mock.add_response(
        method="GET", url=_CAS_URL, status_code=200, content=b"corrupted-on-wire"
    )
    with pytest.raises(CoreLinkDigestMismatchError) as exc:
        client.get(_DIGEST)
    assert exc.value.expected == _DIGEST


def test_get_verify_false_skips_check(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    httpx_mock.add_response(
        method="GET", url=_CAS_URL, status_code=200, content=b"unverified"
    )
    assert client.get(_DIGEST, verify=False) == b"unverified"


def test_get_missing_raises_not_found(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    httpx_mock.add_response(method="GET", url=_CAS_URL, status_code=404, text="not found")
    with pytest.raises(CoreLinkNotFoundError):
        client.get(_DIGEST)


def test_get_erased_410_raises_not_found(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    httpx_mock.add_response(method="GET", url=_CAS_URL, status_code=410, text="erased")
    with pytest.raises(CoreLinkNotFoundError):
        client.get(_DIGEST)


# ---------------------------------------------------------------------------
# stat — HEAD; presence + size from Content-Length
# ---------------------------------------------------------------------------


def test_stat_present_reports_size(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    httpx_mock.add_response(
        method="HEAD",
        url=_CAS_URL,
        status_code=200,
        headers={"content-length": str(len(_BLOB))},
    )
    result = client.stat(_DIGEST)
    assert result.exists is True
    assert result.size_bytes == len(_BLOB)
    assert result.digest == _DIGEST


def test_stat_absent_reports_not_exists(
    httpx_mock: HTTPXMock, client: CoreLinkClient
) -> None:
    httpx_mock.add_response(method="HEAD", url=_CAS_URL, status_code=404)
    result = client.stat(_DIGEST)
    assert result.exists is False
    assert result.size_bytes == 0


# ---------------------------------------------------------------------------
# tenant / context-manager guards
# ---------------------------------------------------------------------------


def test_cas_without_tenant_raises_value_error() -> None:
    c = CoreLinkClient(pat=_TEST_PAT)  # no tenant_id
    with pytest.raises(ValueError, match="require a tenant"):
        c.get(_DIGEST)


def test_sync_context_manager_closes(httpx_mock: HTTPXMock) -> None:
    httpx_mock.add_response(method="PUT", url=_CAS_URL, status_code=201, text=_DIGEST)
    with CoreLinkClient(pat=_TEST_PAT, tenant_id=_TENANT) as c:
        assert c.put(_BLOB) == _DIGEST


# ===========================================================================
# Async surface (AsyncCoreLinkClient) — the documented `async with` / `await`
# ===========================================================================


async def test_async_put_and_get_roundtrip(httpx_mock: HTTPXMock) -> None:
    httpx_mock.add_response(method="PUT", url=_CAS_URL, status_code=201, text=_DIGEST)
    httpx_mock.add_response(method="GET", url=_CAS_URL, status_code=200, content=_BLOB)
    async with AsyncCoreLinkClient(pat=_TEST_PAT, tenant_id=_TENANT) as client:
        digest = await client.put(_BLOB)
        assert digest == _DIGEST
        data = await client.get(digest)
        assert data == _BLOB


async def test_async_put_stream(httpx_mock: HTTPXMock) -> None:
    httpx_mock.add_response(method="PUT", url=_CAS_URL, status_code=201, text=_DIGEST)
    async with AsyncCoreLinkClient(pat=_TEST_PAT, tenant_id=_TENANT) as client:
        digest = await client.put_stream(io.BytesIO(_BLOB), chunk_size=4)
    assert digest == _DIGEST


async def test_async_get_verify_catches_corruption(httpx_mock: HTTPXMock) -> None:
    httpx_mock.add_response(method="GET", url=_CAS_URL, status_code=200, content=b"bad")
    async with AsyncCoreLinkClient(pat=_TEST_PAT, tenant_id=_TENANT) as client:
        with pytest.raises(CoreLinkDigestMismatchError):
            await client.get(_DIGEST)


async def test_async_stat_present(httpx_mock: HTTPXMock) -> None:
    httpx_mock.add_response(
        method="HEAD",
        url=_CAS_URL,
        status_code=200,
        headers={"content-length": str(len(_BLOB))},
    )
    async with AsyncCoreLinkClient(pat=_TEST_PAT, tenant_id=_TENANT) as client:
        result = await client.stat(_DIGEST)
    assert result.exists is True
    assert result.size_bytes == len(_BLOB)


async def test_async_get_stream_yields_and_verifies(httpx_mock: HTTPXMock) -> None:
    httpx_mock.add_response(
        method="GET", url=_CAS_URL, status_code=200, content=_BLOB
    )
    chunks = bytearray()
    async with AsyncCoreLinkClient(pat=_TEST_PAT, tenant_id=_TENANT) as client:
        async with client.get_stream(_DIGEST) as stream:
            async for chunk in stream:
                chunks.extend(chunk)
    assert bytes(chunks) == _BLOB


async def test_async_get_stream_corruption_raises_on_exit(httpx_mock: HTTPXMock) -> None:
    httpx_mock.add_response(
        method="GET", url=_CAS_URL, status_code=200, content=b"corrupted"
    )
    async with AsyncCoreLinkClient(pat=_TEST_PAT, tenant_id=_TENANT) as client:
        with pytest.raises(CoreLinkDigestMismatchError):
            async with client.get_stream(_DIGEST) as stream:
                async for _chunk in stream:
                    pass
