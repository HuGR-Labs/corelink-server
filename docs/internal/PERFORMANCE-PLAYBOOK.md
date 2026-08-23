# CoreLink Performance Playbook

> **Audience:** every engineer touching a hot path
> (CAS read/write, AC read/write, audit emit, BYOK
> encrypt/decrypt, tenant-prefix derivation, auth middleware).
>
> **Companion docs:**
> - `specs/_audits/sealed/2026-05-14-perf-baseline.md` — the criterion bench
>   baseline (the numbers).
> - `specs/_audits/sealed/2026-05-15-perf-optimization-audit.md` — the
>   audit (where we are vs where we should be).
> - `specs/_audits/sealed/perf-optimization-followup-tickets.md` — sprint
>   backlog (what to do next).
>
> **Rule of thumb:** if you are adding code to a path called on
> every CAS or AC request, this playbook applies. If your code runs
> once per hour, none of it matters — go optimize for readability.

---

## How to use this doc

Each pattern below is structured as:

1. **The trap** — a concrete code shape that looks fine but
   inflates p99 latency or per-request allocations.
2. **The fix** — the recommended alternative.
3. **Bench evidence** — a measured (or projected, where measurement
   is pending) delta from the criterion suite.
4. **When the trap is acceptable** — every rule has a context where
   it doesn't apply (cold paths, cleanup-on-shutdown, etc.).

If you cannot fit your change into one of these patterns, write down
*why* in the WI's design notes — the next reviewer will need it.

---

## Pattern A — Avoid per-request heap allocation

### A.1 The trap

```rust
// Anti-pattern: clones the body into a fresh Vec on every request.
async fn handle_put(body: Vec<u8>) -> Response {
    let copy = body.clone();           // alloc #1 (body clone)
    let payload = format!("blob:{}", hex::encode(&copy)); // alloc #2 (String)
    publish(payload).await
}
```

Per-request `Vec<u8>::clone()` and `String` formatting are
quietly responsible for ~10-15% of CPU time under sustained load.
Every clone is a heap alloc + memcpy + drop (= free).

### A.2 The fix

```rust
use bytes::Bytes;

// Body is Bytes — clone is a refcount bump (8 ns), not a heap alloc.
async fn handle_put(body: Bytes) -> Response {
    let payload = format_args!("blob:{}", HexDisplay(&body));
    publish_with_format(payload).await
}
```

Or, more generally:

- Pass body as `&[u8]` or `Bytes` from the entry point down.
- Use `&str` instead of `String` when the lifetime is bounded by
  the request scope.
- Use `format_args!` (lazy) where the formatted result is consumed
  by another `Write` directly — avoids materializing the `String`.

### A.3 Bench evidence

- `Bytes::clone` benchmarked at ~8 ns (refcount bump) vs
  `Vec<u8>::clone` at ~200 ns + heap pressure on a 1 KiB body
  (corroborated by `crates/corelink-hash/benches/blake3.rs` body
  reuse — the `Bytes` path is unmeasurable noise; the `Vec`
  equivalent dominates).
- See `2026-05-15-perf-optimization-audit.md §4 anti-pattern 4`
  for the cleanup wave that converts `r2.rs:208` body-clone to
  `Bytes::clone`.

### A.4 When the trap is acceptable

- One-shot startup paths (we run them once per isolate).
- Test code (clarity wins over performance).
- Cleanup / shutdown handlers (cold path, latency does not matter).

---

## Pattern B — Reuse `blake3::Hasher` state where it's safe

### B.1 The trap

```rust
// Anti-pattern: fresh Hasher::new() per call in a tight loop.
for event in events {
    let mut h = Hasher::new();        // ~70 ns init
    h.update(&prev_hash);
    h.update(&event_canonical_bytes);
    let digest = h.finalize();
    // ...
}
```

Across 10⁶ audit events / sec this is ~70 ms/sec of CPU re-doing
the same SIMD register state setup.

### B.2 The fix

```rust
use blake3::Hasher;

thread_local! {
    static HASHER_TEMPLATE: Hasher = Hasher::new();
}

for event in events {
    HASHER_TEMPLATE.with(|template| {
        let mut h = template.clone(); // ~12 ns refcount-free clone
        h.update(&prev_hash);
        h.update(&event_canonical_bytes);
        let digest = h.finalize();
        // ...
    });
}
```

### B.3 Bench evidence

- `blake3/hash/1B` baseline ~70 ns (setup cost dominated).
- Projected ~50 ns saved per audit append; cumulative on
  `audit_chain/append_10k/sequential` ~250 ms → ~245 ms (-2-4%).
- See `2026-05-15-perf-optimization-audit.md §2 OPT-05`.

### B.4 When the trap is acceptable

- One-shot `Digest::compute(body)` — already uses the optimal
  `blake3::hash(body)` free function. Don't add complexity here.
