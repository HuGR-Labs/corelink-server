# B-165 latency drain evidence — 2026-09-08

Status: **partial/open; served path remains unmeasured**

This is a refusal-only capture from the Mac workstation. It used 10
unauthenticated samples per refusal route and `/health`, discarding sample 1 as
cold. No new request was made while correcting this report. The local files
contain no credential values; the verifier output contains no response bodies.
Both commands intentionally returned exit 2: this is the fail-closed
refusal-only result, not a closure.

| population | status | retained | median | p90 |
|---|---:|---:|---:|---:|
| `/v1/cas/x/y` | 401 | 9 | 107.7 ms | 356.6 ms |
| `/npm/x` | 401 | 9 | 89.7 ms | 177.7 ms |
| `/pip/simple/x` | 401 | 9 | 88.3 ms | 203.5 ms |
| `/v2/x/manifests/latest` | 401 | 9 | 294.0 ms | 568.3 ms |
| `/health` control | 200 | 9 | 73.8 ms | 132.9 ms |

The exact commands used, including the output redirections, were:

```sh
B165_MODE=refusal B165_SAMPLES=10 \
  B165_RAW_OUT=/tmp/b165-live-20260908-drain.tsv \
  scripts/probe-wp-b165-latency.sh \
  >/tmp/b165-live-20260908-drain.log 2>&1
probe_rc=$?
python3 scripts/verify_b165_latency.py \
  /tmp/b165-live-20260908-drain.tsv --samples 10 \
  >/tmp/b165-live-20260908-drain.verify.json \
  2>/tmp/b165-live-20260908-drain.verify.err
verify_rc=$?
test "$probe_rc" -eq 2 -a "$verify_rc" -eq 2
```

Local artifact receipt (filesystem mtime, timezone `-03:00`; this is not a
claim about request start time):

| artifact | mtime | SHA-256 |
|---|---|---|
| raw TSV | `2026-09-08T22:52:52-0300` | `20effd935f2cf585b93de607660921e32580a3f586993c8bb1a2f4004e45036e` |
| probe log | `2026-09-08T22:52:53-0300` | `140985e6887c7db3274015ccbce44a205aea8ebebeeb9987d10302c1f5bfe759` |
| verifier JSON | `2026-09-08T22:52:53-0300` | `c178c40848e440c5cef77c7f7d8f783bed9b1b27873a995e1c0323e1414e0e1b` |
| verifier stderr | `2026-09-08T22:52:53-0300` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

The approved owner packet (`docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-165`)
specifies one dedicated `cas:read` PAT and known served test objects. The
current approved probe/runbook also enforces two distinct tenant/PAT bindings
for the served half, matching the newer B-165 completeness rule in
`docs/campaigns/remediation/work-packages/B131-B167.md`. This is an unresolved
owner contract mismatch, not evidence: the served half remains blocked until
the owner either supplies the second distinct tenant/PAT or updates and
approves the packet. The verifier now requires explicit distinct tenant
bindings and rejects reused paths.

No deployed-version receipt is included: the earlier read-only deployment
lookup was not captured as a transcript. B-102/B-107/B-122/B-129 therefore
remain blocked on a fresh owner-provided bundle identity and authenticated
evidence. This refusal-only report does not close B-165.
