# SOTA Verification / Correctness / Audit Tooling Survey — CoreLink

Date: 2026-06-17
Author: staff correctness/verification engineer (tooling survey)
Scope: everything that *proves* CoreLink is correct — coverage gates, concurrency
verifiers, model-based/stateful testing, deterministic simulation / chaos,
Rust formal verification, protocol conformance suites, contract/snapshot testing,
SBOM/attestation, and cheaper CI to get off the single shared Mac.

All claims grounded via web research (2025-2026). Where a number could not be
nailed down, it is flagged **(verify)**.

---

## System recap (what we are proving)

TS **Worker** → **Durable Objects** → **Rust Containers** (~71 crates) → **R2 + D1 + KV**.
Cache surfaces: native CAS/AC, Bazel **REAPI v2**, **Turborepo v8**, **sccache** (WebDAV),
**OCI registry**. Load-bearing invariants: tenant isolation, byte-accounting/quota
integrity, content-addressing correctness, idempotency, GDPR erasure, money-path.
Constraints: solo founder, cost-sensitive, **CI = one overloaded shared Mac**.

## What we ALREADY have (not re-recommended)

proptest + density gate, cargo-mutants, TLA+, CodeQL, cargo-deny, reproducible-build,
INV-registry×TLA×code×tests cross-gate, clippy -D, validate_specs, DCO, contract-freeze.
**Verified in-repo this pass:** `cargo-llvm-cov` IS wired (`scripts/coverage.sh`,
`.github/workflows/coverage.yml`) and `cargo-nextest` IS configured
(`.config/nextest.toml`) — BUT coverage is **report-only**: no threshold/fail-under,
**no Codecov upload**, runs nightly only. `insta`, `loom`, `shuttle` are **absent**
from every `Cargo.toml` and `src/`. So the real coverage gap is "measured but not
gated", not "unmeasured".

---

## 1. Concurrency verifiers — *the highest-value gap (we just found a concurrent-PUT byte-accounting race)*

### loom — exhaustive concurrency permutation testing — **ADOPT NOW (free)**
- Free, MIT, `tokio-rs/loom`. Deterministically explores **all** thread interleavings
  under the C11 memory model with state-reduction to fight combinatorial blowup.
- Fit: Rust-only, but exactly the tool for the byte-accounting race. You model the
  shared quota counter / AC entry as `loom::sync` types and assert the invariant
  (`bytes_stored == sum(blobs)`) holds under **every** interleaving of two concurrent
  PUTs. A passing loom test is a *proof* over the modeled scope.
- Effort: medium. Needs a `cfg(loom)` shim so prod uses `std::sync` and tests use
  `loom::sync`; keep the modeled critical section small (loom does not scale to whole
  programs).
- Gap closed: **idempotency / concurrent-PUT / quota-accounting races** — the exact
  class of the bug just found.
- **Verdict: adopt-now.** This is the single tool that would have CAUGHT the
  concurrent-PUT byte-accounting race.

### shuttle — randomized concurrency testing (loom's scalable cousin) — **ADOPT NOW (free)**
- Free, `awslabs/shuttle`. loom-inspired but **randomized** (not exhaustive): not a
  soundness proof, but scales to far larger code than loom and reproduces failures
  deterministically from a seed.
- Fit: use loom for the small, critical accounting/idempotency sections (proof) and
  shuttle for larger end-to-end-ish concurrent flows where loom's state space explodes
  (e.g., concurrent PUT + DELETE + erasure on the same key).
- Effort: low-medium (same `cfg`-shim pattern; can share the shim with loom).
- Gap closed: broader concurrency coverage where exhaustive is infeasible.
- **Verdict: adopt-now (pair with loom).**

> Together loom+shuttle close the biggest open correctness gap. Cost: $0.

---

## 2. Coverage gate (we measure but don't gate)

### cargo-llvm-cov `--fail-under-*` + Codecov — **ADOPT NOW (free)**
- Already installed. `cargo-llvm-cov` supports region/line/branch and a fail-under
  threshold; Codecov is **free for OSS** (private-repo free tier is small — **verify**
  current limits before relying on it for a private repo).