- Any path where the hasher is used <= 1 time per worker isolate
  lifetime (just `new()` it).

---

## Pattern C — Prefer `parking_lot::RwLock` / `Mutex` over `std::sync` ones

### C.1 The trap

```rust
use std::sync::Mutex;

struct InProcessCache {
    inner: Mutex<HashMap<Key, Value>>,
}

impl InProcessCache {
    fn get(&self, k: &Key) -> Option<Value> {
        let guard = self.inner.lock().unwrap(); // poison panic risk
        guard.get(k).cloned()
    }
}
```

Two problems: (a) `std::sync::Mutex` carries poison-flag bookkeeping
that `parking_lot` skips, costing ~50-100 ns per lock/unlock; (b)
`.unwrap()` on `PoisonError` cascades panics into 5xx responses
on a single in-process panic.

### C.2 The fix

```rust
use parking_lot::RwLock; // for read-heavy workloads
// or parking_lot::Mutex for balanced

struct InProcessCache {
    inner: RwLock<HashMap<Key, Value>>,
}

impl InProcessCache {
    fn get(&self, k: &Key) -> Option<Value> {
        let guard = self.inner.read();   // no Result; no poison
        guard.get(k).cloned()
    }

    fn insert(&self, k: Key, v: Value) {
        let mut guard = self.inner.write();
        guard.insert(k, v);
    }
}
```

Choose `RwLock` over `Mutex` when ≥ 95% of accesses are reads
(typical for caches). Otherwise stick with `Mutex` (cheaper
uncontended fast path than `RwLock`).

### C.3 Bench evidence

- `parking_lot::Mutex` ~20% faster on the uncontended fast path
  than `std::sync::Mutex` (parking_lot upstream benchmarks).
- Under contention (10k rps, ≥ 4 threads): `parking_lot::RwLock`
  with high read ratio scales linearly where `std::sync::Mutex`
  serializes; expected tail-latency reduction 100-500 µs at p99.
- See `2026-05-15-perf-optimization-audit.md §2 OPT-04`.

### C.4 When the trap is acceptable

- Code that explicitly uses `MutexGuard` poisoning as a cross-thread
  error signal. None of CoreLink's hot path does this.
- Library boundaries where the std type is required by API contract
  (e.g. a third-party trait demanding `std::sync::Mutex`).
- `wasm32-unknown-unknown` builds before parking_lot wasm support
  is verified — confirm builds in CI before swapping.

---

## Pattern D — Avoid `serde_json::from_slice` in hot read paths

### D.1 The trap

```rust
// Anti-pattern: full owned-String deserialization on every read.
#[derive(Deserialize)]
struct AcEnvelope {
    digest: String,          // alloc #1
    tenant_id: String,       // alloc #2
    region: String,          // alloc #3
    blob_hash: String,       // alloc #4
    // ...
}

async fn read_ac(d1: &D1, key: &str) -> AcEnvelope {
    let bytes = d1.get(key).await?;
    serde_json::from_slice(&bytes)? // N allocations for N string fields
}
```

Each `String` field is a heap allocation. An envelope with 8 string
fields is 8 allocs on the synchronous critical path.

### D.2 The fix (a) — borrow when lifetime allows

```rust
use std::borrow::Cow;

#[derive(Deserialize)]
struct AcEnvelope<'a> {
    #[serde(borrow)] digest: Cow<'a, str>,
    #[serde(borrow)] tenant_id: Cow<'a, str>,
    #[serde(borrow)] region: Cow<'a, str>,
    #[serde(borrow)] blob_hash: Cow<'a, str>,
}

async fn read_ac<'b>(d1: &D1, key: &str, buf: &'b mut Bytes) -> AcEnvelope<'b> {
    *buf = d1.get(key).await?;
    serde_json::from_slice::<AcEnvelope<'b>>(buf)?
    // No allocations: deserialized fields borrow from `buf`.
}
```

### D.3 The fix (b) — hand-roll a parser for fixed, performance-critical schemas

For really hot envelopes consider a `postcard` / `bincode`
binary format (or a manual byte-pack parser). **Only do this if
profiling demands it** — never premature.

### D.4 Bench evidence

- Projected -20 to -40 µs per AC GET via fix (a) alone (see
  `2026-05-15-perf-optimization-audit.md §2 OPT-03`).
- Fix (b) is currently DEFERRED to post-GA in the followup backlog;
  the schema-migration risk outweighs the marginal win.

### D.5 When the trap is acceptable

- Cold paths (admin ops, startup config load).
- One-shot deserialization at request boundary where the result
  needs to outlive the buffer (use `.into_owned()` once at the
  boundary).
- Public REAPI wire — we use JSON for compatibility regardless.

---

## Pattern E — Batch D1 writes via DO transaction window

### E.1 The trap

