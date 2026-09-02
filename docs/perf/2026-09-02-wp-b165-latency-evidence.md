# B-165 latency evidence — 2026-09-02

Status: **partial evidence; B-165 remains open**

This is a dated, read-only measurement from the CoreLink production API. It is
not a deployment claim and does not close B-165. The refusal population is
complete for this run; the served population remains blocked because B-160 is
still open and no customer PATs were available. The operator mint key is not a
substitute for customer-path evidence and was not used.

## Contract and provenance

- Contract: B-165 in `BACKLOG.md`, certified catalog commit
  `2c74b8fc9a772c43cdeb3aff3fd277ec31eb1077`.
- Measurement ref: `origin/main` at
  `628d9511d26622ed6cb4fecceef30cf78c5167c8`, fetched 2026-09-02.
- Observed: 2026-09-02 05:00:34–05:00:40 UTC (02:00:34–02:00:40 BRT),
  using the committed harness.
- Client: this Mac, `curl`, default negotiated HTTP/2; destination edge
  confirmed as Cloudflare `GRU` (`cf-ray: a349f01fbd765e5b-GRU` on the
  control response).
- Method: ten unauthenticated GETs per required refusal path; sample 1 was
  discarded as the cold sample. The nine retained observations are the
  population for median and nearest-rank p90 below.
- Control: ten unauthenticated GETs to `/health`, also with sample 1
  discarded. All ten returned HTTP 200; the nine retained samples are shown.

## Refusal results

Every required refusal request returned HTTP 401. No request carried an
`Authorization` header, so these numbers are unauthenticated refusal latency,
not served-path latency.

| population | path | attempts | retained | statuses | median | p90 | min–max |
|---|---|---:|---:|---|---:|---:|---:|
| refusal | `/v1/cas/x/y` | 10 | 9 | 10×401 | 59.2 ms | 62.2 ms | 51.1–62.2 ms |
| refusal | `/npm/x` | 10 | 9 | 10×401 | 56.4 ms | 64.3 ms | 53.2–64.3 ms |
| refusal | `/pip/simple/x` | 10 | 9 | 10×401 | 57.6 ms | 65.7 ms | 53.7–65.7 ms |
| refusal | `/v2/x/manifests/latest` | 10 | 9 | 10×401 | 212.5 ms | 221.2 ms | 206.5–221.2 ms |
| control | `/health` | 10 | 9 | 10×200 | 59.7 ms | 71.5 ms | 56.1–71.5 ms |

Raw observations (seconds; sample 1 remains visible so the discard is
auditable):

```text
surface path sample status total_s
v1 /v1/cas/x/y 1 401 0.067565
v1 /v1/cas/x/y 2 401 0.056244
v1 /v1/cas/x/y 3 401 0.062005
v1 /v1/cas/x/y 4 401 0.062154
v1 /v1/cas/x/y 5 401 0.051096
v1 /v1/cas/x/y 6 401 0.055875
v1 /v1/cas/x/y 7 401 0.056044
v1 /v1/cas/x/y 8 401 0.059599
v1 /v1/cas/x/y 9 401 0.059354
v1 /v1/cas/x/y 10 401 0.059170
npm /npm/x 1 401 0.060390
npm /npm/x 2 401 0.053194
npm /npm/x 3 401 0.055807
npm /npm/x 4 401 0.058550
npm /npm/x 5 401 0.056411
npm /npm/x 6 401 0.064275
npm /npm/x 7 401 0.054955
npm /npm/x 8 401 0.061671
npm /npm/x 9 401 0.055811
npm /npm/x 10 401 0.057538
pip /pip/simple/x 1 401 0.060682
pip /pip/simple/x 2 401 0.059868
pip /pip/simple/x 3 401 0.055074
pip /pip/simple/x 4 401 0.065717
pip /pip/simple/x 5 401 0.053664
pip /pip/simple/x 6 401 0.056152
pip /pip/simple/x 7 401 0.064203
pip /pip/simple/x 8 401 0.057573
pip /pip/simple/x 9 401 0.061299
pip /pip/simple/x 10 401 0.057162
v2 /v2/x/manifests/latest 1 401 0.288126
v2 /v2/x/manifests/latest 2 401 0.217267
v2 /v2/x/manifests/latest 3 401 0.212533
v2 /v2/x/manifests/latest 4 401 0.209787
v2 /v2/x/manifests/latest 5 401 0.213138
v2 /v2/x/manifests/latest 6 401 0.206457
v2 /v2/x/manifests/latest 7 401 0.217519
v2 /v2/x/manifests/latest 8 401 0.212104
v2 /v2/x/manifests/latest 9 401 0.221221
v2 /v2/x/manifests/latest 10 401 0.210229
health /health 1 200 0.053957
health /health 2 200 0.059007
health /health 3 200 0.056149
health /health 4 200 0.065419
health /health 5 200 0.058014
health /health 6 200 0.059817
health /health 7 200 0.063436
health /health 8 200 0.071507
health /health 9 200 0.059190
health /health 10 200 0.059726
```

## Served-path blocker and decision state

B-160 is still `open` at the measured ref: no customer self-service
`POST /v1/pats` exists. The only credential visible locally for minting is an
operator credential, and using it would measure an operator route rather than
the customer served path. No external mutation (minting, creating tenants,
writing objects, or deployment) was performed.

Consequently this report intentionally has no served-path median/p90, no
two-tenant claim, and no padding decision. The 15–30 ms target remains an
undecided contract question; this run supplies refusal evidence only. The next
measurement must provide two distinct customer tenants, two customer PATs, and
one known-existing served object/path for each tenant, then rerun the same
ten-sample/discard-first protocol.

## Reproduction

The approved read-only harness is `scripts/probe-wp-b165-latency.sh`.

```bash
B165_MODE=refusal B165_SAMPLES=10 \
  scripts/probe-wp-b165-latency.sh
```

The script exits 2 in refusal mode after recording the refusal/control result,
because a partial run must not be interpreted as a complete B-165 proof. Full
mode additionally requires `B165_TENANT_A/B`, `B165_PAT_A/B`, and
`B165_SERVED_PATH_A/B`; it rejects missing, non-distinct, or non-tenant-bound
inputs and only accepts HTTP 200 served responses. It never performs a write.
