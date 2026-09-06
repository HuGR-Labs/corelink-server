---
type: "CrateCluster"
title: "Observability plane (telemetry / tracing / SLO)"
description: "The OTel/tracing + multi-burn-rate SLO + logpush/canary/synthetic-pager primitives — STATUS: pure-logic trait+fake skeletons (production wiring deferred to WI-S09-007/S-20); the ONLY symbol consumed in prod handlers is the `Sli` taxonomy enum."
source_files:
  - "crates/corelink-telemetry/src/lib.rs"
  - "crates/corelink-telemetry/src/otel/exporter.rs"
  - "crates/corelink-telemetry/src/logpush/sink.rs"
  - "crates/corelink-telemetry/src/canary.rs"
  - "crates/corelink-telemetry/src/synthetic_pager/decide.rs"
  - "crates/corelink-tracing/src/lib.rs"
  - "crates/corelink-tracing/src/context.rs"
  - "crates/corelink-tracing/src/sampler.rs"
  - "crates/corelink-tracing/src/service.rs"
  - "crates/corelink-tracing/tests/prop_tracing.rs"
  - "crates/corelink-slo/src/lib.rs"
  - "crates/corelink-slo/src/window.rs"
  - "crates/corelink-slo/src/calculator.rs"
  - "crates/corelink-slo/src/decision.rs"
  - "crates/corelink-slo/src/alert.rs"
  - "crates/corelink-slo/src/pagerduty.rs"
  - "crates/corelink-slo/src/definition.rs"
  - "crates/corelink-handler-cas/src/observer.rs"
source_blobs:
  - "crates/corelink-telemetry/src/lib.rs@1b5e6047daa5785a395823959b694836eb29470f"
  - "crates/corelink-telemetry/src/otel/exporter.rs@02e74cbc4ec5db698f7eb1250570a8ed245dc9dc"
  - "crates/corelink-telemetry/src/logpush/sink.rs@403d6c2d05191fac06e7fc2538db2384484f01c6"
  - "crates/corelink-telemetry/src/canary.rs@08bc8092366b2656319f4a706c1bd07835f569e9"
  - "crates/corelink-telemetry/src/synthetic_pager/decide.rs@3a42888ce0ae3f1512e919c9b9beb70c3172694e"
  - "crates/corelink-tracing/src/lib.rs@4d67172dac125b5c0dbe0d26c6b47ac9f3babc95"
  - "crates/corelink-tracing/src/context.rs@a51a52b06c8bec3c5d01401617739b28df787339"
  - "crates/corelink-tracing/src/sampler.rs@284586ef46ba7e86d9e45f78e59c4d2cca3f1609"
  - "crates/corelink-tracing/src/service.rs@9d530f6943d817413b324d96e6af250e7805de03"
  - "crates/corelink-tracing/tests/prop_tracing.rs@c52a652fa52286b2023d62503e92302b57b97d98"
  - "crates/corelink-slo/src/lib.rs@ab88f25561880927d8919c0052cb4d887ca13c69"
  - "crates/corelink-slo/src/window.rs@50139375dc478c41bdb3520ed826050a2e1857fa"
  - "crates/corelink-slo/src/calculator.rs@00711e816e0e7b132d90faac4534b003fe522a0f"
  - "crates/corelink-slo/src/decision.rs@77d7d0469af2c7db6c16005e07364ef86944870d"
  - "crates/corelink-slo/src/alert.rs@341f781afa1641e1b78ff3f86fc0f93d298c867f"
  - "crates/corelink-slo/src/pagerduty.rs@f441a5e050e731519bc0a4cd2f8c893d2838fbd6"
  - "crates/corelink-slo/src/definition.rs@d2856258e269c38847b0cb3b030053bb0630f045"
  - "crates/corelink-handler-cas/src/observer.rs@5d499c7be8e8036bf43b7c837d6994f094b069a8"
checkpoint_sha: "fc7ec9bb9c5d8711cabc4b93c989062e71d2955f"
provenance: "AUTHORED"
tags: ["observability", "telemetry", "tracing", "slo"]
timestamp: "2026-06-28T00:00:00Z"

---
# Observability plane (telemetry / tracing / SLO)

A multi-tenant cache that cannot see its own error budget burn fast enough to page before the budget is spent is flying blind — so this cluster designs the W3C-distributed-tracing + Google-SRE multi-burn-rate-SLO + structured-logging + synthetic-canary substrate. **STATUS — read this first: it is, at HEAD, almost entirely a pure-logic skeleton.** `corelink-tracing`, `corelink-slo`, and every submodule absorbed into `corelink-telemetry` (`otel` / `logpush` / `canary` / `synthetic_pager` / `lighthouse`) ship the *trait surface* every production binding will satisfy plus an *in-memory fake* that exercises the algorithmic invariants, and explicitly defer the real network wiring (Tempo OTLP HTTP exporter, PagerDuty Events API v2 POST, CF Logpush/R2/Loki fan-out, CF-Workers cron canary) to WI-S09-007 / S-20. The single load-bearing exception that touches the live data plane today is the `Sli` taxonomy enum, which the request handlers re-export through their `observer.rs` SLI emit points.

