# SOTA Performance Tooling Survey — CoreLink

**Date:** 2026-06-17
**Scope:** measure / profile / load-test / regression-gate / observe, for CoreLink
specifically (TS edge Worker → Durable Objects → Rust axum Containers → R2 + D1 + KV).
**Cost rule:** free / OSS / generous-free-tier first; paid only if "fucking awesome"
with real $ + concrete payoff. Enterprise-opaque = skip.

> CoreLink is *literally a cache*: latency, throughput, cache-hit-rate, R2
> egress/Class-A/B op counts, and per-request COGS **are the product**. Perf == margin
> (COGS ~$5/tenant, ~80% margin). CI runs on one overloaded shared Mac, so any
> wall-clock-based regression gate is structurally unreliable — **instruction-count /
> deterministic gates are the only ones that survive the noise.**

All version/pricing claims grounded in 2025-2026 sources (see footer). Where a number
could be stale, it is marked **(verify)**.

---

## The three biggest perf-visibility GAPS today (what's missing)

CoreLink has clippy + a reproducible-build gate, but appears to lack:

1. **A benchmark regression gate in CI** for the hot paths (BLAKE3/SHA-256 hashing,
   Argon2id PAT verify, blob read/write framing, D1 byte-accounting). No way today to
   catch a 20% slowdown before it ships. This is the #1 gap because perf == margin.
2. **Load/stress testing of the cache HTTP APIs** — no harness simulating `docker pull`
   fan-out, Bazel `FindMissingBlobs` bursts, Turbo PUT storms, or multi-tenant
   concurrency. We don't know where the container falls over or at what RPS R2
   Class-A/B op cost spikes.
3. **COGS-per-request attribution** — no tooling tying R2 ops + CPU-ms + D1 rows to a
   tenant/request. Margin is asserted, not measured. A single hot tenant doing
   pathological small-blob `HeadObject` storms could silently invert the margin.

Secondary gaps: no continuous/async-stall profiling (we use tokio + `block_in_place`,
a classic executor-stall foot-gun), no edge (Workers) perf observability wired up.

---

## 1. Rust micro/macro benchmarking + CI regression detection

The headline problem: **the gate must be deterministic on a noisy shared Mac.** That
rules out wall-clock as the *gate* (keep it as a local signal only). Instruction-count
(Callgrind) and CPU-simulation (CodSpeed) are the answer.

### iai-callgrind — `adopt-now` (the keystone)
One-liner: Valgrind/Callgrind-based harness measuring **instruction counts, cache
misses, branch misses** — deterministic, noise-immune, one-shot.
- **Cost:** free / OSS (Apache/MIT).
- **Fit:** Rust container **only** (needs the native binary + Valgrind; not Workers).
- **Effort:** medium. Add `iai-callgrind` dev-dep + harness; install `valgrind`.
  *Gotcha for CoreLink:* Valgrind is Linux-first — runs poorly/not-at-all on macOS
  ARM. So the **gate runs in a Linux CI container, not on the shared Mac** — which is
  actually ideal: it sidesteps the Mac-noise problem entirely AND offloads the Mac.
  Note the project recently forked/renamed activity around `gungraun`/`iai-callgrind`
  orgs — pin the crate you adopt **(verify current canonical crate + latest version)**.
- **Measures/gains:** sub-1% reliable detection of regressions in hashing, Argon2id,
  framing — exactly the hot paths. Each bench runs once (fast → good for shared infra).
- **Verdict: adopt-now.** This is the single best "regression gate that survives the
  shared-Mac noise" because it doesn't measure time at all.

### CodSpeed (cargo-codspeed) — `adopt-now` (free for this repo if structured right)
One-liner: hosted CI perf platform; runs your criterion/divan/`iai`-style benches under
**CPU simulation** (instruction-count based), posts PR reports, gates regressions.
- **Cost:** **Free** plan = unlimited runs/repos, 3-month history, up to 5 users on
  private repos (**unlimited users if the repo is OSS**), 600 macro-runner min/mo. Pro
  = **$15/user/mo** (unlimited history). Macro overage $0.013/min. **(verify, pricing
  page 2025/2026.)**
