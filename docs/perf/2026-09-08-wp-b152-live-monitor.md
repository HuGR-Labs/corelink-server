# B-152 live monitor evidence — 2026-09-08

Status: **indeterminate/open; no causal closure**

The bounded read-only collector was run against the GitHub Actions API from a
local authenticated `gh` session for the UTC window
`2026-09-08T18:20:00Z` (inclusive) through `2026-09-08T19:00:00Z`
(exclusive):

```sh
PYTHONPATH=scripts python3 scripts/b152_actions_diagnostic.py \
  --start 2026-09-08T18:20:00Z --end 2026-09-08T19:00:00Z \
  --fetch-logs --output /tmp/b152-live.json
```

The API returned eight runs. All eight had `conclusion=startup_failure`, with
zero jobs and zero failed jobs; consequently there was no 600-second job match
and no job-log request. This is a control-plane/startup population, not
evidence that the historical B-152 job-death cause was fixed or reproduced.
The report classified the run set as `non_success_run_count=8`, retained the
absence-of-cure limitation, and exited 0 because the metadata walk was
complete. A zero `window_jobs` count is explicitly non-closing.

The three historical B-152 job-log endpoints were independently checked by
`gh api` on this date. Each returned HTTP 404; only status/size/digest metadata
was retained and no log body was printed or persisted:

| job ID | API result | body bytes |
|---:|---:|---:|
| 99377079175 | HTTP 404 | 213 |
| 99378740877 | HTTP 404 | 213 |
| 99388658702 | HTTP 404 | 213 |

The historical signature remains confirmed by retained metadata, but the
common cause remains unclassified. The scheduled hosted-runner monitor in
`.github/workflows/b152-actions-monitor.yml` now collects a fresh 20-minute
window every 15 minutes, stores metadata for 30 days, and preserves an
explicit `indeterminate` artifact when the API walk fails. It cannot close
B-152 from a clean window or from an available log without a causal chain.