# Role

This is the *designed* observability plane backing the [Grafana-Cloud-vs-self-hosted](/adr/adr-0017-grafana-cloud-vs-self-hosted-observability.md) decision and the SLO catalog. In production-intent it sits beside the CAS/AC hot path: handlers emit `SliObservation`s, the burn-rate evaluator maps them onto the Google SRE Workbook Ch 5 Table 4 alert matrix, and the tracing/logpush/canary primitives feed Tempo/Loki/Mimir. In *reality* at HEAD the plane is a workspace of self-contained, fully-property-tested skeleton crates: `corelink-telemetry` is an Option-A aggregator that physically absorbed 5 of its 7 tenants as inline `pub mod` submodules and still re-exports `tracing` + `slo`, and nothing in any worker/container route consumes the alerting, dispatch, or trace-export paths.

# How it works

- `corelink-telemetry` is an **Option-A façade/aggregator**, not a behavioral layer: its `lib.rs` re-exports/owns `canary`, `lighthouse`, `logpush`, `otel`, `slo`, `synthetic_pager`, `tracing` as `pub mod`s; the crate's only executed test resolves the seven `module_path_marker()` strings (`crates/corelink-telemetry/src/lib.rs:83-89`).
- **Tracing — W3C Trace Context parser is real pure-logic, exporter is a fake.** `parse_traceparent` strictly enforces the 55-char canonical form, the three dash positions, the `00`-only version, lowercase hex, and rejects all-zero trace/span IDs via `TraceContext::new` (`crates/corelink-tracing/src/context.rs:149-194`; `crates/corelink-tracing/src/context.rs:94-104`). The `RateBasedSampler` is head-based-deterministic: last 8 bytes of the trace_id as a big-endian `u64` compared against `rate × u64::MAX` in u128 space, so the same trace samples identically at every hop (`crates/corelink-tracing/src/sampler.rs:186-218`). The `TracingService` orchestrator emits the `SpanStarted` + `SamplerDecision` audit records BEFORE inserting into the `(tenant_tier, span_id)`-keyed in-flight ledger (`crates/corelink-tracing/src/service.rs:208-227`).
- **SLO — the burn-rate decision tree is real pure-logic; the PagerDuty dispatcher is an in-memory fake.** `BurnRateWindow::threshold_multiplier` pins the Google SRE Table 4 multipliers 14.4× / 6× / 3× / 1× (`crates/corelink-slo/src/window.rs:49-56`). `BurnRateCalculator::decide` computes `error_rate = errors/total`, and at/above `multiplier × error_budget_pct` maps Fast1h→`PageSev0`, Medium6h→`PageSev1`, Slow24h→`TicketSev2`, Long3d→`TicketSev3`, else `Quiet` (`crates/corelink-slo/src/calculator.rs:107-127`). `MultiBurnRateAlert::evaluate` runs the audit-emit-BEFORE-mutation envelope, dispatches a page only when `AlertDecision::is_page()`, and constructs the canonical dedup key `{sli_slug}:{window_slug}:{tenant_id}` (`crates/corelink-slo/src/alert.rs:155-209`). The `InMemoryPagerDutyDispatcher` honors Events-API-v2 dedup-key idempotency in a `HashMap`, NOT an HTTPS POST (`crates/corelink-slo/src/pagerduty.rs:238-253`).
- **The one prod-wired symbol is the `Sli` enum.** `corelink-slo::definition::Sli` is the closed 18-variant SLI taxonomy with `slug()` + `prometheus_metric_base()` (`crates/corelink-slo/src/definition.rs:97-118`), and CAS/AC/admin/customer handlers re-export it through their `observer.rs` SLI emit point + `InMemorySliObserver` (`crates/corelink-handler-cas/src/observer.rs:10`; `crates/corelink-handler-cas/src/observer.rs:92-100`).
- **Telemetry submodules are skeletons too.** The `otel` exporter's vendor adapters are `trait + fake`: `export_batch_fail_open` swallows a vendor `ExporterError` into a `tracing::debug!` and returns `Ok(())` (fail-OPEN), real HTTP/gRPC deferred (`crates/corelink-telemetry/src/otel/exporter.rs:106-128`). The `logpush` `InMemoryLogSink::emit` runs cardinality-guard → redaction → audit → in-memory NDJSON buffer push (NOT CF Logpush/R2/Loki) (`crates/corelink-telemetry/src/logpush/sink.rs:180-217`). The synthetic-pager `decide_drill_outcome` is a pure MTTA classifier with no real pager (`crates/corelink-telemetry/src/synthetic_pager/decide.rs:32-62`). The `canary` crate's own lib documents itself as a pure-logic skeleton whose live CF-Workers cron + `worker::Fetch` probe is deferred to S-20 (`crates/corelink-telemetry/src/canary.rs:142-148`).