- Two cheap upgrades: (a) add a `--fail-under-lines` ratchet to `scripts/coverage.sh`
  so coverage can't regress; (b) emit `--codecov` (region coverage) and upload, OR keep
  it local-only to avoid a SaaS dependency on the private repo — your call given the
  "CI doesn't validate" stance. A **diff-coverage** gate (only new lines must be
  covered) is the highest-signal, lowest-friction form and avoids chasing a global %.
- Effort: low (a few lines in the existing script + workflow).
- Gap closed: coverage *regression* is currently invisible.
- **Verdict: adopt-now** (diff/patch-coverage ratchet; Codecov optional).

---

## 3. Protocol / API conformance suites — *free, external, offload from the Mac, and directly prove surface correctness*

### OCI distribution-spec conformance — **ADOPT NOW (free)** ⭐
- Free, official `opencontainers/distribution-spec/conformance`. Builds a
  `conformance.test` binary; set env vars pointing at your registry, run, get
  `junit.xml` + `report.html` across Pull / Push / Content-Discovery / Content-Management
  workflows. You can **self-certify** by submitting results to `opencontainers/oci-conformance`.
- Fit: directly validates the OCI registry surface (`routes/...` OCI). Black-box —
  runs against a deployed/preview Worker, so it **offloads compute** and doesn't touch
  the Mac's compile budget.
- Effort: low (it's a prebuilt harness; wire env + a CI job hitting a preview URL).
- Gap closed: OCI surface conformance + a public certification badge (marketing win).
- **Verdict: adopt-now.**

### Turborepo v8 remote-cache conformance (spec-driven) — **ADOPT NOW (free, DIY-small)**
- Turborepo publishes an **OpenAPI spec** for the remote cache (`/api/remote-cache-spec`,
  docs at turborepo.dev/docs/openapi). There is **no official conformance binary**, but
  the real protocol is tiny: `PUT /v8/artifacts/:hash`, `GET`, and **HEAD** existence
  checks (Turbo does *not* use the batch `POST /v8/artifacts` despite the spec).
- Fit: write a thin conformance test from the OpenAPI doc (or run **Schemathesis**, §6,
  against the spec) hitting your `turbo_v8.rs`. Cross-check against reference impls
  (`ducktors/turborepo-remote-cache`, `pkarolyi/garden-snail`) for behavior parity.
- Effort: low. Gap closed: Turbo surface conformance.
- **Verdict: adopt-now (lightweight; lean on Schemathesis + the OpenAPI spec).**

### Bazel REAPI v2 conformance — **TRIAL (free, but heavy)**
- The community **remote-apis-testing** project exists
  (`gitlab.com/remote-apis-testing/remote-apis-testing`) — it benchmarks/validates REAPI
  *servers* (Buildfarm/Buildbarn/BuildGrid/RBE) by building real projects (Abseil, Bazel)
  against them via GitLab CI + k8s + AWS. It's a heavyweight infra-conformance harness,
  **not** a drop-in unit suite, and is somewhat dormant **(verify activity 2025-2026)**.
- Cheaper path: drive a **real `bazel build --remote_cache=<your-worker>`** of a small
  repo in CI as a smoke/conformance signal, plus contract tests on the gRPC/proto
  (`bazelbuild/remote-apis` protos) for `bazel_v2.rs`. This is the highest-fidelity check
  (the actual client) without standing up the k8s harness.
- Effort: low (bazel-build smoke) → high (full harness). Gap closed: REAPI fidelity.
- **Verdict: trial** the real-bazel-build smoke now; skip the k8s harness unless a
  customer demands certification.

---

## 4. Deterministic simulation testing (DST) / chaos / fault injection

### madsim — deterministic async runtime + fault injection — **TRIAL (free)**
- Free, `madsim-rs/madsim`. tokio-compatible runtime that runs async code
  **deterministically from a seed** and injects faults (network, time). Used by
  RisingWave, in the FoundationDB/TigerBeetle lineage.
- Fit: strong for the **container's** distributed/async logic (replica-worker, failover-
  router, billing aggregation) — replay a failure from a seed. Caveat: requires your
  code to run on madsim's runtime (a real adoption commitment); Cloudflare Workers/DO
  aren't directly simulatable, so this targets the **Rust container plane**.
- Effort: high (runtime swap + `cfg(madsim)` plumbing). Gap closed: time/network/
  partition fault injection with deterministic replay.
- **Verdict: trial** on the most distributed crate first (failover/replica); don't
  boil the ocean.

