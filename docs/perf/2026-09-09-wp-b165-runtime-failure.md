# B-165 production probe — runtime failure, 2026-09-09

Status: **historical runtime failure; recovered and superseded by the complete receipt**

This file preserves the initial failure state. The read-only B-165 probe ran against production source SHA
`64e57a2ccb218f475b44a260b64af95e0bc7df2c`. The v1, npm, and pip refusal
populations returned the required HTTP 401, and `/health` returned HTTP 200.
All ten OCI `/v2/x/manifests/latest` requests instead returned HTTP 503, so the
probe correctly exited 3 and B-165 remained open at this checkpoint. No customer object was read
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

## Bounded recovery rerun

After the roll-forward reached its independent deep gate, only the failed OCI
population was rerun. Container application version 180 reported image
`2cd609d25-r1` and no provider health errors. All ten requests to
`/v2/x/manifests/latest` returned the required HTTP 401. After discarding the
first sample, median was 207.119 ms and p90 was 239.544 ms (minimum 197.107 ms,
maximum 239.544 ms). The exact sanitized recovery TSV is
`evidence/owner-actions/B-165/oci-recovery-raw-2026-09-09.tsv`, SHA-256
`d8d2f5cb4c5a8f7235ec5f0e667645b5f93acab8e63c2617c6c9e2b478eddc85`.

This proves recovery of the OCI rejection path while preserving the earlier
v1/npm/pip and health findings. At this checkpoint no authenticated served
population had run. The later bounded receipt
`docs/perf/2026-09-09-wp-b165-complete.md` supplies the two tenants, two PAT
fingerprints, two served objects, clock separation and padding decision, and
closes B-165 without rewriting this historical failure. No source-only DO
rename, deployment, mutation, or broader sweep was performed by this lane.