- **Fit:** Rust container benches (CPU-sim instrument is deterministic, same logic as
  `iai`). Not for Workers code.
- **Effort:** low-medium. `cargo-codspeed` subcommand + GitHub Action; supports
  **criterion AND divan** harnesses (Divan support landed Feb 2025), so you can reuse
  bench code. Macro (wall-time) instrument exists but use the **CPU-sim instrument** as
  the gate.
- **Verdict: adopt-now (trial first).** It's basically "iai-callgrind as a managed
  service with PR comments + history dashboards," removing the Valgrind/CI plumbing.
  Best DX. Caveat: CoreLink is private → Free plan caps at 5 users; fine for a solo
  founder. If you want zero external dependency / zero vendor, run iai-callgrind
  yourself instead. **Pick ONE of {iai-callgrind self-hosted, CodSpeed}** as the gate.

### divan — `adopt-now` (bench-authoring layer)
One-liner: modern, ergonomic Rust bench framework (attribute macros, parameterization,
low overhead, CI-friendly sample scaling).
- **Cost:** free / OSS.
- **Fit:** container. **Effort:** low — nicer API than criterion; CodSpeed supports it.
- **Verdict: adopt-now** as the *authoring* framework for new benches (pairs with
  CodSpeed or stands alone). Criterion is fine but in low maintenance; divan is the
  current SOTA for ergonomics.

### criterion — `trial / keep-if-present`
One-liner: the long-standing statistics-driven wall-clock bench lib with HTML reports.
- **Cost:** free / OSS. **Fit:** container.
- **Verdict: trial.** Great local exploratory tool + rich reports, but **do not use its
  wall-clock output as the CI gate on the shared Mac** — it'll false-positive on noise.
  Note: effectively unmaintained for ~4 years. Prefer divan for new work.

### bencher.dev — `trial`
One-liner: OSS continuous-benchmarking *tracker* — ingests criterion/iai/divan output,
**Change-Point-Detection** to cut false positives, dashboards, PR gate; self-hostable.
- **Cost:** **free for public projects**; self-host OSS (Docker/K8s) = free; managed
  "Bare Metal" CBaaS is paid **(verify price)**.
- **Fit:** container benches. **Effort:** medium (run the server, or use hosted).
- **Verdict: trial.** Strong if you want a self-hosted dashboard/history without a
  vendor; overlaps heavily with CodSpeed. If you adopt CodSpeed, skip. If you adopt
  iai-callgrind self-hosted and want history/dashboards, Bencher is the OSS companion.

### hyperfine — `adopt-now` (cheap, for CLI/end-to-end)
One-liner: statistical CLI benchmark tool (warmups, outlier detection) for whole-binary
/ command timings.
- **Cost:** free / OSS. **Fit:** container/CLI (e.g. timing a full `clw` op, a cold vs
  warm cache fetch). **Effort:** trivial.
- **Verdict: adopt-now** for ad-hoc macro timing; not a CI gate (wall-clock).

---

## 2. Profiling / flamegraphs / continuous profiling

### samply — `adopt-now` (best local profiler for the Mac)
One-liner: sampling profiler (Rust-written), opens results in the Firefox Profiler UI;
**best-in-class macOS support**, also Linux/Windows.
- **Cost:** free / OSS. **Fit:** Rust container binary, and **runs natively on the
  founder's Mac** (unlike perf). **Effort:** trivial (`samply record ./target/...`).
- **Verdict: adopt-now.** Default local CPU profiler for CoreLink. Interactive call
  tree / stack chart / inverted stacks beats static SVGs for investigation.

### cargo-flamegraph — `trial`
One-liner: one-command flamegraph SVGs via perf (Linux) / DTrace (macOS).
- **Cost:** free / OSS. **Fit:** container; macOS works via DTrace but needs sudo +
  SIP friction. **Verdict: trial** — keep for shareable SVG artifacts; samply is the
  better daily driver on this Mac.

