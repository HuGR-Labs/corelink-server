# Local diagnostic readback — 2026-09-23

Scope: five representative pilots and only local, bounded, offline diagnostics. No deployment, credentials, network access, GitHub writes, or production systems were used. This readback records command evidence and blockers; it does not claim runtime reachability.

## Checkout and local toolchain

- Checkout: `/private/tmp/corelink-ownership-campaign`.
- At `2026-09-23 05:37:29 UTC`, `git rev-parse HEAD` exited 0 with `b47f3cf1d8865c8a15c6c061c449b6989ee22a9c`.
- At the same time, `git status --short` exited 0 and listed pre-existing/concurrent edits to `.claude/skills/own-corelink-meta-fuzz/SKILL.md`, `docs/ownership/crates/corelink-adapter-host/MAINTENANCE.md`, `docs/ownership/crates/corelink-meta-fuzz/BLAST_RADIUS.md`, `docs/ownership/crates/corelink-meta-fuzz/REFERENCE.md`, `docs/ownership/crates/corelink-reapi/MAINTENANCE.md`, `docs/ownership/crates/corelink-worker/BLAST_RADIUS.md`, `docs/ownership/crates/corelink-worker/REFERENCE.md`, and `docs/ownership/crates/e2e-failover-router/REFERENCE.md`, plus this new readback. These unrelated paths were left untouched.
- At `2026-09-23 05:37:29 UTC`, `rg -n 'channel =|targets\\s*=' rust-toolchain.toml` exited 0 and showed channel `1.91.1`, targets `wasm32-unknown-unknown` and `x86_64-unknown-linux-musl`.
- At `2026-09-23 05:37:29 UTC`, `rustup toolchain list` exited 0 and showed `1.91.1-x86_64-apple-darwin (active)`; `rustup target list --installed` exited 0 and showed both `wasm32-unknown-unknown` and `x86_64-apple-darwin` installed.
- At `2026-09-23 05:35:00 UTC`, `rustc -Vv` exited 0: Rust 1.91.1, host `x86_64-apple-darwin`; `cargo -V` exited 0: Cargo 1.91.1.

## Read-only package selection

At `2026-09-23 05:35:10 UTC`, command exited 0:

```sh
cargo metadata --locked --offline --no-deps --format-version=1 | jq -r '.packages[] | select(.name == "corelink-hash" or .name == "corelink-billing" or .name == "corelink-server" or .name == "corelink-cf-bindings" or .name == "e2e-billing-flow") | [.name,.version,.manifest_path,([.targets[].name]|join(",")),([.features|keys[]]|join(","))] | @tsv'
```

The metadata identified all five workspace packages and their declared targets/features. `corelink-server` maps to `crates/corelink-container/Cargo.toml`; its targets include `signup_pilot_live_d1`, so no broad or ambiguous server test target was selected. Package metadata does not establish source-pin equivalence or runtime behavior.

`corelink-server` PROC-001 also prescribes `cargo metadata --locked --offline --no-deps --format-version=1`. Its command was run at `2026-09-23 05:34:34 UTC`, filtered only for `corelink-server`, and exited 0. Output identified package `corelink-server` version `0.1.0`, manifest `crates/corelink-container/Cargo.toml`, declared binaries/tests, and features `byok-aws-real`, `byok-azure-real`, `byok-gcp-real`, `byok-vault-real`, `cf-billing-real`, `cf-r2-real`, and `default`. This is a partial selection diagnostic: PROC-001's documented source pin `91630baebe3ae7abe686cd4e06a5621ecdc4ab73` does not match this checkout, so the procedure is not certified complete.

The first filtered server metadata invocation printed the same package metadata but the surrounding zsh command exited 1 because it assigned to the read-only shell variable `status`. The Cargo and jq pipeline had produced output; the corrected invocation immediately afterward exited 0. The failed wrapper attempt is not treated as a package failure or execution evidence.

## Pilot procedure outcomes

| Pilot | Documented source pin | Safe local action and result | Procedure disposition |
|---|---|---|---|
| `corelink-hash` | `cacc44fc43ee2f481e933419ba88b9a19ac6c8e8` | PROC-001 requires the exact checkout pin before preparing a build. Current HEAD is `b47f3cf1d8865c8a15c6c061c449b6989ee22a9c`; no hash tests/build were run. | PROC-001 blocked at source-pin precondition; PROC-002/004/005 not run. Existing prior evidence remains source-equivalent evidence only. |
| `corelink-billing` | `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` | Workspace metadata confirmed package and targets, but not the pinned source. The documented package graph command was not run. Current HEAD differs from the pin. | PROC-001/002+ blocked pending the pinned checkout; no Cargo tests/builds run. |
| `corelink-server` | `91630baebe3ae7abe686cd4e06a5621ecdc4ab73` | PROC-001 read-only metadata selection ran successfully as recorded above. Current HEAD differs from the pin. | PROC-001 selection substeps observed, full procedure incomplete due to pin mismatch. PROC-002/003/004/006 builds/tests not run; no remote procedure run. |
| `corelink-cf-bindings` | `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` | Metadata confirmed package target `prop_cas_idempotency` and declared package features. Wasm `cargo check` was not run because the documented pin differs and an isolated cold build would not certify it. | PROC-004 blocked pending pinned source; PROC-001–003 incomplete; PROC-005 remains blocked for operation. |
| `e2e-billing-flow` | `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` | Metadata confirmed target `end_to_end`. No test was run: checkout pin differs, the procedure's existing record says Cargo is prohibited in that review, and the prescribed fresh target would require an unbounded cold dependency build. | PROC-005 not run; no billing/Stripe/DSR runtime claim. |

All Cargo build/test commands were kept offline or were not attempted. We did not create a target directory, modify Cargo.lock, load environment secrets, or contact services. A source-pin mismatch is a blocker to the procedures' claimed baseline, not evidence that the packages fail.

## Worktree check

At `2026-09-23 05:37:45 UTC`, `git diff --check` exited 0 with no whitespace errors. `git diff --no-index --check /dev/null docs/ownership/evidence/revision-1.4/LOCAL-DIAGNOSTICS-READBACK-20260923.md` exited 1 because the new file differs from `/dev/null`; it emitted no whitespace diagnostics. No commit was made.
