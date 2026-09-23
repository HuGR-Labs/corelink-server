# Backlog/deduplication closeout — final four rows (2026-09-22)

Source readback: `origin/main` at
`f9f6eccace9765b9061e71d8d975eee08bffe8fa`; BACKLOG blob
`1d2dc27c523164b85c07a71aa9c4e1be3adffb04`; authenticated snapshot: 267
issues. No ownership title or manifest marker was found and no write occurred.

| Package | Decision | Scope note |
|---|---|---|
| `corelink-tracing` | DISTINCT | No trace-context/OTLP ownership candidate; Server-Timing tickets are separate. |
| `corelink-wasm` | DISTINCT | No package, wasm-bindgen, npm SDK or wasm-pack ownership candidate. |
| `corelink-worker-fuzz` | DISTINCT | Fuzz harness is separate from shared nightly/runner infrastructure tickets. |
| `e2e-replication-failover` | DISTINCT | Harness failover/split-brain scope is separate from D1 residency and probe/hysteresis work. |

This closes the semantic dedup census: 105/105 eligible packages have an
explicit `DISTINCT`, `REUSE`, or `EXPAND` decision. `DISTINCT` means no
duplicate ownership issue was found; it does not mean related runtime work is
absent. `REUSE`/`EXPAND` rows still require ledger confirmation before
publication.