### turmoil — deterministic network simulation — **TRIAL (free)**
- Free, tokio-rs. Lighter than madsim: simulates the **network** (latency, partitions,
  drops) deterministically for tokio code. Good middle ground if full madsim adoption is
  too invasive.
- Effort: medium. Gap closed: partition/latency behavior of multi-node paths.
- **Verdict: trial** (consider instead of madsim if you only need network faults).

### proptest-state-machine — model-based / stateful property testing — **ADOPT NOW (free)** ⭐
- Free, part of the proptest family you already use. Generates random **sequences** of
  operations against a reference model and asserts the SUT matches.
- Fit: ideal for the cache state machine — model `{PUT, GET, HEAD, DELETE, erase}`
  against a `HashMap` reference model and assert content-addressing, idempotency, quota
  accounting, and post-erasure absence hold over **random op sequences**. This is the
  natural extension of your existing proptest investment and would also stress the
  accounting race at the sequential-semantics level (loom covers the interleaving level).
- Effort: low-medium (you already know proptest). Gap closed: **stateful/sequence**
  correctness of every cache surface.
- **Verdict: adopt-now.**

### stateright — explicit-state model checker (Rust) — **SKIP (TLA+ covers it)**
- Free, but you already have TLA+ for model-checking. stateright's pitch is "model-check
  in Rust instead of TLA+"; given you've invested in TLA+ and an INV-registry cross-gate,
  adding a second model checker is redundant. **Verdict: skip** unless you want exec-level
  models tied to real Rust types (revisit later).

### Antithesis — autonomous DST SaaS — **SKIP (for now; price/fit)**
- Best-in-class autonomous deterministic simulation (Jane Street-led $105M Series A,
  Dec 2025). But it runs your **whole system in their hypervisor** and is enterprise-
  priced (~$20k-$100k+/yr **(verify)**). Wrong shape for a solo, cost-sensitive founder
  and not a clean fit for the Workers/DO plane. **Verdict: skip** (revisit post-revenue
  if the container plane grows).

---

## 5. Rust formal verification

### Kani — bounded model checker for Rust — **ADOPT NOW (free, targeted)** ⭐
- Free, `model-checking/kani` (AWS). Bounded model checking: verifies memory safety, a
  subset of UB, and **user assertions** over all inputs within bounds — no manual proof
  language. Actively used to verify the Rust **std library** (2025 AWS effort), so it's
  the most *usable* of the Rust verifiers today.
- Fit: prove pure, bounded invariants in the ~71 crates — content-address hashing
  helpers, quota arithmetic (no overflow / conservation), idempotency-key derivation,
  parsing of digests/refs. `#[kani::proof]` harnesses run in CI.
- Effort: medium (per-harness; keep bounds small). Gap closed: **exhaustive** checking of
  pure invariant code that proptest only samples.
- **Verdict: adopt-now** on a handful of money/accounting/hashing functions.

