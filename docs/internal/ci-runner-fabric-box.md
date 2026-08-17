# The `runs-on: corelink` box — measured inventory + migration plan

**Snapshot date: 2026-08-03. This is a snapshot, not a contract.** See
[§7 The image changes without notice](#7-the-image-changes-without-notice)
before you rely on any line below.

> **Update 2026-08-17 (delta since the snapshot):** `lighthouse-ci` was **deleted** —
> low-value perf score on admin-ui, not worth baking system Chrome, and dead under the
> block today so nothing green is lost. The class-D `lighthouse-ci` row below is stale.
>
> Migrating the remaining hosted lanes is **not a simple `runs-on:` flip**: the `corelink`
> label is a MIXED pool (Mac builders with a shared `$HOME` **and** the ephemeral CF fleet),
> the scheduler picks either, so a migrated job must work on BOTH. Concretely: a job that
> provisions a Rust toolchain or `cargo install`s a tool (e.g. `corelink-client-verify`'s
> `cbindgen-header-stable`) mutates the shared `~/.cargo`/`~/.rustup` on a Mac builder — the
> `actionlint` "must not provision a Rust toolchain" guard rejects it (see
> `scripts/validate_no_shared_rustup_mutation.py`; the fix pattern is the baked toolchain +
> `rust-toolchain.toml`, and cbindgen would need to be baked into the runner image, not
> `cargo install`ed). And a docker job (`smoke-install`) only works where the docker-shim is
> present (CF fleet), not on a bare Mac builder. So the remaining lanes need either an
> ephemeral-fleet-only label or runner-image work — a follow-up, tracked, NOT done here.
> Still hosted: `bazel-starter-ci`, `smoke-install`, `corelink-client-verify/cbindgen`,
> `docs-ci` a11y/lighthouse jobs, `e2e-prod`, `cosign-sign`.

Why this doc exists: reading `deploy/runner/Dockerfile` (in the
`corelink-runners` repo) answers *"what was installed"*, not *"what the box
has"* — and on two decisive points the answers differ. Everything here was
**measured on the box**, not inferred.

**Provenance — every fact below traces to one of two probe runs:**

| ref | run | workflow | date |
|---|---|---|---|
| **P1** | [`30724931256`](https://github.com/HuGR-Labs/corelink-server/actions/runs/30724931256) | `probe-corelink-box` (from the now-closed PR #932) | 2026-08-02 |
| **P2** | [`30825772599`](https://github.com/HuGR-Labs/corelink-server/actions/runs/30825772599) | `.github/workflows/runner-probe.yml` (on `main`) | 2026-08-03 |

`runner-probe.yml` is the live, re-runnable version and is a strict superset of
P1's probe. **Re-run it (`workflow_dispatch`) rather than trusting this page**
whenever the runner image may have moved.

---

## 1. The box

| | measured | ref |
|---|---|---|
| vCPU | **4** (AMD EPYC) | P2 |
| RAM | **11.9 GiB** (`MemTotal: 12514268 kB`), **swap 0** | P1, P2 |
| Disk | **18 GB total, ~16 GB free** at job start (`/dev/vdc`, 13% used) | P1, P2 |
| OS | **Ubuntu 24.04.4 LTS** (noble) | P1, P2 |
| Kernel | `6.18.36-cloudflare-firecracker-2026.6.17` — **Firecracker microVM** | P1, P2 |
| Host | `cloudchamber`; runner names are ephemeral (`cf-runner-<hex>`) | P1, P2 |
| User | `runner` (uid 1001), `$HOME=/home/runner`, **`sudo` available (passwordless)** | P1, P2 |
| Runner | GitHub Actions runner `2.335.1` | P1, P2 |

`sudo` + `apt-get` (2.8.3) are both present, so a workflow *can* install what it
needs at run time. That is a per-run cost, not a fix — see §8.

**Egress** (P2): `github.com`, `api.github.com`, `static.rust-lang.org`,
`registry.npmjs.org`, `pypi.org` all reachable. `ghcr.io/v2/` → 401 and
`mirror.gcr.io` → 302 (both expected for an unauthenticated `HEAD`). The box is
not network-restricted in any way that blocks a normal gate.

`CLW_ENDPOINT=https://corelink-api.humangr.com` is set in the environment (P1).

---

## 2. ⚠️ The counter-intuitive fact: `rustup` honours `rust-toolchain.toml`

**This is the single most misleading thing about the box, and it has already
misled people in both directions.** Read it carefully.

The image's **default** toolchain and the toolchain the **repo actually
compiles with** are different, and both statements are true at the same time:

```
# BEFORE anything touches the repo (P2):
installed toolchains:  1.96.0-x86_64-unknown-linux-gnu (active, default)
default toolchain:     1.96.0-x86_64-unknown-linux-gnu (default)

# INSIDE the repo checkout (P2):
active toolchain:      1.91.1-x86_64-unknown-linux-gnu
                       (overridden by '…/corelink-server/rust-toolchain.toml')
```

So:

- **The image ships Rust 1.96.0.** If you inventory the box outside a checkout,
  that is what you see.
- **This repo builds with 1.91.1**, because `rust-toolchain.toml` pins
  `channel = "1.91.1"` and `rustup` silently obeys it. P1, which probed *inside*
  the checkout, reported `rustc 1.91.1 (ed61e7d7e 2025-11-07)` — correct, and
  easy to mistake for "the image ships 1.91.1". It does not.

**The consequence people miss: the override is not free.** Resolving it
downloads the pinned toolchain on a cold box:

```
info: syncing channel updates for 1.91.1-x86_64-unknown-linux-gnu
info: downloading 8 components
  >>> rust-toolchain.toml resolution took 15s        (P2)
```

**~15 s per job**, every job, for as long as the image default and the repo pin
disagree. Aligning the image to 1.91.1 would erase it.

### 2.1 Components and targets — a live trap

After resolution the 1.91.1 toolchain has (P2):

| asked for | present on the box | note |
|---|---|---|
| `rustfmt` | ✅ `rustfmt 1.8.0-stable` | |
| `clippy` | ✅ `clippy 0.1.91` | |
| `rust-std` wasm32-unknown-unknown | ✅ (`rustup target add` → **0 s**, already there) | |
| `x86_64-unknown-linux-musl` | ✅ | |
| **`llvm-tools-preview`** | ❌ **absent (probe counted 0)** | ⚠️ see below |

⚠️ **`coverage.yml` needs `llvm-tools-preview`** (it runs `cargo-llvm-cov` and
installs the component via its `dtolnay/rust-toolchain` step,
`.github/workflows/coverage.yml:58-61`). The box does **not** ship it. #932
classified `coverage` as "class A, moves today" — that was only true because
#932's policy kept every existing step, including the component-installing one.
**Anyone moving `coverage.yml` to `runs-on: corelink` must confirm a
component-installing step survives the move.** `coverage.yml` also currently
runs on `ubuntu-x64-4core` (not `ubuntu-latest`) with a comment recording that
a 2-core box OOMs during the link — the corelink box is 4-core, which matches,
but that is the floor, not headroom.

---

## 3. ⚠️ Node: JS *Actions* run; `run:` steps calling `node` do **not**

Another fact that reads backwards. Both of these are true:

- **`node` / `npm` / `npx` / `pnpm` are ABSENT from `PATH`** (P1, P2). The full
  `PATH` is `/home/runner/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin` — no node anywhere.
- **JS-based Actions still work**, because the GitHub runner ships its own Node
  under its `externals/` directory (P1):

  ```
  /opt/actions-runner/externals/node20
  /opt/actions-runner/externals/node20_alpine
  /opt/actions-runner/externals/node24
  /opt/actions-runner/externals/node24_alpine
  ```

  The runner invokes these directly by absolute path; it never consults `PATH`.

**The distinction, stated as a rule:**

| construct | works on the box? | why |
|---|---|---|
| `- uses: actions/checkout@…` (a JS Action) | ✅ **yes** | runner's bundled `externals/node*` |
| `- uses: actions/setup-node@…` | ✅ **yes** | JS Action; it then *downloads* a real node onto `PATH` |
| `- run: node script.js` | ❌ **no** | needs `node` on `PATH` |
| `- run: pnpm install` | ❌ **no** | needs `pnpm` on `PATH` |
| `- run: python3 scripts/validate_specs.py` | ❌ **no** | needs `python3` on `PATH` |

This is why `actions/checkout` ran fine on the box from day one despite the
inventory saying "node: ABSENT". Both observations were correct.

---

## 4. What is present

`jq 1.7` · `git 2.43.0` · `curl 8.5.0` · `unzip 6.00` · `tar 1.35 (GNU)` ·
`make 4.3` · `cc 13.3.0` (Ubuntu gcc) · `ld 2.42` · `sha256sum` (coreutils 9.4) ·
`apt-get 2.8.3` · `sudo` · `rustup 1.29.0`

CoreLink-specific: **`clw 0.1.4`**, `cargo-deny 0.19.8`, `cargo-audit 0.22.2`
(P1).

## 5. What is ABSENT from `PATH`

**`node` · `npm` · `npx` · `pnpm` · `python3` · `pip3` · `docker` · `gh` ·
`go` · `java` · `wget`** (P2; P1 agrees on the subset it probed).

> `go` was **not** probed by P1 — only P2 checks it. If you see `go` claimed as
> absent anywhere citing P1, that claim is unsourced. P2 confirms it: `go ABSENT`.

---

## 6. Pre-seeded `~/.cargo` — why a cold compile is competitive

The image ships a **populated** cargo home, before any job work (P2):

```
CARGO_HOME=/home/runner/.cargo
415M    /home/runner/.cargo
  registry/cache entries: 362
  registry/src   entries: 364
```

This is the reason a "cold" box is not really cold. Measured end-to-end on a
fresh box against `corelink-hash` (P2):

| step | wall time |
|---|---|
| `cargo fetch --locked` | 12 s |
| `cargo clippy -p corelink-hash --all-targets -- -D warnings` | 20 s |
| `cargo test -p corelink-hash --all-targets` | 31 s |
| `cargo test --release -p corelink-hash` | 50 s |
| `cargo check -p corelink-hash --target wasm32-unknown-unknown` | 4 s |

After that build: `~/.cargo` grew 415M → **1002M**, `target/` → **996M**, and
disk sat at **4.8 G used / 14 G free (27%)**. A single crate consumes roughly
1 GB of the 18 GB disk. **A full-workspace build has not been measured** — do
not extrapolate this table to one.

---

## 7. The image changes without notice

**Nobody monitors the runner image, and it moves.** Rust went **1.91.1 → 1.96.0
in two days** between P1 and P2 with no announcement, no changelog, and no
signal to this repo. Nothing in CoreLink pins, verifies, or alerts on the image
contents.

Practical consequences:

1. **Every fact on this page has a date on it and may already be false.**
   `runner-probe.yml` is the authority; this doc is a cached read of it.
2. **Do not encode image contents into a gate's assumptions** without a step
   that asserts them. A gate that silently depends on the image is a gate that
   fails on a Tuesday for no reason attributable to the diff.
3. Re-run `runner-probe.yml` after any unexplained self-hosted failure **before**
   blaming the change under test.

---

## 8. Migration plan: the A–E classification

### 8.1 The classes

| class | meaning |
|---|---|
| **A** | Moves today. The box has everything the workflow's `run:` steps invoke. |
| **B** | Unlocked by adding **`python3`** to the image. |
| **C** | Moves today, but **pays a toolchain download per run** (carries its own `setup-*` action). |
| **D** | Needs **more than python3** in the image — `pnpm` and/or `gh`. |
| **E** | **Must stay hosted** — needs `docker`. |

### 8.2 Original counts (#932, 2026-08-02) vs re-derived (this doc, 2026-08-03)

| class | #932 | re-derived | why they differ |
|---:|---:|---:|---|
| A | 25 | **20** | 11 workflows already migrated by #1003 / #1005 |
| B | 13 | **20** | wider `python3` detection; several ship-gates land here |
| C | 17 | **8** | stricter: only counts a *real* `uses:` setup action |
| D | 15 | **17** | `docs-ci` moved C→D (see below) |
| E | 1 | **2** | `container-build-push-prod` is partly `docker` |
| **hosted total** | **71** | **67** | |

> **Method + its limits.** The re-derived table was produced mechanically:
> parse each workflow, collect binaries invoked in non-comment `run:` lines,
> subtract what a `uses:` setup action provides. It is a **planning aid, not a
> gate.** It was wrong twice while being built — first missing `pnpm` in
> multi-line `run:` blocks, then classifying `docs-ci` as C because a **code
> comment** in it mentions `pnpm/action-setup`. Both are fixed; assume more
> like them. **Confirm per-workflow before moving anything.**

### 8.3 Per-workflow assignment (re-derived, 2026-08-03)

**A — moves today (20):**
`byok_kill_switch_drill_weekly`, `byok_matrix_weekly`, `cargo-deny`,
`cas-canary`, `corelink-adapter-host`, `corelink-client-verify`,
`corelink-meta`, `corelink-server`, `cosign-sign`, `coverage`⚠️, `docs-vale`,
`dpa-legal-review`, `dr-drill-monthly`, `gc-sweep-dry-run`,
`legal-changes-review`, `mutation-pr`, `reproducible-build`,
`sbom-consolidated`, `tenant-path`, `terraform-lint`

  ⚠️ `coverage` needs `llvm-tools-preview` — see §2.1.

**B — unlocked by `python3` in the image (20):**
`ac-bucket-acl-cron`, `audit-chain-daily-verify`, `bazel-starter-ci`,
`billing-reconcile-daily`, `cargo-audit`, `cas_foundation`,
`d1-migration-validate`, `gc-ship-gate`, `lockfile-diff`, `okf_nightly`,
`okf_wiki`, `pentest-findings-sync`, `perf-regression`🚫, `region_pinning`,
`s07-ship-gate`, `s08-ship-gate`, `s09-ship-gate`, `s10-ship-gate`,
`secrets-drift`, `semgrep`

**C — moves, pays a per-run toolchain download (8):**
`backup-daily`, `backup-daily-verify`, `e2e-prod`, `ffi-matrix-ci`,
`pnpm-audit`, `signup-worker-vitest`, `translation-import-validate`,
`worker-vitest`

**D — needs `pnpm` / `gh` (17):**
`admin-ui-ci`💰, `admin-ui-deploy`, `admin-ui-e2e`💰, `api-reference-sync`,
`cf-deploy-prod`, `codeql`, `compliance-weekly`, `docs-ci`💰, `docs-deploy`,
`lighthouse-ci`💰, `mutation-nightly`, `okf-autoreconcile`, `openapi-validate`,
`pre-cutover-weekly-cron`, `release-notes`, `sdks-js`, `subprocessors-sync`

**E — must stay hosted (2):**
`smoke-install` (docker), `container-build-push-prod` (docker; partly migrated
already)

**Already migrated to `runs-on: corelink` on `main` (11 files):**
`cargo-audit`, `container-build-push-prod`, `corelink-hash`, `corelink-reapi`,
`corelink-worker`, `fabric-soak-proof`, `gitleaks`, `proof-runs-on-corelink`,
`runner-probe`, `rustfmt`, `trivy`

### 8.4 🚫 Do NOT move `perf-regression`

`perf-regression` (12–13 runs / 3 days, ~1118 s median, ~191 billed min) is
class B on paper. **Do not move it.** Criterion baselines are CPU-bound;
changing the runner changes the numbers and **invalidates the gate itself**.
Its comparison baseline was captured on hosted hardware. Moving it does not
make the gate cheaper — it makes it meaningless.

---

## 9. Where the money actually is

Measured **2026-08-01 → 2026-08-03**: **4429 billed minutes across 22
workflows ≈ $26.57**.

| workflow | billed min | $ | runs/3d ✅ | class |
|---|---:|---:|---:|---|
| `admin-ui-e2e` | 1112 | $6.67 | **83** | D |
| `admin-ui-ci` | 811 | $4.87 | **77** | D |
| `docs-ci` | 712 | $4.27 | **43** | D |
| `lighthouse-ci` | 308 | $1.85 | **84** | D |
| **subtotal** | **2943** | **$17.66** | | **66% of the bill** |

✅ = run counts independently re-verified via `gh run list` for this doc; they
match exactly. The **billed-minute and dollar figures are taken on trust** from
the cost analysis that commissioned this doc — the org billing API
(`/orgs/HuGR-Labs/settings/billing/actions`) now returns **410 Gone** and its
replacement needs an `admin:org` scope this tooling does not hold, so they
could not be re-derived here.

**All four are `ubuntu-latest`, and all four are class D** — every one calls
`pnpm install` from a `run:` step, and none of them uses `pnpm/action-setup`
directly.

By contrast, **the entire Rust class-A opportunity is ~647 billed min ≈
$39/mo** — roughly **a quarter** of what node+pnpm in the image would unlock.

### 9.1 The conclusion the data supports

> **The next high-leverage move is a one-file image change (add `node` +
> `pnpm` to `corelink-runners`' `deploy/runner/Dockerfile`), not another
> label-swap wave.**

Another wave of `runs-on:` edits chases the smaller quarter of the bill. One
Dockerfile line unlocks the 66%.

**One honest caveat, so nobody over-claims this.** The four heavy workflows are
not *hard*-blocked today. Each already carries `actions/setup-node`, and the
repo's local `./.github/actions/setup-pnpm` composite has a documented fallback
to `pnpm/action-setup` "when a runner genuinely lacks pnpm (e.g. a future
hosted/Linux runner)". So in principle they could move now and pay a per-run
node + pnpm download. **That path has never been exercised on a corelink box.**
Baking the tools into the image is better on both counts — it removes the
per-run download *and* it avoids relying on an untested fallback — but the move
still needs a proving run, **not a blind label swap**.

### 9.2 The other axis: Mac contention (free, but costly)

Self-hosted Mac minutes are not billed, but they burn the founder's cores and
the fleet shares one physical machine.

**`region_pinning.yml` is the largest single consumer: ~795 wall-minutes of Mac
in 3 days**, across **3** self-hosted Rust jobs on the PR path —
`clippy-and-existing-tests`, `proptest-30k`, `adversarial-tests` (all
`runs-on: [self-hosted, mac, corelink-builder]`) — plus a 4th,
`proptest-100k-nightly`, on the nightly path. Reducing hosted spend by pushing
work onto the Mac trades a dollar cost for a **capacity** cost on a machine
that has already crashed under load.

---

## 10. ⚠️ The shared-`$HOME` defect — read before moving any workflow

The self-hosted Mac fleet shares **one `$HOME`**. This has produced at least
three distinct failures, and it is the reason a "no step is changed" migration
policy is unsafe:

1. **`Swatinem/rust-cache`** corrupts across concurrent runs. Fixed in **#980**
   by gating every such step on `if: runner.environment == 'github-hosted'`.
2. **`pnpm/action-setup`** installs into a shared `~/setup-pnpm`; concurrent
   runs overwrite it mid-flight (`MODULE_NOT_FOUND` during `pnpm install`).
   Fixed by the local `./.github/actions/setup-pnpm` composite.
3. **The pnpm content-addressable store** (one shared `store/v10`) was corrupted
   by concurrent installs on 2026-08-02 (`ERR_PNPM_ENOENT … reflink`). Fixed by
   giving each runner its own store, keyed on `RUNNER_NAME`.

Note that the corelink (Firecracker) boxes are **ephemeral with a private
`$HOME`**, so they do not have this defect — but they also mean the
`RUNNER_NAME`-keyed pnpm store is a **fresh empty directory every run**, i.e.
no cache benefit there. Neither is a blocker; both are things to expect.

**The lesson #932 taught by getting it wrong:** its stated policy was *"No step
is changed — on a self-hosted runner the saving is total however many steps
run."* That reasoning is sound about *billing* and wrong about *correctness*.
Steps that are safe on an ephemeral hosted runner are not automatically safe on
a shared self-hosted one. **Migrating a workflow means reviewing its steps, not
just its label.**

---

## 11. Re-measuring

```bash
gh workflow run runner-probe.yml --repo HuGR-Labs/corelink-server
gh run list --repo HuGR-Labs/corelink-server --workflow runner-probe.yml --limit 1
```

It is `workflow_dispatch`-only and is **not** a gate: it reports and never
fails a PR for anything it finds absent. Update this doc's snapshot date and
any changed rows when you run it.

---

## Provenance note

Sections 1–8 were salvaged from PR **#932**
("ci: inventory the fabric box, then move the first PR gate onto it"), which
was closed unmerged on 2026-08-03. Its measurement work was sound and is
preserved here; its code was superseded by **#1003** (migration wave 1),
**#1005** (wave 2), and `runner-probe.yml` on `main`. Sections 9–10 post-date
#932 and revise its priority order.
