# Execution-log fixtures

These are real `--execution_log_json_file` outputs, captured from Bazel 9.2.0
building a one-genrule workspace, and they exist so `test_cache_hit_ratio.sh`
asserts against what Bazel actually writes rather than against what the parser
assumes.

| File | How it was produced | What it pins |
|---|---|---|
| `remote-hit.json` | warm build against an HTTP remote cache | `runner: "remote cache hit"`, `cacheHit: true` — a hit that counts |
| `disk-hit.json` | warm build against `--disk_cache` | `runner: "disk cache hit"` — a hit that must NOT count as remote |
| `local-exec.json` | derived from `remote-hit.json` by setting the two cache fields to the values observed on a locally executed spawn (`runner: "darwin-sandbox"`, `cacheHit: false`) | a remote-cacheable spawn that missed |

Every one of them is pretty-printed across many lines. That is the point: the
parser this repo shipped for a year read the file line by line and therefore
reported "no remote cache entries" for all three (BACKLOG B-017).
