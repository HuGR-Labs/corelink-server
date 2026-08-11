# CoreLink Python SDK (`corelink-py`)

> Plain `httpx`-based REST client (sync `CoreLinkClient` +
> async `AsyncCoreLinkClient`), source at `sdks/python/`.
> Client-verify defaults on per CTRL-CAS-002, done client-side in Python
> (BLAKE3 re-hash of the downloaded bytes) — there is no PyO3/Rust FFI
> layer in the shipped package (that architecture was superseded; the
> code that ships is a straight HTTP client over `/v1/cas/{tenant}/{digest}`
> and the control-plane routes in `openapi-corelink-v1.yaml`).

## Installation

```sh
pip install corelink-py
```

## Quick Start

```python
import os
from corelink import CoreLinkClient

client = CoreLinkClient(
    pat=os.environ["CORELINK_PAT"],
    tenant_id="acme-corp",
)
# verify defaults to True per-call (CTRL-CAS-002)
digest = client.put(b"hello world")
data = client.get(digest)
stat = client.stat(digest)
print(f"digest={digest[:16]}... size={stat.size_bytes}")
client.close()
```

Async usage (separate class, mirrors `httpx.Client` / `httpx.AsyncClient`):

```python
import asyncio
from corelink import AsyncCoreLinkClient

async def main() -> None:
    async with AsyncCoreLinkClient(tenant_id="acme-corp") as client:
        digest = await client.put(b"hello world")
        data = await client.get(digest)

asyncio.run(main())
```

## API Reference

### `CoreLinkClient(pat=None, *, tenant_id=None, base_url=..., timeout=30.0)`

| Parameter | Type | Default | Description |
|---|---|---|---|
| `pat` | `str \| None` | `None` | Personal Access Token; falls back to the `CORELINK_PAT` env var. Raises `ValueError` if neither is set. |
| `tenant_id` | `str \| None` | `None` | Tenant scope for CAS operations (`put`/`get`/`stat`). Optional for control-plane calls; a CAS call without it raises `ValueError`. |
| `base_url` | `str` | `https://corelink-api.humangr.com` | Override for staging/local dev. |
| `timeout` | `float` | `30.0` | Request timeout in seconds. |

There is no constructor-level `client_verify` flag — verify is a **per-call**
kwarg on `get()` (see below), not a client-wide setting.

#### `client.put(data: bytes, *, expected_digest: str | None = None) -> str`

`PUT /v1/cas/{tenant}/{digest}`. Computes the BLAKE3 hex digest locally
(unless `expected_digest` is supplied) and uploads.

```python
digest = client.put(b"artifact bytes")
print(f"blake3:{digest}")
```

There is also `client.put_stream(fileobj, *, chunk_size=..., expected_digest=None)`
for streaming large blobs without buffering them whole.

#### `client.get(digest: str, *, verify: bool = True) -> bytes`

`GET /v1/cas/{tenant}/{digest}`. When `verify` is true (the default), the
downloaded bytes are re-hashed client-side and a mismatch raises
`CoreLinkDigestMismatchError`.

```python
data = client.get("6b86b273ff34fc...2e6c...abcd")
data_unverified = client.get("6b86b273ff34fc...2e6c...abcd", verify=False)
```

#### `client.stat(digest: str) -> StatResult`

`HEAD /v1/cas/{tenant}/{digest}`. Returns `StatResult(digest, size_bytes, exists)`
without downloading the blob; 404/410 map to `exists=False` rather than
raising.

```python
stat = client.stat(digest)
if not stat.exists:
    print("blob not found")
```

#### Control-plane methods

`client.get_health()` (`GET /api/health`), `client.issue_pat(request)`
(`POST /v1/pats`), and `client.signup(request, *, idempotency_key=None)`
(`POST /v1/signup`) round out the sync client — see `sdks/python/corelink/client.py`
for the typed request/response models.

## Client-Verify Opt-Out

Opt-out is per-call, not per-client — pass `verify=False` to `get()`:

```python
data = client.get(digest, verify=False)  # skips the BLAKE3 re-hash
```

## Error Codes

| Exception | Meaning |
|---|---|
| `CoreLinkDigestMismatchError` | BLAKE3 hash of downloaded bytes does not match the requested digest (also raised server-side as HTTP 422 on `put`). |
| `CoreLinkNotFoundError` | 404/410 from the CAS route. |
| `CoreLinkAuthError` | Missing/invalid PAT. |
| `CoreLinkQuotaError`, `CoreLinkRequestError`, `CoreLinkServerError` | Other 4xx/5xx mapped errors — see `sdks/python/corelink/_errors.py` and `exceptions.py`. |

## Build from Source

```sh
cd sdks/python
pip install -e ".[dev]"
pytest tests/
```

There is no Rust extension module to build — this is a pure-Python package
(`httpx` + `blake3` as its only runtime dependencies).

## Design

Client-verify (the BLAKE3 re-hash of downloaded bytes) is implemented
directly in Python (`sdks/python/corelink/cas.py`), not via an FFI call
into a shared Rust implementation. This differs from the Go SDK, which
does share a Rust cgo bridge (`tools/sdks/go`) for its verify path — the
two SDKs are not architecturally symmetric today.
