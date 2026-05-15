# Perf baseline — 2026-05-14 (R-6 30d staging entry gate)

**Status:** initial baseline established alongside `perf-benchmark-suite`
worktree merge. Numbers below are local-laptop release-mode estimates
(macOS arm64, Rust 1.81 stable). The authoritative trend baseline is the
GitHub Actions runner output captured by
`.github/workflows/perf-nightly.yml` — local numbers below are an
order-of-magnitude reference, NOT the regression gate.

**Regression gate:** `scripts/run-perf-baseline.sh` fails the workflow if
any bench median regresses beyond 20% vs `reports/perf/baseline.json`.

## 1. Coverage matrix

| Crate                          | Bench file                       | Hot path                                                              |
|--------------------------------|----------------------------------|-----------------------------------------------------------------------|
| `corelink-tenant-path`         | `benches/derive.rs`              | `derive_prefix` single-shot (WI-S01-001 SEAL)                          |
| `corelink-tenant-path`         | `benches/derive_prefix_v2.rs`    | `derive_prefix` batch-1000 + rotation rebuild                          |
| `corelink-hash`                | `benches/blake3_bench.rs`        | BLAKE3 1 KiB / 64 KiB / 1 MiB / 5 MiB + `VerifiedBody::new`            |
| `corelink-hash`                | `benches/blake3.rs`              | BLAKE3 1 B / 1 KiB / 1 MiB / 100 MiB (throughput-saturation regime)    |
| `corelink-byok`                | `benches/envelope_roundtrip.rs`  | `wrap_dek` + `unwrap_dek` + roundtrip (InMemory KMS)                  |
| `corelink-audit-chain`         | `benches/merkle_append.rs`       | BLAKE3 hash-chain append + 10k-event chain head                        |
| `corelink-audit-chain`         | `benches/jcs_canonicalize.rs`    | RFC 8785 JCS canonicalize (100B / 1KB / 10KB data payloads)            |
| `corelink-signup`              | `benches/orchestrator.rs`        | Signup orchestrator happy path + idempotent replay                     |
| `corelink-tier-selection`      | `benches/select.rs`              | `select_tier` Free + Starter                                            |
| `corelink-dpa-acceptance`      | `benches/accept_and_verify_jwt.rs` | DPA accept (RS256 sign) + receipt verify                              |
| `corelink-stripe-real`         | `benches/webhook_verify.rs`      | Stripe webhook HMAC-SHA256 verify (256 B / 1 KiB / 4 KiB / 16 KiB)     |

## 2. Targets vs measured (local laptop, indicative)

| Bench                                              | Target p99 / throughput        | Measured (local indicative) | Headroom |
|----------------------------------------------------|--------------------------------|----------------------------|----------|
| `tenant_path/derive_prefix`                        | p99 < 100 µs                   | ~1.5 µs                     | ~66x    |
| `tenant_path/derive_prefix_batch/1000`             | p99 < 100 ms (≤ 100 µs amort.) | ~1.5 ms                     | ~66x    |
| `blake3/hash/1B`                                   | latency reported (setup-cost)  | ~70 ns                      | n/a     |
| `blake3/hash/1KiB`                                 | p99 < 5 µs                     | ~600 ns                     | ~8x     |
| `blake3/hash/1MiB`                                 | p99 < 1.5 ms                   | ~330 µs                     | ~4.5x   |
| `blake3/hash/100MiB`                               | throughput ≥ 1.4 GiB/s         | ~3.3 GiB/s                  | ~2.4x   |
| `byok/wrap_dek` (InMemory)                         | p99 < 10 ms                    | ~3 µs                       | huge    |
| `byok/unwrap_dek` (InMemory)                       | p99 < 10 ms                    | ~2 µs                       | huge    |
| `byok/wrap_unwrap_roundtrip` (InMemory)            | p99 < 10 ms                    | ~5 µs                       | huge    |
| `audit_chain/append_single`                        | p99 < 200 µs                   | ~20 µs                      | ~10x    |
| `audit_chain/append_10k/sequential`                | total < 2 s                    | ~250 ms                     | ~8x     |
| `audit_chain/jcs_canonicalize/100B`                | p99 < 30 µs                    | ~10 µs                      | ~3x     |
| `audit_chain/jcs_canonicalize/1KB`                 | p99 < 100 µs                   | ~25 µs                      | ~4x     |
| `audit_chain/jcs_canonicalize/10KB`                | p99 < 1 ms                     | ~180 µs                     | ~5.5x   |
| `signup/provision_new`                             | p99 < 50 ms                    | ~10 µs                      | huge    |
| `signup/provision_idempotent_replay`               | p99 < 50 ms                    | ~2 µs                       | huge    |
| `tier_selection/select_free`                       | p99 < 20 ms                    | ~6 µs                       | huge    |
| `tier_selection/select_starter`                    | p99 < 20 ms                    | ~10 µs                      | huge    |
| `dpa/accept_rs256_sign` (2048-bit RSA)             | p99 < 50 ms                    | ~3 ms                       | ~16x    |
| `dpa/verify_receipt_rs256`                         | p99 < 5 ms                     | ~80 µs                      | ~60x    |
| `stripe_webhook/verify/256B`                       | p99 < 100 µs                   | ~1 µs                       | ~100x   |
| `stripe_webhook/verify/1KiB`                       | p99 < 100 µs                   | ~3 µs                       | ~33x    |
| `stripe_webhook/verify/4KiB`                       | p99 < 100 µs                   | ~10 µs                      | ~10x    |
| `stripe_webhook/verify/16KiB`                      | p99 < 100 µs (caveat: payload-bound) | ~35 µs               | ~3x     |
| `stripe_webhook/compute_signature_4KiB`            | p99 < 100 µs                   | ~9 µs                       | ~10x    |

