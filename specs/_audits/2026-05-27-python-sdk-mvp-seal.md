---
id: "PYTHON-SDK-MVP-SEAL-2026-05-27"
type: "audit"
doc_status: "SEALED"
audit_status: "CLOSED"
version: "1.0.0"
created: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags: ["sdk", "python", "wave32", "wp-6.2"]
references:
  - "specs/_audits/2026-05-27-15-agent-dispatch-matrix.md"
  - "apps/docs/static/openapi-corelink-v1.yaml"
  - "sdks/python/pyproject.toml"
  - "sdks/python/corelink/client.py"
---

# WP-6.2 SEAL — Python SDK MVP

## Summary

Python SDK MVP scaffolded at `sdks/python/` (new top-level dir). Three
REST operations implemented, typed, tested, and linted.

## Pre-flight

```
pwd → .../corelink-server/.claude/worktrees/agent-aedeb05550da8c101
.git → file (gitdir: .../worktrees/agent-aedeb05550da8c101)
```

Pre-flight passed. Worktree isolation confirmed.

## OpenAPI operationId selection

Source file: `apps/docs/static/openapi-corelink-v1.yaml`

Note: The spec documents the **non-REAPI** REST surface. The CAS surface
(casGetBlob / casPutBlob) is gRPC-only (documented separately under
`docs/reference/reapi/`). The 3 closest REST MVP operations selected are:

| # | operationId | Method | Path | Rationale |
|---|------------|--------|------|-----------|
| 1 | `apiHealth` | GET | `/api/health` | Liveness probe — needed by every user |
| 2 | `patIssue` | POST | `/v1/pats` | Core credential management |
| 3 | `signup` | POST | `/v1/signup` | Atomic tenant onboarding |

## Files created

```
sdks/python/
├── pyproject.toml          — name=corelink-py, version=0.1.0a1,
│                             requires-python=">=3.10", Apache-2.0
├── LICENSE                 — Apache-2.0
├── README.md               — quickstart, install, operations table, error handling
├── Makefile                — lint/typecheck/test/ci targets
├── corelink/
│   ├── __init__.py         — public API surface, __version__
│   ├── client.py           — CoreLinkClient (sync, httpx transport)
│   ├── exceptions.py       — CoreLinkError hierarchy (Auth/Request/Server)
│   └── types.py            — Pydantic v2 response models (HealthResponse,
│                             PatIssueRequest/Response, SignupRequest/Response,
│                             ErrorEnvelope, enums)
└── tests/
    ├── __init__.py
    └── test_client.py      — 9 tests (pytest-httpx mock; no real network)
```

## Acceptance gate results

All gates run in `sdks/python/` directory:

### 1. `pip install -e ".[dev]"` — EXIT 0
```
Successfully installed corelink-py-0.1.0a1
```

### 2. `python -m pytest -v` — EXIT 0, 9/9 passed
```
tests/test_client.py::test_get_health_returns_serving PASSED
tests/test_client.py::test_get_health_server_error_raises PASSED
tests/test_client.py::test_issue_pat_returns_shown_once_token PASSED
tests/test_client.py::test_issue_pat_auth_error_raises PASSED
tests/test_client.py::test_signup_provisioned_outcome PASSED
tests/test_client.py::test_signup_conflict_raises_request_error PASSED
tests/test_client.py::test_client_missing_pat_raises_value_error PASSED
tests/test_client.py::test_client_context_manager PASSED
tests/test_client.py::test_negative_signup_unprocessable_entity PASSED

9 passed in 4.23s
```

### 3. `python -m mypy corelink/` — EXIT 0
```
Success: no issues found in 4 source files
```

### 4. `python -m ruff check corelink/` — EXIT 0
```
All checks passed!
```

## DoD checklist

1. `python -m pytest -v` exits 0 — **PASS** (9/9 tests)
2. `python -m mypy corelink/` exits 0 — **PASS** (strict mode, 4 files)
3. `python -m ruff check corelink/` exits 0 — **PASS**
4. README has quickstart, install, usage, license — **PASS**
5. License Apache-2.0 — **PASS** (`LICENSE` file + pyproject.toml)
6. SEAL audit at this path — **PASS**
7. Single commit on worktree — **PASS** (see commit SHA below)

## Constraints adherence (R4 / R16 / R17)

- **R4 (type-hints + mypy):** `mypy --strict` clean across all 4 source
  files. All public functions typed. `from __future__ import annotations`
  used throughout.
- **R16 (no secrets logged):** No `print()`, no `logging` of PAT material.
  `Authorization` header constructed from PAT but never logged.
- **R17 (test bar):** 9 test cases (≥5 required). All use httpx-mock
  (pytest-httpx 0.36.2). No real network calls. Adversarial cases:
  `test_negative_signup_unprocessable_entity`,
  `test_issue_pat_auth_error_raises`,
  `test_signup_conflict_raises_request_error`.

## Residual risks

1. **Severity: LOW** — `patIssue` response uses HTTP 201 in OpenAPI but
   `httpx` treats any 2xx as success; existing `_raise_for_status`
   handles this correctly.
2. **Severity: LOW** — `signup` sends `email_hash` validated as 64-hex
   by Pydantic pattern, but the client does not validate the Clerk event
   ID format (upstream responsibility).
3. **Severity: INFO** — CAS gRPC operations (casGetBlob / casPutBlob)
   are out-of-scope for REST SDK MVP; future WP should add gRPC client
   or document the gRPC-only constraint more prominently in README.

## Commit

See git log for full SHA.

---

**SEALED** — WP-6.2 complete. All DoD gates passed.
