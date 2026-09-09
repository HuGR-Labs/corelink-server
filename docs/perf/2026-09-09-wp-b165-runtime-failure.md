# B-165 production probe — runtime failure, 2026-09-09

Status: **failed/open; this is not green latency evidence**

The read-only B-165 probe ran against production source SHA
`64e57a2ccb218f475b44a260b64af95e0bc7df2c`. The v1, npm, and pip refusal
populations returned the required HTTP 401, and `/health` returned HTTP 200.
All ten OCI `/v2/x/manifests/latest` requests instead returned HTTP 503, so the
probe correctly exited 3 and B-165 remains open. No customer object was read
and no credential was retained.

The response body classified the defect as `CONTAINER_UNAVAILABLE` /
`container_health_check_failed`. Repeated requests after the container rollout
reported the same failure. A short read-only Worker tail showed Worker version
`dd270ff0-8ab5-4dd7-8de3-54df432e927f` completing normally with the 503 response,
without a Worker exception. Provider readback then showed container application
version 179 on image `b90b245df-r1`, state `ready`, but the `_oci` instance was
`inactive` with no location or version. This localizes the failure to starting
the shared OCI container after the rollout. It does not justify guessing among
image boot, scheduling, or a boot-time dependency.

The normalized receipt is
`evidence/owner-actions/B-165/rejection-served-latency.json`; the exact sanitized
TSV is `evidence/owner-actions/B-165/rejection-raw-2026-09-09.tsv`. Its SHA-256 is
`820754b2b16d1f5532c9d71829d463819b84848f407f00250b6f1bafef43637e`.

Next safe action: restore OCI availability before retrying latency. Either
diagnose image `b90b245df-r1` boot logs/provider scheduling, or roll the IAD
container application back to the last known OCI-serving image under the normal
production rollout procedure. A source-only rename of the `_oci` Durable Object
was deliberately not made: the current evidence shows a general container-start
failure and does not prove a stale DO-placement defect. After availability is
restored, rerun the complete B-165 probe with two distinct approved tenants,
two distinct PATs, and one known served object per tenant.