```rust
// Anti-pattern: one D1 round-trip per write.
for record in records {
    d1.prepare("INSERT INTO foo VALUES (?, ?)")
      .bind(&[&record.k, &record.v])?
      .run().await?;
}
```

Each `.run()` is one network round-trip. 100 records = 100 RTTs =
~3 seconds in the worst case (D1 is sub-100ms typical but tail is
ugly).

### E.2 The fix — batch within a Durable Object transaction window

```rust
// Open a transaction; all writes commit atomically + in one RTT.
let stmts: Vec<_> = records.iter().map(|r| {
    d1.prepare("INSERT INTO foo VALUES (?, ?)").bind(&[&r.k, &r.v])
}).collect::<Result<_, _>>()?;

d1.batch(stmts).await?;
```

For writes that span multiple tables, coordinate via a Durable
Object's storage API — its `put()` operations are batched into
the same persistence window automatically (DO storage is
transactional within the actor scope; see `specs/03_architecture/
storage_semantics_matrix.md` for the matrix of write guarantees
across R2 / KV / D1 / DO).

### E.3 Bench evidence

- We do not have a direct CoreLink bench for this (D1 is mocked
  in unit tests). Cloudflare's published numbers: a 50-write
  D1 batch completes in 1 RTT (~30-50 ms in-region) vs 50 RTTs
  (~1.5-2.5 s) serially.
- See WI-S04-* (REAPI AC) for the canonical batch-size pattern:
  `MAX_BATCH_SIZE = 250` is the established ceiling (mirrors the
  S-04 TTL evictor; see `crates/corelink-worker/src/reapi/cas/
  sweeper.rs:102`).

### E.4 When the trap is acceptable

- Truly independent writes where one failing should not invalidate
  the others (rare on our hot path).
- Single-write operations (`.run()` directly is correct).
- Writes that span > 1 D1 database (cross-DB transactions don't
  exist; use idempotency + retries instead).

---

## Cross-cutting checklist for a new hot-path change

Before merging a PR that touches CAS / AC / audit / BYOK / auth:

- [ ] No per-request `String::clone()` on shared context fields
      (Pattern A); use `Arc<str>` for principal identifiers.
- [ ] No per-request `Vec<u8>::clone()`; use `Bytes::clone`
      (Pattern A).
- [ ] No `format!()` on success path error mappers (Pattern A);
      use `thiserror` `#[source]`.
- [ ] No `blake3::Hasher::new()` in a tight loop (Pattern B);
      thread-local template + clone.
- [ ] No `std::sync::Mutex` on new code (Pattern C); use
      `parking_lot::Mutex` / `RwLock`.
- [ ] No owned-`String` field on a deserialized struct used in
      the synchronous read path (Pattern D); use `Cow<'a, str>`
      with `#[serde(borrow)]`.
- [ ] No serial D1 writes when batching is correct (Pattern E);
      use `d1.batch(...)` or DO storage transactions.
- [ ] A criterion bench exists for the new hot path
      (`crates/<crate>/benches/<name>.rs`) and is wired into
      `perf-nightly.yml`.
- [ ] Mutation kill-rate on the changed module ≥ 75% (gate
      established by `2026-05-14-mutation-baseline.md`).
- [ ] PR description includes a bench delta vs
      `reports/perf/baseline.json` if the change touches a hot
      path (no regression > 20%).

---

## How the perf-regression signal works (ADVISORY, not a merge gate)

This section documents the **`perf-regression` CI workflow** — the
automated counterpart to the patterns above. The patterns tell you
how to write fast code; this signal reports when a tracked bench
drifted. **As of 2026-08-11 this is explicitly NOT a merge gate** —
see "Why it does not block" below. An earlier revision of this
document described a blocking p99/10%/Linux-CI design; that design
was never what shipped in `.github/workflows/perf-regression.yml`,
and this section is corrected to match the workflow as committed.

### What it does

On every PR that touches a perf-critical crate (or carries the
`perf-sensitive` label), GitHub Actions:

1. Runs the criterion bench suite for the tracked
   `(crate, bench)` pairs (see `reports/perf/README.md` for the list).
2. Reads criterion's `target/criterion/<bench>/new/` output —
   `estimates.json` (median, mean).
3. Compares each current metric against the **committed** baseline
   at `reports/perf/baseline-<crate>-<bench>.json`.
4. Reports (does **not** fail) the PR check if any bench regresses
   beyond its tolerance class: **5% for CRITICAL benches** (tenant-path
   derive, blake3, JCS canonicalize, audit-chain merkle append, JWT
   verify under DPA accept, envelope_roundtrip, orchestrator, tier
   selection), **15% for non-critical benches** — a per-bench
   `tolerance_pct` in the baseline JSON overrides the class default.
   `--threshold-pct` / `PERF_REGRESS_THRESHOLD_PCT` can force a single
   legacy threshold via `workflow_dispatch`.

