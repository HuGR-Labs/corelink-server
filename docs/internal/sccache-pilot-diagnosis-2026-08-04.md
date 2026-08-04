# Why the sccache pilot lost — the diagnosis behind the 270 ms

**Date:** 2026-08-04 · **Status:** diagnosis complete; the fix is PR #1022, unmerged and undeployed.

## The claim being retracted

The 2026-08-03 sccache pilot on `corelink-reapi::pr-gate` produced four runs and one
conclusion. The conclusion, written into `.github/workflows/corelink-reapi.yml`, was:

> "This is NOT a malfunction … The cost IS the round-trips."

That attribution is **wrong**. The measurements are sound and the rollout verdict
(pilot stays OFF) is unchanged, but the cause is not the network. It is ~248 seconds of
Argon2id burned inside a half-vCPU container, on a code path that recomputes the same
key-derivation for every one of the 827 cache reads.

A cache hit that costs more than the work it replaces is a broken configuration, not a
verdict on caching. It was.

## The data

| run | sha | sccache | hit rate | lane |
|---|---|---|---|---|
| 30857028233 | `d88b1af4` | absent | — | 409 s |
| 30857956568 | `81919a49` | absent | — | 423 s |
| 30858811500 (attempt 1) | `1f832a41` | on | 22.85 % (189/638) | 917 s |
| 30860149618 | `a4e864e0` | on | **100.00 % (827/0)** | **631 s** |
| 30861037937 | `69d6811b` | OFF (gated) | — | 307 s |

All five on `corelink` fabric boxes (4 vCPU / 12.5 GB), runner group `Default`.

Per-step, the entire regression sits in the three compile-bearing steps and nowhere else
(±1 s step-timestamp granularity):

| step | 409 s base | 423 s base | 631 s (100 %) | Δ vs base mean |
|---|---|---|---|---|
| cargo clippy | 64 | 72 | 145 | **+77** |
| cargo test (debug) | 118 | 110 | 189 | **+75** |
| cargo test (release) | 191 | 206 | 259 | **+60** |
| every other step | — | — | — | ≤ ±1 each |
| **total** | **409** | **423** | **631** | **+215** |

And sccache's own accounting on the 631 s run (`sccache --show-stats`, client 0.16.0):

```
Cache hits                           827
Cache misses                           0
Compilations                           0
Cache errors                           0
Average cache read hit             1.702 s
```

**`Average cache read hit 1.702 s` is the number this whole investigation was looking
for.** It was in the log the entire time. The 270 ms figure derived by subtraction
(222 s ÷ 827) is not the per-object latency — it is the per-object latency *after*
overlap, divided across a lane that no longer compiles anything.

## Hypothesis ledger

### H1 — no local layer: **CONFIRMED** (and unfixed, including for customers)

`sccache/src/cache/cache.rs::storage_from_config` picks exactly one storage:

```rust
if let Some(multilevel) = MultiLevelStorage::from_config(config, pool)? {
    return Ok(Arc::new(multilevel));
}
if let Some(cache_type) = &config.cache {
    return build_single_cache(cache_type, &config.basedirs, pool);   // ← WebDAV lands here
}
// No remote cache configured - use disk cache only
```

With `SCCACHE_WEBDAV_ENDPOINT` set and `SCCACHE_MULTILEVEL_CHAIN` unset, the disk cache
branch is unreachable. The pilot config is remote-only: **every hit is an HTTPS round
trip, with no local disk layer behind it.** `SCCACHE_DIR` is inert in that configuration.

The fix exists upstream: `SCCACHE_MULTILEVEL_CHAIN="disk,webdav"` builds an L0 disk / L1
WebDAV chain with automatic async backfill of L0 from L1 on a hit
(`src/cache/multilevel.rs`, landed `d11e2e08` 2026-04-17, released in **v0.15.0**; the
pilot ran 0.16.0, so it was available and unused).

Two consequences, neither of which PR #1022 touches:

1. On an ephemeral box the L0 only helps *within* one job — but this lane runs eight
   cargo invocations in one job, so it is not nothing.
2. **`apps/docs/docs/integrations/sccache-cargo.md` gives customers the same remote-only
   recipe** (plus three i18n copies). Every CoreLink sccache customer is paying a round
   trip for artifacts they already have on local disk. Deliberately left unchanged here —
   changing customer-facing configuration guidance should follow its own measurement, not
   ride on this diagnosis. Named as a follow-up.

### H2 — serial fetches: **REFUTED**

