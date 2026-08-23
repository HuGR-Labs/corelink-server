# BACKLOG — the single source of truth

Every open item across the CoreLink family lives here: `corelink-server`,
`corelink-runners`, `corelink-workspaces`. If it is not in this file, it is not
tracked. Session notes, memory files and chat threads are working material, not
the record.

## Why this file checks itself

On 2026-08-23 a day of planning was built on notes that had quietly gone stale.
Three items recorded as open had shipped days earlier; one cited a count of
hosted CI lanes that matched neither the file count nor the job count. Nothing
was dishonest — the notes were written once and never re-checked, and nothing
existed that could notice.

So a list alone is not enough. **Every item carries a command that decides
whether its own claim is still true**, and `scripts/backlog_verify.py` runs them:

| verify exits | meaning |
|---|---|
| `0` | CONFIRMED — the declared status still holds |
| non-zero | **DRIFTED** — the item says one thing and the repo says another |

An item that genuinely cannot be checked by a command declares `verify: manual`
and must carry a fresh `last-verified` date. Those **decay**: past 14 days they
go STALE and fail the gate. An unverifiable claim is allowed; sitting unchallenged
forever is not, because that is precisely what happened to the notes this replaces.

```bash
python3 scripts/backlog_verify.py
```

The gate runs on every PR touching this file and daily on a schedule. The cron
earns its keep under this repo's own rule — an item here closes when work lands
in *another* repo, which happens with no commit to this one.

## Rules

- **Fix the item or fix the world. Never delete the check.** A DRIFTED item means
  reality moved; the answer is to update the status or finish the work.
- **`owner: owner` means it needs the human** — a credential, a payment, a
  deletion, a legal call. Everything else is `owner: tl` and is mine.
- **A `verify` for an `open` item must exit 0 while the work is unfinished** and
  start failing once it lands. That is what makes the file self-closing: finishing
  the work turns the gate red until the status is updated to match.

---

## Runner fabric — the 2026-08-23 leaked-box incident

Three boxes ran 10.5 h against a 15-minute idle window (~126 vCPU-h). Four
distinct defects, in layers. `#486` (reaper) and `#487` (PID 1 answers SIGTERM)
have landed.

### B-001 — the idle deadline is in-memory and rearms itself

`sleepAfterMs` is a bare instance field and the SDK's `Container` constructor
calls `renewActivityTimeout()` unconditionally, so any DO re-instantiation — an
eviction, a redeploy, even a `/v1/status` poll on a cold stub — silently rearms
the full window with no real activity. This sits *upstream* of the SIGTERM defect:
there `stop()` was called and had no effect; here it is never called at all.

**Landed 2026-08-23 in `#490`** — a durable last-activity stamp plus escalation to
`destroy()` after two soft stops that did not end a still-running container. The
check below is now inverted: it fails if the backstop ever leaves `main`.

This item is also the first thing the gate caught. Merging `#490` turned it
DRIFTED within minutes, exactly as designed — finishing the work reddens the gate
until the record is updated.

```backlog
id: B-001
repo: corelink-runners
owner: tl
status: done
verify: |
  gh api "repos/HuGR-Labs/corelink-runners/contents/deploy/cloudflare/src/index.ts?ref=main" \
    -q .content | base64 -d | grep -q enforceDurableIdleBackstop
verify-means: done while main carries the durable backstop; red if it is ever removed
last-verified: 2026-08-23
```

### B-002 — the reaper still starts from our own bookkeeping

`#486` reaps via the durable `sbox:` record written at spawn. The three boxes
leaked on 2026-08-23 had no such record, so the layer built to catch bookkeeping
loss still begins with bookkeeping. Needs a reconciliation path keyed on platform
truth. The instance `name` field is the handle UUID that `/v1/teardown` takes.

```backlog
id: B-002
repo: corelink-runners
owner: tl
status: open
verify: manual
verify-means: design not started; nothing in the repo to grep for yet
last-verified: 2026-08-23
```

### B-003 — the 864s ceiling from 2026-08-02 has no established mechanism

That incident concluded jobs past ~900 s were being SIGTERMed. They could not
have been: `exec` had left `entrypoint.sh` twelve days earlier, so PID 1 had no
handler and the signal was never delivered. The 864 s figure is real data. A
false causal chain must not stand in the CHANGELOG.

```backlog
id: B-003
repo: corelink-runners
owner: tl
status: open
verify: |
  gh api "repos/HuGR-Labs/corelink-runners/contents/CHANGELOG.md?ref=main" \
    -q .content | base64 -d | grep -q "was being SIGTERMed"
verify-means: open while the unqualified claim stands; closes when corrected or explained
last-verified: 2026-08-23
```