### Verus / Creusot / Prusti — deductive verifiers — **SKIP (too costly for solo)**
- All real in 2026 but require writing specs/proofs (Verus & Creusot use Rust-ish spec
  langs; Prusti is safe-Rust-only and can't do your unsafe/concurrent hot paths). High
  ongoing maintenance burden; verification community consensus is people reach for **Kani
  first** for convenience. **Verdict: skip** for a solo founder; Kani gives 80% of the
  value at 20% of the cost.

---

## 6. API conformance / fuzzing, contract, snapshot

### Schemathesis — property-based API fuzzing from OpenAPI — **ADOPT NOW (free)** ⭐
- Free OSS (paid cloud exists). Generates structurally-valid + edge-case requests from
  an OpenAPI/GraphQL schema; checks for 500s, schema violations, response conformance,
  content-type mismatches; **stateful** mode chains operations. Research shows 1.4-4.5x
  more defects than peers.
- Fit: point it at the **Turborepo OpenAPI spec** (§3) and any OpenAPI you expose for the
  Worker money-path/admin API. Black-box against a preview URL → **offloads the Mac**.
- Effort: low. Gap closed: surface fuzzing + response conformance across HTTP surfaces.
- **Verdict: adopt-now** (doubles as the Turbo conformance harness).

### Pact — consumer-driven contract testing — **TRIAL (free)**
- Free OSS; the **reference Pact core is written in Rust** and `pact_consumer` gives a
  Rust DSL (V3/V4). Worker (consumer) ↔ container (provider) and adapter seams are
  exactly the Worker↔container↔adapter contracts you freeze manually today.
- Fit: replaces hand-maintained contract-freeze prose with executable, versioned
  contracts; provider verification runs against the container. Note: you have a *static*
  contract-freeze discipline already — Pact adds the *executable enforcement* of those
  seams. The mixed TS-consumer / Rust-provider story works (Pact is polyglot) but is the
  fiddliest setup here.
- Effort: medium-high (broker or file-based pact exchange between TS and Rust).
- Gap closed: executable Worker↔container↔adapter contract drift detection.
- **Verdict: trial** on the single highest-risk seam (Worker→container money-path) before
  going broad.

### insta — snapshot testing — **ADOPT NOW (free)** ⭐
- Free, `mitsuhiko/insta` + `cargo-insta`. The de-facto Rust snapshot tool; inline or
  file snapshots, JSON/YAML/etc., `cargo insta review` workflow.
- Fit: lock down serialized protocol responses (REAPI/OCI/Turbo wire shapes, error
  bodies, billing/usage JSON) so any unintended change to a wire format fails loudly —
  cheap regression armor for the cache surfaces and money-path payloads.
- Effort: low. Gap closed: wire-format / serialization regressions.
- **Verdict: adopt-now.**

### cargo-semver-checks — public-API/SemVer drift — **ADOPT NOW (free)**
- Free, `obi1kenobi/cargo-semver-checks`. Detects breaking changes in a crate's public
  API. Given your **contract-freeze discipline between ~71 crates**, this mechanizes
  "did a frozen crate interface break?" in CI. **(verify** it plays with your workspace
  layout.)
- Effort: low. Gap closed: automated inter-crate contract-freeze enforcement.
- **Verdict: adopt-now.**

---

## 7. DB / migration testing (D1 + sqlx)

### Migration round-trip + forward/back tests — **ADOPT NOW (free, DIY)**
- No off-the-shelf tool fits Cloudflare **D1** cleanly. DIY is the SOTA here: a CI job
  that applies every migration in order to a throwaway D1/SQLite, asserts the resulting
  schema matches a checked-in golden schema (an **insta** snapshot, §6), runs seed +
  invariant queries (byte-accounting conservation, no orphan AC rows post-erasure), and —
  given your memory note that auth migrations are additive-only with an ADR gate — a lint
  that fails on destructive statements without a waiver.
- Effort: low-medium. Gap closed: migration correctness / schema drift / accidental
  destructive auth migration.
- **Verdict: adopt-now** (golden-schema snapshot + additive-only lint).

---

## 8. SBOM / build-integrity attestation

### GitHub Artifact Attestations (SLSA provenance + signed SBOM) — **ADOPT NOW (free)**
- Free for the container/wasm/OCI artifacts you build. GA since Jun 2024; one Action
  gets you **SLSA Build L2** provenance and a signed SBOM (Sigstore/cosign-verifiable),
  default-on for public repos through 2025-2026. Pairs with your reproducible-build gate
  to make supply-chain claims *verifiable*, and feeds the OCI-registry trust story.
- Effort: low (one workflow step). For an explicit CycloneDX/SPDX SBOM use **Syft** (most
  common) or `cargo-cyclonedx` **(verify maintenance)**; cosign can sign it.
- Gap closed: build provenance + signed SBOM (currently absent).
- **Verdict: adopt-now** (when you build release artifacts in GH Actions — which ties to
  §9).

---

## 9. Get CI OFF the shared Mac (cheap)

> GitHub adds a **$0.002/min platform fee on ALL Actions usage from Mar 1 2026**, so the
> calculus shifts toward 3rd-party runners that absorb/undercut it. All below are x86/ARM
> Linux — your Worker(wasm)/container/Rust workspace builds on Linux fine; only genuinely
> macOS-specific work needs a Mac, and almost none of your gates do.

### Blacksmith — **ADOPT NOW (free tier, then cheap)** ⭐ *(best "get off the Mac" pick)*
- **3,000 free min/month**, then ~**$0.004/min** (2 vCPU x64) — roughly **half**
  GitHub-hosted cost and ~2x faster; optional Docker-layer caching add-on (~$0.50/GB/mo).
  Drop-in `runs-on:` swap.
- Fit: moves the heavy Rust compiles + nextest off the founder's Mac with near-zero
  migration. The free tier alone likely covers a solo founder's fast-gate volume.
- **Verdict: adopt-now.** Single best option to stop the Mac from being the bottleneck.

### Depot — **TRIAL (paid, generous)**
- Dev plan 2,000 min included, **$0.004/min** after, up to ~3x faster; strong **caching**
  story (Depot is cache-first — on-brand for a cache company) incl. Docker + remote build
  cache.
- **Verdict: trial** if you want best-in-class cache acceleration beyond Blacksmith.

### Namespace — **TRIAL (paid; Bazel/Turbo-native)**
- Pay-as-you-go ~$0.0015/min (Dev), Team $100/mo/100k min. **Treats Bazel & Turborepo as
  first-class** cache targets — interesting since those are *your* surfaces (dogfood +
  comparison reference).
- **Verdict: trial** (esp. for Bazel/Turbo-heavy CI legs and competitive intel).

### Self-hosted on Hetzner-class — **TRIAL (cheap, aligns with roadmap)**
- A €4-15/mo Hetzner box as a self-hosted runner gives big, predictable compute and is
  **literally the roadmap's expansion-#1 infra** (ephemeral runners on cheap 3rd-party
  infra). Dogfood opportunity. More ops than Blacksmith though.