The arithmetic that made this hypothesis attractive (827 × 270 ms ≈ 222 s) is a
coincidence of two independent factors cancelling.

- If the 827 reads were serial at their measured 1.702 s each, they would have added
  **1408 s** of wall time. The lane took 631 s total. Serial is arithmetically impossible.
- Actual overlap: 1408 s of read time inside the 593 s the three compile-bearing steps
  took ⇒ **~2.4 concurrent reads on average**.
- That is bounded, not by opendal, but by CoreLink:
  `ARGON2_PER_TENANT_PERMITS = max(2, ARGON2_VERIFY_PERMITS / 4) = 4`
  (`crates/corelink-container/src/adapter_pat.rs`). One tenant gets at most 4 concurrent
  Argon2id verifies on the container. Measured 2.4 sits under that ceiling, as it must.
- There is no concurrency knob in sccache's WebDAV backend to find
  (`src/cache/webdav.rs` builds a bare opendal `Operator` with a user-agent layer and a
  logging layer; no retry layer, no pool tuning). Client concurrency is simply cargo's
  `-j`.

Reconstruction of the +222 s without appealing to the network: the three compile steps did
**zero compilation** (`Compilations 0`) and took 593 s of cache reads, against 373 s of
actual compilation in the 409 s baseline. 593 − 373 = 220 s. Every other step moved by
≤1 s.

### H3 — routing / latency: **REFUTED as the cause, and now measured.** The cost is CPU, not distance.

#### The measurement (run 30868784748, box `cf-runner-3a0c4d34`, 4 vCPU, colo **BOS**)

Every request below is an authenticated `GET /cargo/<tenant>/<random-64-hex>` returning
**404** — a real miss, no blob fetched, nothing written. A cache **hit** costs strictly more.

| | p50 | p90 | max |
|---|---|---|---|
| `/health` TTFB, new conn (Worker-only floor) | **25 ms** | 29 ms | 29 ms |
| — of which TCP connect | 3 ms | 3 ms | 3 ms |
| — of which TLS handshake | 18 ms | 20 ms | 21 ms |
| `/cargo` TTFB, **new** connection | **514 ms** | 598 ms | 3922 ms |
| — server time (TTFB − TLS done) | **489 ms** | 575 ms | 3902 ms |
| `/cargo` TTFB, **reused** connection (sccache-shaped) | **501 ms** | 740 ms | 2191 ms |

Three things fall out, and they close H3:

1. **The network is 3 ms.** TCP connect to the edge is 3 ms and a full Worker round trip is
   25 ms. The fabric box is not badly placed; there is no region to move it to that would
   recover meaningful time.
2. **Connection reuse buys 13 ms** (514 → 501 ms). If TLS handshakes or connection churn
   were the cost, this is where it would show. It does not. opendal already pools
   connections, so this is the case sccache actually runs in.
3. **~489 ms of server time on a request that fetches nothing.** That is ~20× the Worker
   floor, on a path whose only remaining work is: container dispatch, PAT verify
   (HMAC + D1 row read + **Argon2id**), and a url-map lookup that returns "absent". The
   1.702 s a real hit costs is this number plus the R2 fetch plus 2.4× self-contention.

#### The concurrency sweep is where the ceiling becomes visible

`PatVerifier` **sheds** load rather than queueing it: a request that cannot take an Argon2id
permit within `ARGON2_PERMIT_WAIT` (250 ms) is rejected. A rejected request is *fast*, so
raw req/s **rises** as the server refuses more traffic — which is why the probe counts 404s
(served) separately and reports a served throughput. First run (status codes not yet
captured — the script was amended for exactly this reason):

| P | reqs | wall | req/s | eff. ms/req |
|---|---|---|---|---|
| 1 | 4 | 2.04 s | 1.96 | 511 |
| 2 | 8 | 3.73 s | 2.14 | 466 |
| 4 | 16 | 6.50 s | 2.46 | 406 |
| 8 | 32 | 4.80 s | 6.66 | 150 |
| 16 | 64 | 4.45 s | 14.38 | 70 |

**From P=1 to P=4 — the range inside `ARGON2_PER_TENANT_PERMITS = 4` — throughput is flat
at ~2.0–2.5 req/s.** Four-way client concurrency buys 25 %. That is a hard server-side CPU
ceiling and it is the predicted one: 0.5 vCPU against a per-verify cost of ~0.2–0.25 CPU-s
(the ~0.15 CPU-s figure is an explicit lower bound, so measuring slightly *below* 3.3 req/s
is the expected direction).