---

## CI integrity

### B-004 — workflows silenced by the 2026-08-08 mass-disable

**21** were disabled inside a single 39-second window on 2026-08-08 — one script,
not 21 decisions, ~24 minutes into a day-long GitHub Actions billing outage. Not
three, as first reported: `gh api ... --paginate` is required or the count comes
out wrong.

**12 have been re-enabled and verified**, including `backup-daily` (dispatched and
proven to run: real runner, 13 steps, 50 s, success — D1 backups had not run for
16 days). Two of them turned out not to be dormant crons at all but **silenced
pull-request gates** — `region_pinning`, the Schrems II tenant-isolation forensic
check, and `corelink-client-verify` — which had been letting PRs go green
unchecked for 15 days.

**17 are now re-enabled.** The nine chronically-red ones were triaged individually
rather than flipped: `pnpm-audit` (its advisory — a HIGH in `js-yaml` — turned out
to have been fixed nine days earlier in `#1129`), `cas_foundation`, `coverage` and
`ffi-matrix-ci` (all genuinely dispatch-only parks, now enabled with their crons
still commented out so nothing auto-fires), and `smoke-install`, which is enabled
urgently because it is actively catching a live regression — see [B-018].

**4 remain, each with a decision rather than a flip:** `reproducible-build` and
`bazel-starter-ci` (see [B-016] and [B-017]), `mutation-nightly` (every crate job
dies on `--in-place` conflicting with `--jobs` after a cargo-mutants CLI change —
a one-line fix, but its weekly cron does not earn its keep under this repo's own
rule, since mutation kill-rate only changes with a commit), and
`pre-cutover-weekly-cron`, whose premise expired: it verified readiness for a GA
cutover that happened on 2026-07-10, six weeks ago, and it now dies trying to
apply a GitHub label that does not exist. That one should be **retired**, not
fixed — a one-line repair to a workflow whose reason to exist has passed is the
wrong move.

```backlog
id: B-004
repo: corelink-server
owner: tl
status: open
verify: |
  test "$(gh api repos/HuGR-Labs/corelink-server/actions/workflows --paginate \
    -q '.workflows[]|select(.state!="active")|.path' | wc -l | tr -d ' ')" -gt 0
verify-means: open while any workflow is non-active; closes only when every one is enabled or deleted
last-verified: 2026-08-23
```

### B-005 — hosted lanes still fire on PR/push

The mandate is zero GitHub-hosted spend: a job runs on the self-hosted `corelink`
fleet or it does not run. `lighthouse-ci` is a genuine exception pending a
browser-baked image; the rest are not.

```backlog
id: B-005
repo: corelink-server
owner: tl
status: open
verify: |
  grep -lE '^\s+runs-on:\s*ubuntu-latest' .github/workflows/*.yml \
    | xargs grep -l 'pull_request' | head -1 | grep -q .
verify-means: open while any ubuntu-latest workflow still has a pull_request trigger
last-verified: 2026-08-23
```

---

## Product and cost

### B-006 — F4 (the N-image menu) waits on demand, not on cost

The cost premise that justified deferring this is measured false: a second
container class costs nothing at zero idle. The criterion is now a number —
`capability_claim_unserved`, live since `#485`, currently **0**. Nobody has yet
asked for a machine shape the fleet does not serve.

```backlog
id: B-006
repo: corelink-runners
owner: tl
status: parked
verify: manual
verify-means: parked until capability_claim_unserved leaves zero; read via /internal/v1/metrics
last-verified: 2026-08-23
```

### B-007 — the near-ceiling warning goes nowhere

`$`-ceiling hardening C1 landed in `#1074`. C2 — routing the warning to a paging
or email sink — did not. The emitter is still a bare `tracing::warn!`.

```backlog
id: B-007
repo: corelink-server
owner: tl
status: open
verify: grep -q 'deliberate follow-up (C2)' crates/corelink-container/src/tenant_quota.rs
verify-means: open while the emitter's own TODO comment stands
last-verified: 2026-08-23
```

### B-008 — PagerDuty accepts our events; nobody knows if they reach a human

**Half of this closed on 2026-08-23.** Dispatching `audit-chain-daily-verify`
(run `32661206591`) fired **7 SEV-0 events** and the workflow's dispatch step —
which checks its own response and only prints success on HTTP `202`
(`audit-chain-daily-verify.yml:292-303`) — printed 7 successes and **zero**
`::error::PagerDuty enqueue` lines. So the routing key is valid and armed, and
PagerDuty's Events API accepted every one. That rules out the "always prints
success" failure mode and the "key was never provisioned" theory.