The criterion HTML report and `perf-regression-report.json` are
uploaded as a workflow artifact (`criterion-report-<run_id>`,
14d retention) for offline triage.

### Why MEDIAN (not p99)

The workflow gates on **MEDIAN**, not p99. Criterion's `estimates.json`
carries the median directly; p99 from raw per-iteration samples proved
too noisy on the runner this lane actually uses (see next section) —
empirically ~22% p99 jitter run-to-run vs <4% for the median on
identical code — so the split 5%/15% thresholds are sized against the
median's noise floor, not p99's.

### Why it does not block (the runner is the shared founder's Mac)

The job runs on `runs-on: [self-hosted, mac, corelink-builder]` — the
shared, multi-tenant `corelink-builder` Mac fleet, the SAME box class
used for the rest of self-hosted CI — **not** dedicated or Linux-hosted
silicon. Back-to-back runs of byte-identical code have measured up to
**~5x** median swing from noisy-neighbour core-steal on that shared
host. Gating merges on that signal would false-positive constantly, so
the check step runs with `continue-on-error: true` and the workflow
explicitly reports itself as **"INFORMATIONAL (non-blocking)"** in its
own header and job-summary output. This is a documented, human-authorized
WAIVER (owner "0 hosted spend", 2026-08-11): only isolated perf silicon
(a dedicated box or a cgroup-pinned lane) would justify restoring a
blocking gate, and the zero-hosted-spend mandate rules out paying for
stable hosted silicon instead.

**Practical consequence: a perf-sensitive PR needs a human to actually
look at the criterion-report artifact.** A single red run is not proof
of a regression (it may be noise); a persistent multi-run trend is the
signal worth acting on. Triage flow: `specs/_runbooks/RB-PERF-REGRESSION.md`.

### The baseline manifest

Baselines live in `reports/perf/` as one JSON file per tracked bench.
They are **committed** — the gate is git-versioned, not stored in
CI cache, so the threshold for any given PR is the baseline at its
merge-base. See `reports/perf/README.md` for the schema.

The initial baselines are **pending** (median_ns=null) on the first
merge of this workflow. The script tolerates pending baselines as
"record one" and does **not** fail on them. The Engineering Lead
records real baselines via:

```bash
scripts/refresh-perf-baseline.sh
git diff reports/perf/    # inspect
git add reports/perf
git commit -m "chore(perf): record initial baselines"
```

Each subsequent baseline refresh follows the same pattern, with the
rationale captured in the commit message (e.g. "refresh after merging
WI-S07-014 BYOK envelope batching — expected −18% p99").

### When the gate fails

Triage flow lives in `specs/_runbooks/RB-PERF-REGRESSION.md`. Summary:

1. **Diagnose:** is the regression real (median moved with p99,
   reproduces locally, touches the bench's hot path)?
2. **Decide:** optimize (preferred), revert, or accept-with-sign-off.
3. **Cross-link:** every regression maps to a DEBT-013 follow-up WI.

Accept-with-sign-off is gated by **two approvals** (Engineering Lead +
Quality Lead) AND a follow-up WI committing to restore the baseline by
a named date. There is no "just bump the baseline" path.

### Cost

The workflow caches `target/criterion` keyed on `Cargo.lock`, which
keeps end-to-end runtime around 8–12 minutes for the full tracked
suite. The cache hit ratio is high for typical PRs (Cargo.lock
unchanged), so most PRs see a fast bench run plus the regression
check.

---

## References

- `specs/_audits/sealed/2026-05-14-perf-baseline.md` — criterion baseline numbers.
- `specs/_audits/sealed/2026-05-15-perf-optimization-audit.md` — audit identifying these patterns.
- `specs/_audits/sealed/perf-optimization-followup-tickets.md` — Sprint-ready optimization WIs.
- `specs/_audits/sealed/2026-05-14-mutation-baseline.md` — kill-rate floor preserved by every optimization.
- `specs/_runbooks/RB-PERF-REGRESSION.md` — triage when the gate fails.
- `specs/03_architecture/slo_catalog.md` — SLO targets.
- `specs/03_architecture/storage_semantics_matrix.md` — storage write-batching guarantees.
- `crates/corelink-worker/src/reapi/cas/sweeper.rs` — canonical `MAX_BATCH_SIZE = 250` pattern.
- `crates/corelink-hash` — the BLAKE3 / VerifiedBody primitives.
- `scripts/perf-regression-check.py` — gate implementation.
- `scripts/refresh-perf-baseline.sh` — operator refresh script.
- `reports/perf/README.md` — baseline manifest schema.
- `.github/workflows/perf-regression.yml` — CI workflow.

---

**Fim de PERFORMANCE-PLAYBOOK (v1.1, 2026-05-15) — regression gate section added.**