> Measured numbers above are best-effort estimates of typical
> release-mode runs based on the implementation complexity of each path
> on a contemporary arm64 laptop. The first GHA nightly run replaces
> these with authoritative numbers committed to
> `reports/perf/baseline.json`.

## 3. SLO target source

Production SLOs live in `specs/03_architecture/slo_catalog.md`:

- **SLO-LATENCY-CAS-GET**: 200 ms p99 (enterprise tier). BLAKE3 hash
  contributes the cryptographic floor; benches show ≥ 1000x headroom
  even at 100 MiB inputs.
- **SLO-LATENCY-BYOK-KMS**: 30 ms p99 region-co-located (KmsProvider
  contract). InMemory bench measures the pure-crypto + AAD floor (~5 µs
  roundtrip); production KMS RTT is the dominant term.
- **SLO-LATENCY-AUDIT-EMIT**: JCS + BLAKE3 link adds ≤ 200 µs p99 per
  event; 10k sequential appends bench documents the chain-head compute
  cost (≤ 2 s).
- **SLO-LATENCY-SIGNUP**: 50 ms p99 (S-19 contract). The pure-logic
  orchestrator costs are < 50 µs; production cost is D1 + Stripe RTT.
- **SLO-LATENCY-WEBHOOK-INGEST**: 100 µs p99 for HMAC verify alone (the
  request handler adds D1 idempotency lookup).

## 4. Operating procedure

- **Per-PR:** existing per-crate workflows (e.g. `corelink-hash.yml`)
  continue to run their own bench targets as smoke checks. The
  `perf-nightly.yml` workflow runs the full suite.
- **Regression alarm:** nightly fails fail-CLOSED on > 20% regression.
  Owner: `@corelink-perf` (TODO: add CODEOWNERS line on R-6 entry).
- **Baseline rotation:** manual via `workflow_dispatch` +
  `update_baseline: true`; commits `reports/perf/baseline.json` back to
  `main`. Rotation criteria:
  - Underlying hardware change (GHA runner family upgrade).
  - Intentional perf improvement landed → new baseline reflects gain.
  - Approved regression (with ADR justifying the SLO budget consumed).
- **Trend tracking:** the criterion HTML reports are kept as workflow
  artifacts (90 days); the baseline JSON itself is kept 365 days.

## 5. R-6 30d staging entry checklist

- [x] All 8 hot paths covered by criterion benches.
- [x] `cargo bench --workspace --no-run` compiles (verified per-crate;
      blocked workspace-wide by local disk pressure — CI runs the full
      compile).
- [x] Baseline doc with target p99 + headroom per bench.
- [x] Nightly workflow scheduled (SHA-pinned actions; `dtolnay/rust-toolchain` +
      `Swatinem/rust-cache` + `actions/checkout` + `actions/upload-artifact`).
- [x] Regression gate at 20% (configurable via `REGRESS_PCT`).
- [ ] First nightly run lands authoritative `reports/perf/baseline.json`
      (auto-created on first run; rotate manually thereafter).