What remains genuinely open is the last hop: **escalation**. Accepted by
PagerDuty is not the same as delivered to a person. And it now carries an
uncomfortable implication — if the escalation policy works, the owner has been
paged SEV-0 daily since roughly 2026-07-17; if it does not, then no alert this
system raises has ever reached anybody.

```backlog
id: B-008
repo: corelink-server
owner: owner
status: open
verify: manual
verify-means: only the owner or the PagerDuty incident log can say whether a page ever arrived
last-verified: 2026-08-23
```

---

## Compliance and legal

### B-009 — the seven-year Object-Lock drain is a stub

The last open piece of DSR WI-S11-008. Erasure itself is live and proven. The
retention backend is in-memory: no stored `retain_until`, no Governance or
Compliance mode flag, no R2 Object-Lock enforcement. Needs a policy decision
before any build.

```backlog
id: B-009
repo: corelink-server
owner: tl
status: open
verify: test -f crates/corelink-privacy-erasure-worker/src/backends/r2_cas_legalhold_pseudo.rs
verify-means: open while the pseudo backend is still the implementation
last-verified: 2026-08-23
```

### B-015 — the sealed audit archive was never built, only its verifier

`audit-chain-daily-verify` lists `corelink-audit-archive` under `audit/<date>/`.
That bucket does not exist and never did. The producer does —
`crates/corelink-audit-chain/src/archive_producer.rs`, writing exactly the key
shape the verifier looks for — but it is dead on two independent axes: its only
caller sits behind the Cargo feature `cf-billing-real`, which no build or deploy
path ever passes, and the crate that owns that caller is a wasm cdylib the live
Worker never imports. Commit `8ba0353b` added producer and verifier together;
nothing since ever wired the producer to a binding. The `AUDIT_BUCKET` binding
that would do it exists **only** on the quarantined `feat/remediation-gated-features`
branch, which must not land, and even there it backs a different consumer.

So this is not a broken cron. **Live tamper-evidence is D1-only** — a real BLAKE3
hash chain with an Ed25519-signed head, sealed by `/_internal/audit/drain` — with
**no offsite, immutable copy**. The control the specs describe (S-09: R2 NDJSON
archive with 7-year retention) exists in source and tests, not in the running
system. Pairs with [B-009], which is the retention half of the same gap.

Meanwhile the verifier has fired SEV-0 pages every day since ~2026-07-17 over a
configuration that could never have been satisfied — burying any real chain break
under weeks of false alarm.

```backlog
id: B-015
repo: corelink-server
owner: tl
status: open
verify: |
  ! gh api "repos/HuGR-Labs/corelink-server/contents/wrangler.toml?ref=main" \
      -q .content | base64 -d | grep -q "AUDIT_BUCKET"
verify-means: open while main has no AUDIT_BUCKET r2 binding wiring the archive producer
last-verified: 2026-08-23
```

### B-010 — the TLS floor change is recorded nowhere in the repo

The `humangr.com` zone's `min_tls_version` was lowered 1.3 to 1.2 because
1.3-only was blocking sccache clients. Live and correct, but it exists only as a
manual API change with no ADR and no IaC record.

```backlog
id: B-010
repo: corelink-server
owner: tl
status: open
verify: |
  ! ls docs/design/ | grep -qi tls
verify-means: open while no ADR mentioning TLS exists
last-verified: 2026-08-23
```

---

## Repo hygiene

### B-016 — the build-attestation lanes attest an artifact that cannot exist

`reproducible-build` compiles `corelink-worker` for `wasm32-unknown-unknown` and
then hashes `target/wasm32-unknown-unknown/release/corelink_worker.wasm`. That
file has never existed: the crate has no `[lib] crate-type = ["cdylib"]`, so the
build can only ever emit an `.rlib`. Every run dies on `sha256sum: ... No such
file or directory` **after** a successful compile. So reproducibility has never
been tested even once — the gate has been red for a structural reason, not a
flaky one.

Worse, the deployed Worker is not wasm at all: `wrangler.toml:15` reads
`main = "worker/src/index.ts"`. So the artifact being attested is neither
producible nor shipped. `release-slsa3.yml` builds the identical artifact with
the identical command and has run exactly once, on 2026-05-29, and failed — which
means the SLSA provenance bundle has most likely never been produced either.

