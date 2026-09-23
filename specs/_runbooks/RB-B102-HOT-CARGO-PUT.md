---
id: "RB-B102-HOT-CARGO-PUT"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-09-22"
updated: "2026-09-23"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "b-102", "cargo", "sccache", "latency", "staging"]
---

# RB-B102 — hot `/cargo` PUT latency

This runbook measures the authenticated sccache write path against the fixed
staging origin. It performs ten serial 1 KiB PUTs, then bounded serial and
concurrency-four controls. Every key is unique and is deleted immediately
after its observation. Production is forbidden.

## Preconditions

Use the protected `staging` environment secret
`CORELINK_B102_STAGING_PAT` and an owner-approved staging tenant. The hosted
lane is manual:
`.github/workflows/issue-1658-b102-cargo-put.yml`. It is bound to protected
`main`, uses the canonical staging hostname, and requires the exact dispatch
confirmation. If the staging PAT or target is unavailable, the lane fails
closed and retains an indeterminate artifact.

## Attribution contract

The probe retains status, retry metadata, request and response digests,
request identity, and the final `Server-Timing` value without retaining the
PAT, body, generated key, or response body. It requires these phases on every
authenticated PUT:

- `auth`, `wdb`, `qtier`, `qbatch`, `qresid`, and `total` from the Worker;
- `origin`, `ohop`, `opat`, `oquota`, `ostore`, `oaccounting`, and `ohandler`
  from the container.

`ostore` is the durable R2 phase. `oaccounting` is the D1 byte-accounting and
URL-map phase. They remain separate in the artifact so storage latency cannot
be hidden in a combined number. BLAKE3 body hashing is currently part of the
container handler window (`ohandler`); the probe records that fact instead of
inventing a hash duration.

The probe classifies `auth;desc="d1"` as cold and `auth;desc="l1"` or
`auth;desc="kv"` as warm. Both classifications must be observed. It computes
the arithmetic median and nearest-rank p90 for total and every named phase,
and checks the origin phase budget within one millisecond of Server-Timing
rounding.

The direct HTTP client never retries. A 429 or 5xx response is retained with
its `Retry-After` value and makes the lane fail, so retry behavior cannot be
mistaken for a successful latency sample.

## Closure

The issue remains open until a fresh staging receipt compares p50 and p90 by
phase against the previous receipt and the hot server path returns to the
owner-declared order of 30–50 ms. A missing phase, auth rejection, retryable
response, or wall-time-only result is indeterminate.
