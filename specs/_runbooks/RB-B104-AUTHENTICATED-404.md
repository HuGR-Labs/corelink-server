# RB-B104 — authenticated 404 latency evidence

This runbook measures the served miss path for an authenticated, nonexistent
`GET /cargo/<tenant>/<random-key>`. It is read-only: every key is generated
fresh and no PUT, DELETE, deployment, or Cloudflare state mutation is allowed.

## Preconditions

The operator must obtain the deployed Worker/container version from the
deployment authority and export `PROBE_VERSION`. A read-capable PAT for the
selected tenant is required as `PROBE_TOKEN`; it is written to a mode `0600`
temporary header file and is never printed or passed as a process argument.

```sh
export PROBE_BASE=https://<approved-staging-origin>
export PROBE_VERSION=<deployed-version-or-sha>
export PROBE_TOKEN=<read-capable-pat>
PROBE_SAMPLES=10 ./scripts/probe-authenticated-404.sh
```

The probe refuses to run without a version, a UUID tenant, at least ten
samples, an HTTPS origin, and a PAT. It reports `INDETERMINATE` on transport,
timeout, empty-body, malformed status/timing, or missing attribution.

The canonical hosted lane is the manual
`.github/workflows/issue-1660-b104-authenticated-404.yml` workflow. It is
bound to protected `main` and the `staging` environment, accepts only an
HTTPS origin whose host identifies `staging`, `stage`, or `stg`, and requires
the environment secret `CORELINK_B104_STAGING_PAT`. The dispatch requires the
deployed version and both exact read-only acknowledgements. If staging or its
PAT is unavailable, the lane fails closed; production is not an acceptable
substitute. The retained log labels the target `staging-origin-redacted` and
does not retain the PAT, generated object keys, headers, or response bodies.

## Controls and population

The controls run before the timing population:

1. `GET /health` without credentials must return `200`.
2. `GET /cargo/<tenant>/<random-key>` without credentials must return `401` or
   `403`, proving the target is not an unauthenticated 404.

The population then sends fresh random keys with the PAT. Every request must
return `404`; a `401`, `403`, other status, transport failure, or partial
population invalidates the run. Reported wall time is retained for context,
but the phase statistics are the attribution record. The script reports the
arithmetic median and nearest-rank p90 for each phase and deliberately omits
the maximum.

## Attribution contract

The final `Server-Timing` header is parsed using a fixed, bounded vocabulary:

| phase | owner | meaning |
| --- | --- | --- |
| `auth` | Worker | PAT authentication phase; `desc` cache tier is ignored |
| `wdb` | Worker | quota and residency work between auth and origin |
| `origin` | Worker | DO/container subrequest |
| `ohop` | Worker | residual DO dispatch and wire time |
| `opat` | container | PAT row plus cargo URL map co-read |
| `oaccounting` | container | URL map/accounting D1 windows when separately emitted |
| `ostore` | container | durable blob work; absent on a URL map miss |
| `ohandler` | container | routing, rate limit, HMAC, and response assembly |
| `total` | Worker | end-to-end response time, including the 404 timing pad |

`oother` is accepted only as the temporary compatibility alias for
`ohandler`, and is counted once. Unknown names are ignored. A missing required
phase fails the run, so an old deployment cannot look fast because it omitted
instrumentation. On the miss path, `404` status plus `opat` and absent
`ostore` identify the URL map miss without treating the miss as a successful
storage read.

The timing pad is a security control and must remain enabled. It means `total`
is not a raw server execution profile; the unpadded attribution phases are the
repository-owned evidence used to separate auth, routing, and miss work. The
probe reports median and nearest-rank p90 for end-to-end wall time plus every
present timing phase.

## Closure gate

Issue #1660 remains open until a fresh run is linked with the deployed version,
at least ten valid authenticated 404s, median and p90 phase values, and
redacted output. The canonical B-104 target is median latency in the order of
low tens of milliseconds for a miss. A lone maximum, a wall-time-only run, a
missing phase, or repository prose cannot close the issue. If `ohandler` or
`ohop` dominates, the next change must name that owner before optimization;
the probe itself never changes padding or route behavior.