The jump at P=8/16 is above the permit cap and is therefore not a scaling result — see
"found, not fixed" #6.

#### Why 490 ms — the mechanism

The container is provisioned `instance_type = "standard-1"` in `[[env.prod.containers]]`
— **1/2 vCPU**, 4 GiB ([Cloudflare Containers limits](https://developers.cloudflare.com/containers/platform-details/limits/)).

Every authenticated request to the cache-adapter surfaces runs a full OWASP-2024 Argon2id
(m = 64 MiB, t = 3, p = 4) against the stored PAT hash. The Worker pins one tenant to one
Durable Object and therefore to **one** container, so a single tenant's whole adapter plane
is CPU-bound at:

```
0.5 vCPU ÷ ~0.15 CPU-s per Argon2id ≈ 3.3 verifies/second
827 cache reads ÷ 3.3 /s          ≈ 248 s of unavoidable serialised container CPU
```

Measured overhead: **215–222 s**. The ~0.15 CPU-s figure is a lower bound (C reference
implementation on Apple silicon), so the prediction is a sufficient explanation of the
whole regression with nothing left over for the network to explain.

It also explains the shape, not just the size. Four concurrent 64-MiB memory-hard verifies
sharing half a core are each ~4× slower than one — which is why the *measured* per-read
latency is 1.702 s rather than ~0.4 s, and why it did **not** improve as the cache warmed
(1.663 s at 22.85 % hits → 1.702 s at 100 %; a network- or warm-up-bound path would have).

Corroborated end to end: predicted single-request server time ~0.2–0.25 CPU-s at 0.5 vCPU
⇒ ~400–500 ms; **measured 489 ms at P=1**. Predicted saturated throughput ~3.3 req/s (lower
bound); **measured 2.0–2.5 req/s across P=1…4**. Predicted total for the pilot 248 s;
**measured 215–222 s**.

#### Why hand-probing this path misleads

The trap this path sets for anyone probing it by hand: an unauthenticated or malformed
bearer `GET /cargo/<tenant>/<key>` returns **401 in ~30 ms**, short-circuiting *before*
Argon2id. Measured from a laptop against prod on 2026-08-04:

```
noauth  code=401  connect=0.009  appconnect=0.025  ttfb=0.080
badpat  code=401  connect=0.009  appconnect=0.025  ttfb=0.047
```

Those numbers measure the rejection, not the cache. A prior "~20 ms of server time, the
network was never implicated" claim was made this way and has been retracted.
`.github/workflows/cargo-cache-latency-probe.yml` + `scripts/probe-cargo-cache-latency.sh`
exist so the authenticated path can be measured instead of inferred: they use the real
PAT, assert a **404** (an authenticated miss on a random key — a lower bound on a hit, and
never a write), and **exit non-zero on 401/403** so a run that measured nothing cannot look
like a run that measured something.

### H4 — the `CARGO_INCREMENTAL=0` confound: **REFUTED as an explanation**

The concern is legitimate in principle — the comparison did move two variables — but it
cannot account for any of the 631 s run, because `Compilations 0`: sccache performed
**zero** local compilations, so there was no compilation for an incremental setting to make
faster or slower. `CARGO_INCREMENTAL` has no effect on a build that does not compile.

(It is also close to free on this lane by construction: the box is ephemeral with a cold
`target/`, incremental buys nothing on a first compile, and no source changes between the
eight cargo invocations in the job.)

### H5 — "the lane was the wrong instrument": **mostly REFUTED**

The lane is more compile-dominated than the hypothesis assumed. In the 409 s baseline,
clippy + debug tests + release tests = 373 s of 409 s (**91 %**). The five trailing
`cargo test --test <name>` invocations cost **0–1 s each, ~4 s combined** — the warm-
`target/` effect is real, but it makes those steps *negligible*, not misleading. Test
execution is not hiding a large cost: the whole suite, including the 100k-concurrent
isolation property test, runs in ~1 s once built.

`pr-gate` is therefore a fair instrument. What was wrong was not the lane; it was the
attribution of what the lane measured.

## What a correct experiment looks like

The discriminating experiment is now cheap, and it is not a lane re-run.

**Step 1 — measure the surface (≈1 minute, no compilation).**
Dispatch `cargo-cache-latency-probe.yml` **before** PR #1022 deploys and again **after**.

> A trap worth recording, because it nearly shipped an unproven instrument: GitHub will
> only dispatch a `workflow_dispatch` workflow that already exists **on the default
> branch** — `gh workflow run … --ref <feature-branch>` answers
> `HTTP 404: workflow … not found on the default branch`. A dispatch-ONLY probe therefore
> cannot produce a number until after it merges, i.e. it merges unproven. This one carries
> a `pull_request` trigger scoped by `paths:` to its own two files, so the PR that
> introduces or edits it runs it, and an ordinary PR still pays nothing.

The **before** number is already in hand (above): p50 **501–514 ms**, served throughput flat
at **2.0–2.5 req/s** across P=1…4. The prediction to falsify is therefore sharp: after
#1022 deploys, a warm-memo lookup should drop to roughly the Worker floor plus a D1 read
(**tens of ms, not ~490 ms**) and served throughput should climb well past 2.5 req/s. If it
does not, the Argon2id attribution is wrong and this document is wrong with it.

**Step 2 — only then, re-run the pilot**, holding everything fixed:

- Same lane (`corelink-reapi::pr-gate`), same `runs-on: corelink`, same commit.
- `CARGO_INCREMENTAL=0` **in the baseline arm too** — remove the confound rather than
  argue it away.
- **Configure a local layer**: `SCCACHE_MULTILEVEL_CHAIN="disk,webdav"` +
  `SCCACHE_DIR=$RUNNER_TEMP/sccache`. Without it, H1 stands and every hit is still a round
  trip regardless of how fast that round trip becomes.
- **≥5 runs per arm**, not 1. The `69d6811b` control ran the identical lane in 307 s
  against "baselines" of 409 s and 423 s — the same lane on the same box class varies by
  ±35 % on the compile steps. A single-sample 1.5× survives that; a 10–20 % claim would
  not, and the win, if it comes, will be in that range.
- Report `Average cache read hit` from `--show-stats` as the primary metric. It is a
  direct measurement of the thing under test; lane duration is a noisy proxy for it.

**Can sccache win here once configured properly?** Nothing found in this diagnosis says it
cannot. The structural facts are: 833 compile units, ~0.45 s each amortised on 4 local
vCPUs. For a fetch to beat that it must land under ~0.45 s amortised — i.e. under ~1.1 s
per object at the ~2.4× overlap this lane sustains. The measured 1.702 s misses that by
~1.6×, and ~248 s of the cost is Argon2id that PR #1022 removes. That is the arithmetic
being fixed. It has to be re-measured, not predicted — but "the cache cannot win" is not
what the evidence says, and it is not what should be written down.

## Found, not fixed

1. **PR #1022 is open, unmerged, and undeployed.** Until the container image is rebuilt,
   repinned and rolled, *every* customer on `/cargo`, `/npm`, `/pip`, `/brew` and OCI
   `/token` is capped at ~3.3 req/s per tenant. This is a live product-performance defect
   on the cache we sell, not a CI curiosity.
2. **The doc-comment on `ARGON2_PER_TENANT_PERMITS` asserts a behaviour that does not
   exist**: *"each request verifies once, briefly, then the result is reused"*. The result
   was never reused — that is precisely what #1022 adds. A designed-vs-wired gap sitting in
   a rationale that reads as description.
3. **Customer sccache docs recommend the remote-only configuration** (H1), in
   `apps/docs/docs/integrations/sccache-cargo.md` plus `de` / `es-419` / `pt-BR` copies.
4. **The pilot comment in `corelink-reapi.yml` still carries the retracted "the cost IS the
   round-trips" attribution.** PR #1022 amends it; deliberately not touched here to avoid a
   conflicting edit on the same lines.
5. **sccache's WebDAV operator has no opendal `RetryLayer`** (`src/cache/webdav.rs`). Any
   transient 5xx is a hard read error. It cost nothing in this pilot (0 cache errors), but
   it means a brief container blip degrades straight to cold compiles with no retry.
6. **What happens above `ARGON2_PER_TENANT_PERMITS = 4` needs its own look.** The sweep
   showed raw throughput rising to 6.7 and 14.4 req/s at P=8/16 against a served ceiling of
   ~2.5 — arithmetically that can only be load-shedding, since the server cannot serve
   14 req/s when it serves 2.5. The first probe run did not record status codes there, so
   *which* rejection (503 vs something else) is unconfirmed; the script now counts them and
   the answer will be in the next run. It matters because `cargo -jN` with N > 4 is the
   normal case, and #1022's own commit message describes exactly this cold-burst 503 as
   **pre-existing on main** — its single-flight remedy was withdrawn after review and is
   tracked separately. So this is an open defect, not a probe artifact: a customer running
   `cargo -j8` against `/cargo` today has half their requests shed.
