# corelink-py — Python SDK for CoreLink

Python client SDK for the [CoreLink](https://corelink.humangr.com) Customer & Privacy REST API.

**Status:** Alpha (`v0.1.0a1`). Control-plane (health / PAT / signup) +
BLAKE3-keyed CAS data plane (put / get / stat), sync and async.  
**License:** Apache-2.0

---

## Install

```bash
pip install -e .
```

For development (mypy, ruff, pytest, pytest-httpx):

```bash
pip install -e ".[dev]"
```

Requires Python **3.10+**.

---

## Quickstart

```python
import os
from corelink import CoreLinkClient, PatIssueRequest

# PAT can also be set via CORELINK_PAT env var
client = CoreLinkClient(pat=os.environ["CORELINK_PAT"])

# 1. Health probe — no auth required
health = client.get_health()
print(health.status.value)  # "SERVING"

# 2. Issue a PAT
pat = client.issue_pat(PatIssueRequest(label="my-notebook"))
print(pat.shown_once_token)  # store securely — shown exactly once

# 3. Context-manager form (closes connection pool on exit)
with CoreLinkClient(pat=os.environ["CORELINK_PAT"]) as cl:
    health = cl.get_health()
```

### CAS (content-addressable storage)

CAS operations are BLAKE3-keyed and tenant-scoped — pass `tenant_id=`:

```python
from corelink import CoreLinkClient

cas = CoreLinkClient(pat=os.environ["CORELINK_PAT"], tenant_id="acme-corp")
digest = cas.put(b"hello world")        # 64-char BLAKE3 hex
data = cas.get(digest)                  # client-verified by default
info = cas.stat(digest)                 # info.exists, info.size_bytes
```

The `async with` / `await` twin is `AsyncCoreLinkClient` (adds `get_stream` for
chunked, incrementally-verified downloads):

```python
from corelink import AsyncCoreLinkClient

async with AsyncCoreLinkClient(tenant_id="acme-corp") as client:
    digest = await client.put(data)
    blob = await client.get(digest)
```

---

## Operations

| Method | HTTP | Path |
|--------|------|------|
| `get_health()` | GET | `/api/health` |
| `issue_pat(req)` | POST | `/v1/pats` |
| `signup(req)` | POST | `/v1/signup` |
| `put(data)` / `put_stream(f)` | PUT | `/v1/cas/{tenant}/{digest}` |
| `get(digest)` / `get_stream(digest)` (async) | GET | `/v1/cas/{tenant}/{digest}` |
| `stat(digest)` | HEAD | `/v1/cas/{tenant}/{digest}` |

Control-plane source: `apps/docs/static/openapi-corelink-v1.yaml`; CAS routes:
`crates/corelink-container/src/routes/cas.rs`.

---

## Error Handling

```python
from corelink.exceptions import (
    CoreLinkError,               # base
    CoreLinkAuthError,           # 401
    CoreLinkRequestError,        # 4xx (has .status_code, .error_code)
    CoreLinkServerError,         # 5xx (has .status_code)
    CoreLinkNotFoundError,       # CAS 404 / 410 (get)
    CoreLinkDigestMismatchError, # BLAKE3 mismatch (server 422 or client verify)
    CoreLinkQuotaError,          # CAS 402 (over quota; has .status_code)
)

try:
    pat = client.issue_pat(req)
except CoreLinkAuthError:
    print("Check your CORELINK_PAT")
except CoreLinkRequestError as e:
    print(f"Request error {e.status_code}: {e.error_code}")
```

---

## Development

```bash
# Run all checks
make ci

# Individual targets
make lint       # ruff
make typecheck  # mypy --strict
make test       # pytest -v
```

---

## License

Apache-2.0 — see [LICENSE](./LICENSE).
