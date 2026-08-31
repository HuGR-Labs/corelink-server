### Fixed

- **`github.ref` collapsed `push`, `schedule` and `workflow_dispatch` into one
  concurrency group on `main` (B-136, B-137).** All three resolve to
  `refs/heads/main`, and `cancel-in-progress: true` was on, so a merge that
  touched `BACKLOG.md` could cancel the in-flight daily cron — and a manual
  dispatch could cancel a running nightly. The failure mode is the dangerous
  one: the run does not go red, it goes *absent*.

  Six workflows now interpolate `${{ github.event_name }}` alongside the ref:
  `backlog-verify` (B-136) plus `byok_kill_switch_drill_weekly`,
  `byok_matrix_weekly`, `dr-drill-monthly`, `nightly` and `perf-nightly`
  (B-137). Population measured on both sides, not sampled: 6 of 6 group lines
  lacked `github.event_name` before, 6 of 6 carry it now.

  `cancel-in-progress` was deliberately left on for all six. Serialising a costly
  nightly instead of scoping it is a legitimate answer and B-137 says so, but
  scoping closes the collision without changing any lane's cost.