- **Verdict: trial** — natural fit, but Blacksmith is the lower-effort first move.

> Recommendation: **Blacksmith now** (free tier, zero ops), keep the Mac only for any
> truly macOS-specific check. Revisit Hetzner self-hosted as the runners product matures.

---

## TOP-7 ADOPT-NOW (free-first)

1. **loom** (free) — exhaustive interleaving proof; *catches the concurrent-PUT
   byte-accounting race*.
2. **proptest-state-machine** (free) — stateful/sequence correctness of every cache
   surface; extends existing proptest.
3. **OCI distribution-spec conformance** (free, official) — proves the OCI registry
   surface + self-cert badge; offloads the Mac.
4. **insta** (free) — wire-format/serialization regression armor for protocol + money-path
   payloads.
5. **Kani** (free) — bounded *proof* of pure accounting/hashing/idempotency functions.
6. **Schemathesis** (free) — OpenAPI fuzz/conformance (doubles as Turbo conformance);
   black-box → offloads the Mac.
7. **Blacksmith** (free 3k min/mo) — get heavy CI off the shared Mac, ~½ cost / 2x speed.

(Runners-up, all free, adopt-soon: **shuttle**, **cargo-llvm-cov fail-under + diff
coverage**, **cargo-semver-checks**, **GitHub Artifact Attestations/SLSA**, **migration
golden-schema snapshot**.)

## WORTH-PAYING-FOR (only if "fucking awesome", with $)

- **Depot** — ~$0.004/min after 2,000 included; cache-first runners (on-brand). Pay only
  if you want acceleration beyond Blacksmith's free tier.
- **Namespace** — ~$0.0015/min / Team $100/mo; Bazel+Turbo-native — worth it for
  Bazel/Turbo-heavy CI + competitive dogfooding.
- **Antithesis** — ~$20k-$100k+/yr **(verify)** — best-in-class autonomous DST, but
  enterprise-priced and wrong shape for solo today → **skip until post-revenue**.

## The 3 biggest verification GAPS (today)

1. **No concurrency verification** — directly responsible for the concurrent-PUT
   byte-accounting race slipping through. Closed by **loom** (proof) + **shuttle** (scale).
2. **No protocol conformance** — OCI/Bazel/Turbo surfaces are tested ad-hoc, not against
   their specs. Closed by **OCI conformance** + **Schemathesis/Turbo OpenAPI** +
   **real-bazel-build smoke**.
3. **Coverage measured but not gated** + **no executable cross-crate/wire contracts** —
   regressions are invisible. Closed by **cargo-llvm-cov diff-coverage gate**,
   **cargo-semver-checks**, **insta**, and (trial) **Pact**.

## Single best "get CI off the shared Mac"

**Blacksmith** — 3,000 free min/month, ~$0.004/min after, drop-in `runs-on:` swap,
~2x faster at ~half cost. Frees the founder's Mac with near-zero migration effort.