The fix is therefore **not** "add cdylib". It is to decide what should actually be
attested — the TypeScript Worker bundle and the container image are what ship —
or to withdraw the claim. No customer-facing page currently makes a reproducible-
build or SLSA claim, so this is an internal-integrity gap today, not a false
public statement; it would become one the moment such a page is written.

```backlog
id: B-016
repo: corelink-server
owner: tl
status: open
verify: |
  ! git show origin/main:crates/corelink-worker/Cargo.toml | grep -q 'crate-type'
verify-means: open while corelink-worker still cannot emit the wasm its attestation lanes hash
last-verified: 2026-08-23
```

### B-017 — our own flagship Bazel cache demo records zero cache hits

`bazel-starter-ci` fails on two independent breaks. One is cosmetic — a negative
scenario asserts a `CORELINK_PAT` error string the CLI no longer emits. The other
is not: the "Bazel cache hit (>= 80%)" job reports `WARNING: No remote cache
entries in execution log.` **Zero** REAPI hits. The example that exists to
demonstrate CoreLink's Bazel cache is not hitting the cache at all in CI.

That is close to the product claim, so it needs a real answer rather than a
threshold tweak. Likely a credential or tenant wiring gap in the example's
`.bazelrc` credential helper rather than Bazel itself — but likely is not
established.

It was also a **silenced PR gate**: the workflow has a live `pull_request` trigger
scoped to `examples/bazel-starter/**`, so any PR touching that path between
2026-08-08 and today merged without it.

```backlog
id: B-017
repo: corelink-server
owner: tl
status: open
verify: |
  test "$(gh api repos/HuGR-Labs/corelink-server/actions/workflows/bazel-starter-ci.yml -q .state)" != "active"
verify-means: open while the workflow stays disabled; re-enabling it requires the cache-hit break fixed first
last-verified: 2026-08-23
```

### B-018 — `corelink --version` demands a PAT it is documented not to need

`smoke-install` runs the published CLI inside a clean container. Its last real run
(2026-08-05) shows **both** invocations failing — including the step explicitly
documented as the unauthenticated path — with `error: No PAT found. Set env var
CORELINK_PAT or run corelink config set auth.pat <value>`.

This is a customer-facing break: the very first command a new user types fails on
a fresh install. The gate is working exactly as designed and found a real
regression; it went unreported only because the gate was switched off. Its own
header records that it caught this same class of bug once before (`#842`) while
unit tests stayed green — the lesson being that the published binary must be
operated, not unit-tested.

The defect is in the separate `corelink-cli` repo, not here. The workflow has
been re-enabled.

```backlog
id: B-018
repo: corelink-server
owner: tl
status: open
verify: manual
verify-means: lives in the corelink-cli repo; settled by running the published binary with no PAT
last-verified: 2026-08-23
```

### B-011 — ~115 branches in corelink-runners have no open PR

Large relative to the other two repos, which carry none. Needs a merged-vs-
abandoned sweep. Live worktrees point at some of them, so nothing may be deleted
blind.

```backlog
id: B-011
repo: corelink-runners
owner: tl
status: open
verify: |
  test "$(gh api repos/HuGR-Labs/corelink-runners/branches --paginate -q '.[].name' | wc -l | tr -d ' ')" -gt 20
verify-means: open while the branch count is unswept; the threshold is a floor, not a target
last-verified: 2026-08-23
```

---

## Needs the owner

These are not mine to do: they need a credential, a permanent deletion, or a
decision only the owner can make.

### B-012 — a non-Actions credential so bot PRs get CI

`GITHUB_TOKEN`-created events do not trigger workflows — a recursion guard — so
bot-opened PRs arrive with zero checks and a green-looking gate that proves
nothing. Needs a fine-grained PAT or GitHub App token with `contents:write` and
`pull_requests:write`. Deliberately not reusing an existing release token: one
secret, one purpose.

```backlog
id: B-012
repo: corelink-server
owner: owner
status: open
verify: manual
verify-means: settled when a bot-opened PR shows checks
last-verified: 2026-08-23
```

### B-013 — three PEM files in ~/Downloads

Permanent deletion is not something I do. `rm -P`.

```backlog
id: B-013
repo: corelink-server
owner: owner
status: open
verify: manual
verify-means: local filesystem state, outside any repo
last-verified: 2026-08-23
```

### B-014 — confirm the leaked Stripe webhook secret was rotated

A `whsec_` value was exposed. Rotation must be confirmed in the Stripe dashboard.

```backlog
id: B-014
repo: corelink-server
owner: owner
status: open
verify: manual
verify-means: vendor dashboard state; not observable from any repo
last-verified: 2026-08-23
```
