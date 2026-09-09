# B-165 latency drain evidence — 2026-09-09

Status: **partial/open; served path remains unmeasured**

The read-only refusal probe ran against `https://corelink-api.humangr.com` with
10 samples per population, discarding sample 1 as cold. The local client was
the Mac workstation; raw TSV and verifier output were written under `/tmp` and
contain no credential values. The probe and verifier both returned exit 2,
which is the fail-closed refusal-only result. No production deployment or
configuration mutation was performed.

| population | status | retained | median | p90 |
|---|---:|---:|---:|---:|
| `/v1/cas/x/y` | 401 | 9 | 107.7 ms | 356.6 ms |
| `/npm/x` | 401 | 9 | 89.7 ms | 177.7 ms |
| `/pip/simple/x` | 401 | 9 | 88.3 ms | 203.5 ms |
| `/v2/x/manifests/latest` | 401 | 9 | 294.0 ms | 568.3 ms |
| `/health` control | 200 | 9 | 73.8 ms | 132.9 ms |

Exact bounded commands:

```sh
B165_MODE=refusal B165_SAMPLES=10 \
  B165_RAW_OUT=/tmp/b165-live-20260908-drain.tsv \
  scripts/probe-wp-b165-latency.sh
python3 scripts/verify_b165_latency.py \
  /tmp/b165-live-20260908-drain.tsv --samples 10
```

Cloudflare read-only deployment context at observation time:

- `wrangler deployments status --env prod` reported deployment
  `7048b4b3-6752-4b09-8580-a44d10611c9f`, created 2026-08-31.
- The repository `wrangler.toml` production container pin is `ddd95560-r1`.
- The current worktree timing changes therefore are not claimed as deployed
  production behavior.

Acceptance remains blocked until the owner supplies two distinct customer
tenants, two distinct customer PATs, and one known served path per tenant for
10-sample authenticated served populations. This refusal-only report does not
close B-165 and must not be used to infer a new production bundle.