### tokio-console — `adopt-now` (directly targets a known foot-gun)
One-liner: live debugger for async Rust — per-task busy/idle/scheduled time, **poll
durations, "never-yielded" / long-poll warnings**.
- **Cost:** free / OSS. **Fit:** the tokio container. **Effort:** low-medium (add
  `console-subscriber`, build with `--cfg tokio_unstable`; gate behind a feature so
  prod isn't always instrumented).
- **Measures/gains:** **exactly the `block_in_place` / executor-stall risk CoreLink
  has.** It will flag tasks that monopolize a worker thread (e.g. Argon2id or a big
  hash done on the async runtime instead of `spawn_blocking`) — those stalls tank
  tail-latency on a cache. **Verdict: adopt-now** for async-stall investigation.

### dhat (dhat-rs) — `adopt-now` (memory regression gate; you had container OOM)
One-liner: heap profiler with **assertion-style tests** — "peak heap < 10 MiB", "exactly
N allocations", "all freed before exit" — runnable in CI.
- **Cost:** free / OSS. **Fit:** container. **Effort:** low (feature-gated test).
- **Verdict: adopt-now.** Given the container OOM concerns, a dhat peak-heap assertion
  on the blob-streaming path is a cheap, deterministic memory regression gate (pairs
  with the repo's memory-plumbing skill/`memgate`). Deterministic → shared-Mac-safe.

### pprof-rs — `trial`
One-liner: in-process sampling CPU profiler exposing pprof/flamegraph from a running
binary (handy as an HTTP `/debug/pprof`-style endpoint).
- **Cost:** free / OSS. **Fit:** container (can expose on an internal port for live prod
  profiling). **Verdict: trial** — nice for on-demand prod profiling without redeploy;
  samply covers local.

### Parca / Polar Signals (eBPF continuous profiling) — `trial` (deferred until multi-node)
One-liner: always-on, fleet-wide CPU/alloc profiling via eBPF, <1% overhead, zero code
changes; Parca = OSS self-host, Polar Signals = managed.
- **Cost:** Parca OSS free (self-host); Polar Signals Cloud = 14-day trial + usage-based
  **(verify $)**.
- **Fit:** Linux containers/VMs with eBPF. **Does NOT run on Cloudflare Workers**, and
  CoreLink's Cloudflare Containers may not give you the host-level eBPF access these
  need **(verify whether CF Containers permit the eBPF agent)**. **Effort:** high.
- **Verdict: trial / defer.** Overkill for a single solo-tenant container today. Becomes
  compelling **when the CI/build-acceleration expansion (ephemeral Hetzner-class
  runners) lands** — that's a real fleet where continuous profiling earns its keep.

### Grafana Pyroscope — `trial` (continuous profiling, Grafana-stack-native)
One-liner: OSS continuous-profiling DB; integrates with the Grafana LGTM stack; v2
(2026) made it scale better.
- **Cost:** OSS self-host free; Grafana Cloud free tier includes **50 GB profiles/mo**.
- **Fit:** container (push via SDK or eBPF). **Verdict: trial** — adopt only if you go
  Grafana Cloud for observability anyway (see §5); then profiles ride the same free
  tier. Same "defer until fleet" logic as Parca.

### valgrind/callgrind, heaptrack — `keep as ad-hoc`
Already implied by iai-callgrind (callgrind). heaptrack = deep Linux heap analysis for a
specific OOM hunt. **Verdict:** ad-hoc tools, not standing infra.

---

## 3. Load / stress / soak testing of the cache HTTP APIs

This is GAP #2. Need to simulate: docker pull (many parallel GET blobs + manifest),
Bazel `FindMissingBlobs` (batch existence checks), Turbo PUT bursts, sccache WebDAV,
and **multi-tenant concurrency** with distinct PATs.

### k6 — `adopt-now` (primary; scenario realism)
One-liner: developer-centric load tester, **JS/TS scripts**, scenarios/stages,
thresholds-as-pass/fail, HTTP/1.1+2+gRPC+WS.
- **Cost:** OSS free (self-run); k6 Cloud paid for distributed/SaaS **(verify $; skip
  unless you need huge distributed load)**. v1.0 shipped May 2025 with first-class TS.
- **Fit:** hits the **public HTTP cache surfaces** (it's a client — provider-agnostic;
  works against the Cloudflare edge end-to-end). **Effort:** medium — but scriptability
  is exactly what's needed to model multi-tenant + per-surface request mixes and to
  encode **`thresholds`** (e.g. p95 < X ms, error rate < 1%) as CI pass/fail.
- **Verdict: adopt-now.** Best fit for CoreLink's heterogeneous, stateful cache
  scenarios (auth headers, batch REAPI calls, blob upload/download mixes). Run from a
  cheap cloud box, not the Mac.

### oha — `adopt-now` (quick Rust-native smoke/throughput)
One-liner: tiny Rust+tokio load generator with a live TUI; dead-simple high-RPS
single-endpoint benchmarking.
- **Cost:** free / OSS. **Fit:** client → any URL. **Effort:** trivial.
- **Verdict: adopt-now** as the quick "how many blob-GETs/sec can the container take"
  smoke tool. Pair: **oha for raw single-endpoint throughput, k6 for scripted
  multi-tenant scenarios** (a widely recommended combo).

### vegeta — `trial`
One-liner: constant-**rate** HTTP attacker (open-model load → measures latency under a
fixed RPS, not a fixed concurrency).
- **Cost:** free / OSS. **Verdict: trial.** Its constant-rate (open-model) attack is the
  *correct* model for "what happens at 5k req/s sustained" capacity tests and is great
  for CI latency-SLO regression. Use if you want open-model load that k6 scenarios can
  also do — minor overlap; vegeta is simpler for pure rate tests.

### wrk / drill / Locust / Gatling — `skip` (for now)
- **wrk:** max-RPS C tool, Lua scripting — oha covers this niche with nicer output.
- **Locust (Python) / Gatling (Scala/JVM):** powerful but heavier ecosystems; k6 covers
  the scripted-scenario need with less weight. **Verdict: skip** unless a team
  preference emerges.

**Recommendation:** **k6 (scenarios) + oha (smoke) + optional vegeta (rate/SLO).** Wire
a nightly k6 run against staging with thresholds as the soak/regression signal; keep it
**off the shared Mac** (run on a $5 cloud box or GH-hosted runner, not self-hosted).

---

## 4. Cloudflare-edge performance (the Worker + DO layer)

Native Cloudflare tooling is the cheapest, best-integrated path for the edge half —
3rd-party APMs mostly can't *run inside* Workers, they only *receive exported telemetry*.

### Workers Observability (Logs + Metrics + Query Builder) — `adopt-now`
One-liner: built-in logs/metrics/queries in the CF dash; **now reports per-invocation
CPU time AND wall time**.
- **Cost:** included in Free + Paid Workers plans; **Workers Logs billing began
  2025-04-21** (so it's metered — watch volume) **(verify current included quota)**.
- **Fit:** Workers/DO native. **Effort:** trivial (enable `observability` in
  `wrangler.toml`). **Verdict: adopt-now** — turn it on; the CPU-vs-wall split is the
  cheapest edge perf signal you'll get.

### Workers Analytics Engine — `adopt-now` (the COGS/perf attribution backbone — see §6)
One-liner: write high-cardinality time-series datapoints from a Worker/Tail Worker,
query with SQL.
- **Cost:** generous free tier, cheap beyond. **Fit:** Workers native. **Effort:** low
  (call `writeDataPoint` with tenant_id, surface, bytes, R2-op-class as
  dimensions/blobs). **Verdict: adopt-now** — this is the lever for per-tenant
  per-request attribution at the edge.

### Tail Workers — `adopt-now`
One-liner: a Worker that receives every producer Worker's execution record (status,
logs, exceptions, **CPU/wall time**) in real time — format/route it.
- **Cost:** Workers pricing. **Fit:** Workers native. **Effort:** low-medium.
- **Verdict: adopt-now** — use a Tail Worker to (a) aggregate into Analytics Engine for
  COGS, and (b) export OTLP to an external backend (§5) without inline overhead.

### Smart Placement — `trial`
One-liner: CF auto-relocates a Worker's execution closer to back-end (D1/container) to
cut round-trips.
- **Cost:** free toggle. **Fit:** Workers. **Verdict: trial** — measure with/without;
  can cut latency when the Worker makes several sequential DO/D1/container hops (likely
  true for the auth+accounting path). Cheap experiment.

### Workers Logpush — `trial`
One-liner: push Worker trace-event logs to an external sink (R2/S3/HTTP).
- **Cost:** Logpush is an Enterprise/paid feature historically **(verify availability on
  your plan)**. **Verdict: trial** — only if you need raw logs in an external lake;
  otherwise Tail Worker → Analytics Engine is cheaper.

### R2 / DO performance characteristics — `know-the-cost` (not a tool, a constraint)
- **R2 pricing (Standard, 2025):** storage **$0.015/GB-mo**; **Class A $4.50/M ops**
  (PUT/List/multipart); **Class B $0.36/M ops** (GET/Head); **egress = $0** (the whole
  R2 thesis). Free tier: 10 GB, 1M Class-A, 10M Class-B/mo. **(verify.)**
- **CoreLink implication:** because egress is free, **the COGS knob is OP COUNT, not
  bytes.** A cache pattern that does many small `HeadObject`/`GetObject` (Class B) or
  worse many `PutObject`/multipart (Class A — 12.5× more expensive) per logical request
  is what erodes margin. Bazel `FindMissingBlobs` and docker manifest checks are
  Head/Get-heavy → cheap; small-blob PUT storms (Turbo, sccache) are the expensive
  pattern. **Load tests in §3 must report R2 op-class counts, not just latency.**

---

## 5. Observability / APM / tracing (free-first) — for the Rust container + OTLP export

Cloudflare Workers now support **exporting OpenTelemetry traces/logs** to external
backends (Honeycomb, Grafana Cloud, Axiom, Sentry, Dash0). So the play is:
**OTel everywhere → one backend.** Note: **CF tracing becomes billed as part of Workers
usage starting 2026-03-01** (spans share the logs quota) — **(verify).**

### OpenTelemetry (instrumentation standard) — `adopt-now`
One-liner: vendor-neutral traces/metrics/logs; `tracing` + `tracing-opentelemetry` in
the Rust container, OTel exporter in the Worker.
- **Cost:** free. **Fit:** both (Rust container fully; Workers via the CF OTel export).
- **Verdict: adopt-now** — instrument once, stay portable across backends. Non-negotiable
  foundation.

### Grafana Cloud (LGTM: Loki/Tempo/Mimir/Pyroscope) — `adopt-now` (best free tier)
One-liner: hosted logs+traces+metrics+profiles, generous perma-free tier.
- **Cost:** **Free forever:** 10k metric series, **50 GB logs**, **50 GB traces**, 50 GB
  profiles, 3 users. Overage e.g. logs/traces **$0.50/GB**, metrics ~$8/1k series.
  **(verify 2026 numbers.)**
- **Fit:** receives OTLP from both the container and (via export) Workers; can also host
  Pyroscope profiles from §2. **Effort:** medium. **Verdict: adopt-now** as the primary
  free backend — one stack for traces+logs+metrics(+profiles later), and it's where you
  build the COGS/perf dashboards.

### Honeycomb (free tier) — `trial` (best trace-debugging UX)
One-liner: event/trace analytics with BubbleUp (auto-correlate a latency spike to the
dimension causing it — e.g. tenant_id, surface, blob-size).
- **Cost:** **Free forever: 20M events/mo**, tracing, BubbleUp, 2 triggers. **(verify.)**
- **Fit:** receives OTLP export from Workers + container. **Verdict: trial** — BubbleUp
  is genuinely great for "which tenant/endpoint is slow," and CoreLink is exactly a
  high-cardinality multi-tenant workload. 20M events/mo is plenty solo. Could even be
  the primary if you prefer it to Grafana for tracing.

### Baselime — `trial` (Cloudflare-native, but post-acquisition — verify status)
One-liner: serverless observability, **acquired by Cloudflare** and migrated onto the CF
dev platform; auto-captures Worker logs via Logpush + OTel tracing.
- **Cost:** **(verify — pricing/availability post-acquisition is in flux; some of it is
  folding into native Workers Observability).** **Fit:** Workers-native by design.
- **Verdict: trial / verify.** Most CF-native option, but the acquisition means its
  future is "becoming Workers Observability" — don't build on it until the path is
  clear; lean on native Workers Observability (§4) instead.

### Axiom — `trial`
One-liner: cheap high-volume log/event store with an official CF Workers OTel guide.
- **Cost:** free tier exists **(verify GB/mo)**; cheap at scale. **Fit:** OTLP from
  Workers + container. **Verdict: trial** — strong if log/event *volume* gets large and
  Grafana/Honeycomb free tiers pinch; otherwise redundant.

### Dash0 — `skip-for-now`
One-liner: OTel-native APM with a clean CF Workers OTLP integration.
- **Cost:** **14-day trial**, then paid **(no perma-free → verify)**. **Verdict: skip**
  for a cost-sensitive solo founder unless free tier appears; Grafana/Honeycomb dominate
  on free.

### Sentry (Performance/Tracing) — `trial`
One-liner: errors + perf tracing; has a Workers SDK + OTel ingest; generous-ish free dev
tier. **Verdict: trial** — adopt if you also want error monitoring (likely yes); its
perf tracing then comes "for free" alongside.

**Recommendation:** **OTel instrumentation → Grafana Cloud free tier as primary**, with
**Honeycomb free tier trialed for trace-debugging UX**. Keep edge telemetry in native
**Workers Observability + Analytics Engine** and *export* via Tail Worker only what you
want long-term.

---

## 6. COGS / cost attribution (margin protection) — GAP #3

No off-the-shelf tool attributes *R2-op + CPU-ms + D1-rows* to *tenant/request* — this is
a **build, not buy**, but the building blocks are cheap and CF-native:

**Recommended architecture (adopt-now, low effort, free-tier):**
1. **At the edge:** the Worker / a **Tail Worker** calls **Analytics Engine
   `writeDataPoint`** per request with dimensions: `tenant_id`, `cache_surface`
   (cas/reapi/turbo/sccache/oci), `op` (get/put/head/findmissing), and metrics:
   `r2_class_a_ops`, `r2_class_b_ops`, `bytes`, `cpu_ms`, `wall_ms`. (CPU/wall now
   exposed per invocation.)
2. **Cost model:** a small query/cron turns op-class counts into $ using the known R2
   rates ($4.50/M Class-A, $0.36/M Class-B, $0.015/GB-mo storage, $0 egress) + Workers
   CPU pricing + D1 row-read/write pricing → **$ per tenant per surface**.
3. **Surface it:** Grafana dashboard (Analytics Engine is SQL-queryable; or pull into
   the same Grafana stack) → a **margin panel per tenant** and an alert when any tenant's
   COGS approaches its plan price.
- **Verdict: adopt-now (build it).** This directly defends the 80% margin thesis and is
  the thing most uniquely valuable to CoreLink that no vendor sells. The instrumentation
  is ~a day of work given Analytics Engine.
- **General cloud-cost tools (Vantage, CloudZero, etc.):** **skip** — they don't do
  per-request/per-tenant attribution for R2 op-classes; they're account-level FinOps.

---

## TOP-7 adopt-now shortlist (free-first)

| # | Tool | Cost | What it gets CoreLink |
|---|------|------|----------------------|
| 1 | **iai-callgrind** | free OSS | Deterministic instruction-count **regression gate** — the only kind that survives the noisy shared Mac (runs in Linux CI, not on the Mac). |
| 2 | **k6** (+ **oha** smoke) | free OSS | Load-test the cache hot paths — scripted multi-tenant docker-pull / REAPI / Turbo bursts; thresholds as CI pass/fail. |
| 3 | **tokio-console** | free OSS | Catch `block_in_place`/executor stalls that wreck tail-latency on the async container. |
| 4 | **samply** | free OSS | Best local CPU profiler/flamegraph with native macOS support. |
| 5 | **dhat (dhat-rs)** | free OSS | Deterministic **peak-heap assertion gate** — addresses the container OOM risk, shared-Mac-safe. |
| 6 | **OTel → Grafana Cloud free tier** | free (50 GB logs+traces+50 GB profiles, 3 users) | One backend for container traces/metrics/logs(+profiles later) and the COGS dashboards. |
| 7 | **Workers Observability + Analytics Engine + Tail Worker** | free-tier native | Edge CPU-vs-wall perf signal **and** the substrate for per-tenant per-request COGS attribution. |

Honorable mention / build-it: **the COGS attribution pipeline (§6)** — not a tool, but
the single highest-margin-leverage thing to build, and it rides on #7 for free.

---

## Worth-paying-for shortlist (real $, only if "fucking awesome")

| Tool | Price | Why it can be worth it |
|------|-------|------------------------|
| **CodSpeed** | **Free** for solo (≤5 users, private repo; unlimited if OSS); **$15/user/mo** Pro for unlimited history | Managed instruction-count (CPU-sim) perf gate with PR comments + history — "iai-callgrind as a service," removes Valgrind/CI plumbing. Likely **$0** for CoreLink today. Best DX of any option. |
| **Honeycomb** | **Free** 20M events/mo; paid only past that | Not really "paid" yet at solo scale, but worth flagging: BubbleUp trace-debugging is best-in-class for high-cardinality multi-tenant latency hunts. Pay later if event volume grows. |
| **Polar Signals Cloud / Grafana Pyroscope (paid)** | usage-based **(verify)** | Defer until the **CI/build-acceleration fleet** (Hetzner-class ephemeral runners) exists — continuous eBPF profiling earns its keep on a fleet, not one container. |

**Skip (enterprise-opaque or no perma-free at solo scale):** Dash0 (trial-only), k6
Cloud (unless distributed load needed), Logpush (plan-gated), account-level FinOps tools
(no per-request R2 attribution).

---

## The single best "regression gate that survives the shared-Mac noise"

**iai-callgrind** (or its managed equivalent **CodSpeed CPU-sim instrument**). Both gate
on **CPU instructions / cache-misses, not wall-clock**, so the founder's overloaded Mac
— and CI bursts on it — **cannot** produce false regressions. iai-callgrind additionally
runs under Linux Valgrind in a CI container, so it doesn't even execute on the Mac.
Pick **iai-callgrind** for zero-vendor/zero-cost; **CodSpeed** if you want the managed PR
dashboard (free for a solo private repo). Pair either with **dhat** for a deterministic
memory-regression gate on the same principle.

---

### Sources (2025-2026)
- CodSpeed — codspeed.io, /pricing, /docs/instruments/cpu, github.com/CodSpeedHQ
- iai-callgrind — github.com/iai-callgrind, crates.io/crates/iai-callgrind, bencher.dev
- divan — github.com/nvzqz/divan, nikolaivazquez.com, CodSpeed Divan changelog (2025-02)
- bencher.dev — bencher.dev, /pricing, github.com/bencherdev/bencher
- samply / cargo-flamegraph — github.com/flamegraph-rs/flamegraph, oneuptime/markaicode profiling guides (2026)
- tokio-console — github.com/tokio-rs/console, docs.rs/tokio-console
- dhat-rs — github.com/nnethercote/dhat-rs, docs.rs/dhat
- Parca / Polar Signals / Pyroscope — polarsignals.com, github.com/parca-dev/parca, grafana.com/oss/pyroscope, InfoQ Pyroscope 2.0 (2026-05), uptrace continuous-profiling roundup
- k6 / oha / vegeta — k6 v1.0 (2025-05), hotosm load-testing decision, codenote k6-alternatives, github.com/hatoo/oha, github.com/tsenart/vegeta
- Cloudflare Workers Observability / Tail Workers / Analytics Engine / Logpush / Smart Placement — developers.cloudflare.com/workers/observability/*, blog.cloudflare.com (Workers Observability GA)
- Cloudflare R2 pricing — developers.cloudflare.com/r2/pricing, r2-calculator.cloudflare.com
- Grafana Cloud / Honeycomb free tiers — grafana.com/pricing, honeycomb.io/pricing, monitoringcost.com, cloudzero
- Baselime / Dash0 / Axiom / Sentry on Workers — developers.cloudflare.com/workers/observability/exporting-opentelemetry-data/*, blog.cloudflare.com (Baselime acquisition), dash0.com, axiom.co/docs
