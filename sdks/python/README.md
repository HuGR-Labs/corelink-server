# corelink-py — Python SDK for CoreLink

Python client SDK for the [CoreLink](https://corelink.humangr.com) Customer & Privacy REST API.

**Status:** Alpha (`v0.1.0a1`). MVP scope — 3 operations.  
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

---

## MVP Operations

| Method | operationId | HTTP | Path |
|--------|------------|------|------|
| `get_health()` | `apiHealth` | GET | `/api/health` |
| `issue_pat(req)` | `patIssue` | POST | `/v1/pats` |
| `signup(req)` | `signup` | POST | `/v1/signup` |

Source: `apps/docs/static/openapi-corelink-v1.yaml`

---

## Error Handling

```python
from corelink.exceptions import (
    CoreLinkError,       # base
    CoreLinkAuthError,   # 401
    CoreLinkRequestError,  # 4xx (has .status_code, .error_code)
    CoreLinkServerError,   # 5xx (has .status_code)
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