# Invariants

- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (designed, enforced in the fakes): every state mutation is preceded by its audit emit, and an audit-sink failure aborts the path fail-CLOSED — in tracing's `start_span` (`crates/corelink-tracing/src/service.rs:208-222`) and in the SLO orchestrator's evaluate envelope (`crates/corelink-slo/src/alert.rs:163-209`).
- **INV-TENANT-ISOLATION** (in-memory deferred-façade — per-TIER bucketing, NOT per-tenant): the tracing in-flight ledger key is `InFlightKey { tenant_tier, span_id_hex }` where `tenant_tier` is a **billing-plan tier** (Free/Solo/Team), NOT a tenant id (`crates/corelink-tracing/src/service.rs:130-132`). So the key namespace is partitioned **per tier, not per tenant** — two DISTINCT tenants on the SAME tier share one key namespace and can collide on a span id; "a cross-tenant span lookup is impossible" is therefore FALSE. (The proof test forces two DISTINCT tiers, so it never exercises the same-tier-two-tenant collision — `crates/corelink-tracing/tests/prop_tracing.rs:300-304`.) The practical risk is ~zero because this whole plane is an in-memory, not-yet-prod-wired skeleton façade, but it must NOT be claimed as per-tenant isolation. The SLO orchestrator keeps a per-tenant **evaluation-count ledger** (`PerTenantLedger::evaluation_count`, a monotonic count of `evaluate` calls per tenant — NOT flapping detection, which is deferred) under a per-instance `Arc<Mutex<>>` (`crates/corelink-slo/src/alert.rs:213-223`).
- **W3C Trace Context strict reject** (real): malformed length / dash positions / non-`00` version / uppercase or malformed hex / all-zero IDs are all rejected by the parser (`crates/corelink-tracing/src/context.rs:151-193`).
- **Google SRE Table 4 boundary discipline** (real pure-logic): the multipliers are pinned at the type-system layer and the decision arm fires at-or-above `multiplier × error_budget_pct` (`crates/corelink-slo/src/window.rs:49-56`; `crates/corelink-slo/src/calculator.rs:114-127`).
- **PagerDuty dedup idempotency** (designed, modeled in the fake): a repeated `Trigger` with an open dedup key is collapsed via `HashMap::entry(..).or_insert` — no second incident (`crates/corelink-slo/src/pagerduty.rs:239-243`).
- **Page/ticket/quiet partition** (real): `AlertDecision::is_page` / `is_ticket` / `is_quiet` are the predicates the orchestrator branches on; `is_page` is true exactly for `PageSev0`/`PageSev1` (`crates/corelink-slo/src/decision.rs:57-72`).

# Gotchas

- **This whole plane is a skeleton — do not cite it as live monitoring.** The crate-level docs say it plainly: tracing is `trait + fake` with the real Tempo OTLP HTTP exporter at WI-S09-007 (`crates/corelink-tracing/src/lib.rs:107-116`), SLO defers the live PagerDuty Events API v2 POST + Terraform + flapping cron to the same gate (`crates/corelink-slo/src/lib.rs:111-129`), and the canary defers its CF-Workers cron probe to S-20 (`crates/corelink-telemetry/src/canary.rs:111-129`). No worker/container route imports the alerting, dispatch, or trace-export paths.
- **Doc-comment drift on the page severity mapping.** `corelink-slo/src/lib.rs` prose says fast-burn is "`PageSev0` or `PageSev1`", but the EXECUTED `calculator.decide` deterministically maps Fast1h→`PageSev0` and Medium6h→`PageSev1` (`crates/corelink-slo/src/calculator.rs:121-126`) — trust the code, not the summary.
- **`corelink-telemetry` is an aggregator, not where the logic lives.** 5 of 7 tenants were physically absorbed as inline submodules and 2 (`tracing`, `slo`) remain pure re-exports of the standalone crates; importing `corelink_telemetry::slo::*` and `corelink_slo::*` reach the same types (`crates/corelink-telemetry/src/lib.rs:56-78`).
- **The fail-OPEN vs fail-CLOSED split is deliberate.** Observability *export* fails OPEN (a vendor outage must not take the request down — `otel`/`logpush`/tracing-exporter), while the *audit* envelope around every emit fails CLOSED (`crates/corelink-telemetry/src/otel/exporter.rs:106-128`; `crates/corelink-telemetry/src/logpush/sink.rs:29-40`).
- **Only the `Sli` enum is load-bearing in prod.** If you need real SLI emission today, it flows through the handler `observer.rs` modules over `corelink_slo::definition::Sli`; everything downstream of `SliObservation` (the burn-rate evaluator, the dispatcher) is not yet wired (`crates/corelink-handler-cas/src/observer.rs:10`; `crates/corelink-handler-cas/src/observer.rs:44-51`).

