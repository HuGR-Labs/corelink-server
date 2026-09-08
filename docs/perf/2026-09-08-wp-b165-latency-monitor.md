# B-165 latency monitor evidence — 2026-09-08

Status: **partial/open; served path remains unmeasured**

The read-only refusal probe ran against `https://corelink-api.humangr.com` with
10 samples per population, discarding sample 1 as cold. The local client was
the Mac workstation; the raw TSV was written under `/tmp` and contains no
credential values. The verifier reported `partial/open` and
`closure_allowed=false` because no customer PATs or known served paths were
available.

| population | status | retained | median | p90 |
|---|---:|---:|---:|---:|
| `/v1/cas/x/y` | 401 | 9 | 181.5 ms | 1,237.2 ms |
| `/npm/x` | 401 | 9 | 190.2 ms | 352.4 ms |
| `/pip/simple/x` | 401 | 9 | 152.2 ms | 1,114.4 ms |
| `/v2/x/manifests/latest` | 401 | 9 | 287.2 ms | 607.2 ms |
| `/health` control | 200 | 9 | 167.5 ms | 304.5 ms |

The exact bounded commands were:

```sh
B165_MODE=refusal B165_SAMPLES=10 \
  B165_RAW_OUT=/tmp/b165-live-20260908.tsv \
  scripts/probe-wp-b165-latency.sh
python3 scripts/verify_b165_latency.py \
  /tmp/b165-live-20260908.tsv --samples 10
```

The probe returned exit 2 (expected refusal-only/blocked result), and the
verifier returned exit 2 with `status=partial/open`. The six-hour hosted-runner
monitor stores this refusal/control evidence for 30 days. Full acceptance still
requires two distinct customer tenants, two distinct customer PATs, one known
served path per tenant, and 10-sample served populations; the verifier rejects
closure when either served population is absent.
