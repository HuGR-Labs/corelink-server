# `corelink-hash` clippy and wasm check — 2026-09-22

Execution mode: `LOCAL_ISOLATED`, offline, no deployment or runtime access.

- `cargo clippy --locked --offline -p corelink-hash -- -D warnings` — **PASS**.
- `rustup target list --installed` confirmed `wasm32-unknown-unknown` present.
- `cargo check --locked --offline -p corelink-hash --target wasm32-unknown-unknown` — **PASS**.

The commands ran on the source-equivalent checkout described by the hash test
evidence. They prove only local lint/target compilation; they do not prove
Cloudflare bindings, deploy, runtime reachability or consumer compatibility.