# Citations

1. `crates/corelink-telemetry/src/lib.rs:83-89` — the Option-A aggregator: 7 submodules re-exported/owned as `pub mod`s.
2. `crates/corelink-telemetry/src/lib.rs:56-78` — Wave-35 absorption note: 5 tenants inlined, `tracing`+`slo` stay re-exports.
3. `crates/corelink-tracing/src/lib.rs:107-116` — tracing is `trait + fake`; real Tempo OTLP HTTP exporter deferred to WI-S09-007.
4. `crates/corelink-tracing/src/context.rs:149-194` — strict W3C `traceparent` parse (length/dash/version/hex/all-zero rejects).
5. `crates/corelink-tracing/src/context.rs:94-104` — `TraceContext::new` rejects all-zero trace_id / span_id.
6. `crates/corelink-tracing/src/sampler.rs:186-218` — head-based deterministic sampler (last-8-bytes u64 vs `rate × u64::MAX`).
7. `crates/corelink-tracing/src/service.rs:208-227` — audit-emit-BEFORE-mutation + `(tenant_tier, span_id)`-keyed in-flight ledger.
8. `crates/corelink-slo/src/window.rs:49-56` — Google SRE Table 4 multipliers 14.4× / 6× / 3× / 1× pinned.
9. `crates/corelink-slo/src/calculator.rs:107-127` — `decide`: error_rate ≥ threshold → Fast1h=PageSev0 / Medium6h=PageSev1 / Slow24h=TicketSev2 / Long3d=TicketSev3 / else Quiet.
10. `crates/corelink-slo/src/alert.rs:155-209` — evaluate envelope: audit-before-mutation, page IFF `is_page()`, canonical dedup key.
11. `crates/corelink-slo/src/alert.rs:213-223` — per-tenant evaluation-count ledger (`PerTenantLedger::evaluation_count`, counts `evaluate` calls; NOT flapping detection) under per-instance `Arc<Mutex<>>`.
11a. `crates/corelink-tracing/src/service.rs:130-132` — the in-flight ledger key is `InFlightKey { tenant_tier, span_id_hex }`; `tenant_tier` is a billing-plan tier, NOT a tenant id → per-TIER bucketing, NOT per-tenant isolation (INV-TENANT-ISOLATION corrected).
11b. `crates/corelink-tracing/tests/prop_tracing.rs:300-304` — `prop_tenant_isolation` forces two DISTINCT tiers (`tier_b_offset >= 1`), so it never exercises the same-tier-two-tenant span-id collision.
12. `crates/corelink-slo/src/pagerduty.rs:238-253` — in-memory dedup-key idempotency (HashMap), NOT a real HTTPS POST.
13. `crates/corelink-slo/src/lib.rs:111-129` — production wiring (PagerDuty POST / Terraform / flapping cron / Twilio) deferred to WI-S09-007.
14. `crates/corelink-slo/src/definition.rs:97-118` — `Sli` closed taxonomy `slug()` + `prometheus_metric_base()`.
15. `crates/corelink-handler-cas/src/observer.rs:10` — the one prod-consumed symbol: `pub use corelink_slo::definition::Sli`.
16. `crates/corelink-handler-cas/src/observer.rs:92-100` — `InMemorySliObserver::observe` best-effort SLI capture in the handler.
17. `crates/corelink-telemetry/src/otel/exporter.rs:106-128` — `export_batch_fail_open` swallows vendor error to `debug!` + `Ok(())` (fail-OPEN; real client deferred).
18. `crates/corelink-telemetry/src/logpush/sink.rs:180-217` — `InMemoryLogSink::emit` cardinality→redaction→audit→in-memory buffer (not CF Logpush/R2/Loki).
19. `crates/corelink-telemetry/src/synthetic_pager/decide.rs:32-62` — pure MTTA drill-outcome classifier (no real pager).
20. `crates/corelink-telemetry/src/canary.rs:142-148` — canary `pub mod`s; lib documents the live CF-Workers cron probe as deferred to S-20.
21. `crates/corelink-slo/src/decision.rs:57-72` — `AlertDecision::is_page`/`is_ticket`/`is_quiet` predicates the orchestrator branches on.
