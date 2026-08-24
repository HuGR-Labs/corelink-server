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
verify: manual
verify-means: |
  lives in corelink-runners; the Actions token cannot read a sibling repo, so this
  cannot be automated until [B-012] lands a cross-repo credential
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
verify: manual
verify-means: |
  lives in corelink-runners; not automatable until [B-012] lands a cross-repo
  credential. Open while the unqualified claim stands in that CHANGELOG.
last-verified: 2026-08-23
```

---

### B-025 — the runner image has no `/dev/shm`, so Bazel cannot sandbox

With the credential leak fixed (B-017), the Bazel starter's cold build reached
CoreLink and then died in Bazel's Linux sandbox:

```
I/O exception during sandboxed execution: [unix_jni.cc:382] /dev/shm (No such file or directory)
```

The `corelink` runner image does not provide `/dev/shm`. The example works
around it with `build:ci --spawn_strategy=local`, which is honest for a cache
demo — the action keys, uploads and hit ratio are identical either way — but it
means **every Bazel build on our own runners executes unsandboxed**, and any
customer-facing workload we run there inherits that. Hermeticity is exactly what
Bazel users buy.

The fix belongs in the runner image (`corelink-runners`), not in each example's
`.bazelrc`. Until it lands, the workaround stays and this item holds the debt.

```backlog
id: B-025
repo: corelink-runners
owner: tl
status: open
verify: |
  grep -q "spawn_strategy=local" examples/bazel-starter/.bazelrc
verify-means: |
  open while the example still needs the workaround; goes red once the runner
  image provides /dev/shm and the line is deleted
last-verified: 2026-08-24
```


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

`audit-chain-daily-verify` listed `corelink-audit-archive` under `audit/<date>/`.
That bucket did not exist and never had. The producer did —
`crates/corelink-audit-chain/src/archive_producer.rs`, writing exactly the key
shape the verifier looked for — but it was dead on two independent axes: its only
caller sits behind the Cargo feature `cf-billing-real`, which no build or deploy
path ever passes, and the crate that owns that caller is a wasm cdylib the live
Worker never imports. Commit `8ba0353b` added producer and verifier together;
nothing since ever wired the producer to a binding.

So this was not a broken cron. **Live tamper-evidence was D1-only** — a real
BLAKE3 hash chain with an Ed25519-signed head, sealed by `/_internal/audit/drain`
— with **no offsite, immutable copy**. 56,026 rows sealed, zero archived.

**Closed 2026-08-23.** `POST /_internal/audit/archive`
(`crates/corelink-container/src/routes/audit_archive.rs`) reads rows the drain
has already sealed, re-verifies every link from the persisted JCS bytes, and
writes NDJSON chunks to `corelink-audit-weur`, which already carries the 7-year
Object Lock rule `corelink-audit-7y-retention`. It is a SEPARATE endpoint from
the drain on purpose — an R2 outage must never be able to abort or corrupt a D1
seal — and that separation is also what lets it walk backward over the 56,026-row
backlog through its ordinary hourly sweep. Three things had to change beyond the
wiring: the archive line format now carries `canonical_jcs` verbatim (the live
chain's generic CloudEvents rows never deserialized into the crate's typed
`AuditEvent`, so the verifier binary could not have verified a real archive); the
chunk key now carries `(tenant_id, region)` (the documented shape collided
between partitions, which under a create-if-absent writer would have silently
dropped a chunk); and both verifier workflows now default to the bucket that is
actually written to. Pairs with [B-009], the retention half of the same gap,
which the chosen bucket already satisfies.

Remaining after this: the archive proves the chain is intact against ITSELF. An
external anchor (Rekor) is still roadmap, and the ≤1h pre-seal window in
`audit_outbox` is still unprotected — both pre-existing and out of this item.

**Proven in production 2026-08-24.** The first hourly tick after the container
roll archived real rows: `archived_at` moved from 0 to 3,456 of 56,266 sealed
rows, and ten NDJSON chunks are readable in `corelink-audit-weur` under
`audit/<Y>/<M>/<D>/<tenant>/<region>/<seq>.ndjson`. Every one of the ten was
pulled back out of the bucket and fed to the `verifier` binary built from this
commit: `AUDIT_CHAIN_VERIFY_OK` on all ten, 1,712 events total, no chain break.
The backlog drains on its own from here, one hourly sweep at a time. One
cosmetic artifact of real data: a probe row carrying `enqueued_at_ms: 1` lands
under `audit/1970/01/01/…`, which is the key shape faithfully encoding a
1970 timestamp, not a bug.

```backlog
id: B-015
repo: corelink-server
owner: tl
status: done
verify: "grep -q 'audit/archive' crates/corelink-container/src/main.rs && grep -q 'corelink-audit-weur' .github/workflows/audit-chain-daily-verify.yml"
verify-means: done while the archive route is MOUNTED in the container composition root and the daily verifier reads the same bucket the archiver writes
last-verified: 2026-08-24
```

### B-010 — the TLS floor change is recorded nowhere in the repo

The `humangr.com` zone's `min_tls_version` was lowered 1.3 to 1.2 because
1.3-only was blocking sccache clients. Live and correct, but it exists only as a
manual API change with no ADR and no IaC record.

```backlog
id: B-010
repo: corelink-server
owner: tl
status: done
verify: "ls specs/03_architecture/adrs/ | grep -qi tls"
verify-means: |
  done — ADR-0072 landed in #1232, with an OKF concept grounding it. Red if the ADR
  is ever removed. NOTE: this check first pointed at
  docs/design/, which holds informal planning docs — the numbered ADR series lives
  in specs/03_architecture/adrs/. A check aimed at the wrong directory would have
  stayed green forever after the ADR landed, which is a false negative in the gate
  itself, not in the item.
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

**Closed 2026-08-24.** The fix was not "add cdylib" — it was to attest what
ships. `wrangler.toml` deploys `worker/src/index.ts`; `corelink-worker` survives
as an ordinary Rust library linked into the container, and nothing wasm exists
anywhere in the release. What users download and execute is the `corelink` CLI,
four platform binaries on the GitHub release — which GA-GATE-E09 already names,
and which had neither a reproducibility check nor provenance.

`reproducible-build` now builds `corelink-cli` with `release-cli.yml`'s own
command, `SOURCE_DATE_EPOCH` and `--remap-path-prefix` set, twice, into separate
target directories so the second leg is a real compile rather than a cache hit.
Measured, not asserted — run 32726344224 on the self-hosted fleet:
**bit-identical**, `58cc00a621dc095160bd54ecebee00640f8aa5a9e8f4acb7c262117aadc8c216`,
4 691 516 bytes, 0 differing bytes. That is the first successful reproducibility
measurement this repo has ever produced. Both legs run on one host, so the claim
is build determinism, not cross-environment reproducibility, and the workflow
says so.

`release-slsa3` takes its subjects from the published release binaries instead
of rebuilding a second artifact, so the provenance describes the bytes a user
downloaded. Verified against the live `cli-v0.1.0` release: the four digests
computed that way are byte-identical to the release's own `checksums.txt`. Job 3
re-hashes after signing, so an asset swapped mid-flight cannot ship under a
valid-looking bundle. The one thing still blocked is the hosted SLSA builder —
tracked as B-031.

Also fixed in passing: the diff-threshold comparison truncated the percentage to
an integer, so a 5.9 % diff passed a 5 % gate.

```backlog
id: B-016
repo: corelink-server
owner: tl
status: done
verify: |
  grep -q "cargo build .*corelink-cli\|-p corelink-cli" .github/workflows/reproducible-build.yml && \
  ! grep -qE "^[^#]*--target wasm32-unknown-unknown" .github/workflows/release-slsa3.yml
verify-means: |
  done — reproducible-build compiles the shipped CLI, and release-slsa3 no longer
  BUILDS the wasm artifact (its header still describes the old bug on purpose, so
  the check ignores comments). Red if either is pointed back at the wasm target.
last-verified: 2026-08-24
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

**Closed 2026-08-24.** The zero-hit reading was a dead parser, not a dead
cache: the job read `--execution_log_json_file` line by line looking for
`remoteCacheHit`, and that file is pretty-printed JSON objects carrying
`cacheHit`. Every line failed to parse, so the total was always zero. The same
parser had been copied into `README.md` and `scripts/benchmark.sh`.

Running the workflow for real then surfaced two breaks the disabled gate had
hidden: `.bazelrc` registered an **unscoped** `--credential_helper`, so every
build sent the tenant PAT to `bcr.bazel.build` (which answered 401, killing the
build before it reached CoreLink); and `N4` matched the comment explaining the
rule it enforces. Both fixed, with `N5` added to keep the helper host-scoped.

Proven, not asserted — run 32718680738 on the `corelink` runner against prod:
`3 remote cache hit`, ratio `3/3 = 100.0%`, warm 9 523 ms vs cold 21 001 ms.
First green run this workflow has ever had. It is `active` again and its waiver
is removed.

```backlog
id: B-017
repo: corelink-server
owner: tl
status: done
verify: |
  test "$(gh api repos/HuGR-Labs/corelink-server/actions/workflows/bazel-starter-ci.yml -q .state)" = "active"
verify-means: |
  done — the workflow is enabled and gating again. Red the moment it is disabled
  once more, which is the state that let it rot unnoticed for a year.
last-verified: 2026-08-24
```

### B-018 — the public install one-liner has never worked end to end

**Corrected 2026-08-23.** I first reported this as "`corelink --version` demands a
PAT". That was the symptom visible in the CI log, not the truth — `--version`
never actually ran.

The installer at `apps/get-corelink-worker/src/install.ts:183-187` writes
`~/.corelink/config.toml` as a flat `token = "..."`. The CLI
(`tools/cli/src/config.rs`, `auth.rs:47`) only ever reads the PAT from an
`[auth]` table's `pat` key. TOML deserialisation **silently ignores** the
unrecognised top-level scalar, so `auth.pat` stays `None` for any PAT, however
well-formed. The script's own closing `corelink whoami` then fails with "No PAT
found", and under `set -eu` that aborts the whole `curl | sh` pipeline non-zero —
swallowing everything chained after it, including the `--version` call that
appeared in the log but never executed.

So **every** real install via the public one-liner dies looking like a total
install failure, when in fact the binary installed correctly. The config-write
schema bug is day-one (`77927c6e`); it only started surfacing as "No PAT found"
on 2026-08-02 (`#942`), when the closing verb changed from a `ping` subcommand
the CLI has never had, to `whoami`, which exists. Before that it failed with a
different error. The one-liner has arguably never worked.

Not a hard lockout — `corelink config set auth.pat` works standalone, so a user
who knows to do that can recover. It is a first-impression failure on the very
first thing a new user does.

Fixed in `#1233`, with regression tests that parse the real Rust struct fields
rather than restating the expected shape.

```backlog
id: B-018
repo: corelink-server
owner: tl
status: done
verify: "grep -qE '^\\[auth\\]$' apps/get-corelink-worker/src/install.ts"
verify-means: |
  done — the installer emits an [auth] table, which is what the CLI reads. Red if
  that table is ever dropped. NOTE: this check first grepped for the old flat
  `token = ` key and reported the item still OPEN after the fix landed — because it
  matched a COMMENT describing the old bug, not code. A check that passes for the
  wrong reason is the same defect this gate exists to catch, so it now asserts the
  presence of the correct shape rather than the absence of the wrong one.
last-verified: 2026-08-23
```

### B-019 — the security model asserts a control that is not in force

`specs/03_architecture/security_model.md:254` lists **CTRL-CRYPTO-001 — "TLS 1.3
only"** as a live control, with quarterly review and SSL Labs A+ as its evidence.
The zone has been on **TLS 1.2** since sccache clients were found to be rejected
by a 1.3-only floor. Three S-02 sprint documents repeat the same claim.

This one matters beyond tidiness: a security model is what an auditor or a
customer's security review reads, and it currently describes a control that is
not in force. The change itself was correct and is now recorded in ADR-0072; what
is missing is that the control table was never updated to match.

**Closed 2026-08-24.** Measured first, then corrected: the `humangr.com` zone
answers `min_tls_version = 1.2`, and `corelink-api.humangr.com` completes a real
TLS 1.2 handshake (`ECDHE-ECDSA-CHACHA20-POLY1305`). The claim was false in 25
places across 13 compliance documents plus the architecture set — every
ISO/SOC 2/GDPR/LGPD/FedRAMP crosswalk citing CTRL-CRYPTO-001, both
INV-CONF-IN-FLIGHT rows in the invariant registry, the compliance matrix, and
`_sprint_creation_contract.md`, which was still ordering every future sprint to
refuse TLS < 1.3. Live documents were corrected in place; the two FROZEN/AUDITED
ones (`SOC2-GAP-ANALYSIS`, `DRATA-INTEGRATION-COVERAGE`) keep their audited
bodies and carry a dated errata instead; the S-02 sprint records keep their
history with a superseded marker.

The evidence line was corrected too rather than quietly reused: the SSL Labs A+
scan cited by CTRL-CRYPTO-001 predates the floor change, so the control now says
so instead of implying the scan covers today's configuration.

`scripts/check_tls_floor.py` + `.github/workflows/tls-floor-drift.yml` close the
gap ADR-0072 named but could not fix: the floor lives in a Cloudflare dashboard
setting, so nothing in this repo could see it drift. The check asserts
**equality** with the documented value — a raise back to 1.3 breaks sccache
again, a drop below 1.2 makes the compliance documents overstate the control —
and exits non-zero when it cannot authenticate, because a drift check that
cannot read the value must never report "no drift".

```backlog
id: B-019
repo: corelink-server
owner: tl
status: done
verify: |
  ! grep -q 'TLS 1.3 only' specs/03_architecture/security_model.md
verify-means: |
  done — the control table no longer claims a 1.3-only floor. Red if the claim
  is reintroduced without the zone actually being raised (and if the zone IS
  raised, tls-floor-drift.yml goes red first).
last-verified: 2026-08-24
```

### B-020 — a cron sweep that cannot authenticate logs nothing at all

Every hourly sweep in `apps/signup-worker/src/index.ts` resolves a credential
first and returns `skipped: true` when it is unbound — and the caller only logs
when `skipped` is false. So an archive, drain, DSR-verify or PAT-scrub sweep that
never ran because its key went missing is indistinguishable in the logs from an
hour with nothing to do. Four sweeps share the shape
(`audit_archive_cron.ts`, `audit_drain_cron.ts`, `dsr_verify_cron.ts`,
`pat_scrub_cron.ts`).

Found while proving [B-015]: the archive produced nothing for two ticks and the
absence of a log line could not distinguish "not deployed yet" from "no key".
The fix is small — log the skip — but the class is the one that keeps costing
us: silence read as success.

**Closed 2026-08-24.** Each of the four sweeps now emits
`console.warn("[<sweep>] skipped=true reason=<credential>-unbound")`, naming the
credential that was missing. A fifth case was found while fixing it: the
DSR sweep is wrapped in `if (db)`, so an unbound `CONFIG_DB` skipped it without
even reaching the sweep function — that branch now warns too.

```backlog
id: B-020
repo: corelink-server
owner: tl
status: done
verify: "test \"$(grep -c 'skipped=true' apps/signup-worker/src/index.ts)\" -ge 5"
verify-means: |
  done while every sweep in scheduled() — the four sweep results plus the
  CONFIG_DB guard — names its missing credential instead of returning silently.
last-verified: 2026-08-24
```

### B-021 — nothing notices if the audit archive stops advancing

`audit-chain-daily-verify` proves that the chunks in `corelink-audit-weur`
verify. It does not prove that everything sealed in D1 reached the bucket. A day
with no chunks is a clean no-op (`AUDIT_CHAIN_VERIFY_OK: no input paths`,
confirmed by running the verifier with no arguments), which is correct for a
quiet day and indistinguishable from an archiver that died.

So the offsite copy has a monitor for corruption and none for absence. If the
erase key is rotated, the route stops mounting, or the sweep starts 500ing on
one partition, `archived_at` simply stops advancing and the first symptom is a
compliance question nobody can answer. Since B-020 the skip case at least warns
in the Worker log, but no gate reads that log.

What is missing is a lag check: `COUNT(*) WHERE emitted_at IS NOT NULL AND
archived_at IS NULL` compared against a threshold, paging when the unarchived
tail stops shrinking. It cannot be armed at a fixed threshold today — the
56k-row historical backlog is still draining through the hourly sweep, so any
useful threshold has to wait for convergence or be expressed as "not shrinking"
rather than "not zero".

**Closed 2026-08-24.** `.github/workflows/audit-archive-lag.yml` is the absence
monitor, hourly at :20 so it samples after the archive tick rather than racing
it. The predicate is a two-clause conjunction with T = 3h — *there exists a
sealed row unarchived longer than T* **AND** *`MAX(archived_at)` is older than T
or NULL* — which is correct in all three regimes without any stored trend state:
clause 1 alone would fire throughout the healthy backlog drain, clause 2 alone
would fire in a genuinely quiet period. Validated against PRODUCTION before
merge: clause 1 true (29,843 pending old rows), clause 2 false (the archiver had
written within the window), verdict no-page — the correct answer.

Writing it also surfaced that `audit-chain-daily-verify.yml` had been sending
every chain-break page with `runbook: specs/_runbooks/RB-AUDIT-CHAIN-VERIFY.md`,
a file that never existed. The spec gate caught it the moment the new runbook
cited it by name. That runbook now exists.

```backlog
id: B-021
repo: corelink-server
owner: tl
status: done
verify: "grep -q 'archived_at IS NULL' .github/workflows/audit-archive-lag.yml && grep -q 'MAX(archived_at)' .github/workflows/audit-archive-lag.yml && test -f specs/_runbooks/RB-AUDIT-ARCHIVE-ABSENT.md"
verify-means: |
  done while the absence monitor still evaluates BOTH clauses of the predicate
  and its runbook exists. A monitor reduced to one clause is a false-alarm
  generator, not a monitor.
last-verified: 2026-08-24
```

### B-022 — a partially-failing archiver pages nobody

`POST /_internal/audit/archive` returns 500 when `partitions_failed > 0`, and
the hourly sweep logs the status. Nothing pages on it. The B-021 absence monitor
does not catch this case either, and deliberately so: its second clause asks
whether the archiver wrote ANYTHING recently, so one broken partition among many
healthy ones leaves the predicate false. Named in
`specs/_runbooks/RB-AUDIT-ARCHIVE-ABSENT.md` §3.3 rather than left silent.

The gap is narrow but real: a single tenant/region partition could fail every
hour indefinitely while the fleet looks healthy. Closing it needs either a
per-partition lag query or a page driven off the sweep's own non-200.

```backlog
id: B-022
repo: corelink-server
owner: tl
status: open
verify: "! grep -q 'partitions_failed' .github/workflows/audit-archive-lag.yml"
verify-means: |
  open while no scheduled check looks at per-partition archive failure. Closes
  when a partial failure can raise a page on its own.
last-verified: 2026-08-24
```

### B-023 — the two repos carry different pre-merge gates

`scripts/pre-merge-gate-check.sh` is 456 lines in corelink-server and 156 in
corelink-runners. The runners copy was ported by its PR #482 from an earlier
revision and lacks the `--merge` mode, whose whole purpose is that a human
cannot accidentally discard the gate's verdict by piping it — the failure that
merged PR #1049 with four checks still pending.

So the repo where a bad merge rolls a container image onto customer jobs has the
weaker gate.

```backlog
id: B-023
repo: corelink-runners
owner: tl
status: open
verify: "test \"$(wc -l < ../corelink-runners/scripts/pre-merge-gate-check.sh 2>/dev/null || echo 0)\" -lt 400"
verify-means: |
  open while the runners copy of the gate is materially shorter than the
  server's, i.e. missing the later defenses. Path is relative to a sibling
  checkout; a missing checkout reads as still-open, which is the safe direction.
last-verified: 2026-08-24
```

### B-026 — eight audit-chain partitions forked on 2026-08-14 and cannot be archived

The archive built for [B-015] refused to write eight partitions, and the reason
is a real integrity defect it inherited rather than caused. `_public`/`wnam` sat
at exactly 131 unarchived rows for eight consecutive hourly ticks while
`enam` drained 29,832 to 13,097. Pulling that partition out of D1 and running
the verifier against it gives:

```
AUDIT_CHAIN_BREAK_DETECTED: tenant=_public: region=wnam:
  sealed archive chunk sequence gap: expected 11, found 10
```

Six sequence numbers (10, 13, 20, 30, 32, 37) are each held by TWO rows, and the
six numbers immediately after them (12, 14, 21, 31, 33, 38) are missing. Each
pair is the `read.attempted` / `read.served` pair of one request, and the two
rows carry DIFFERENT `prev_hash` values — so the chain did not merely duplicate a
label, it FORKED into two branches at each of those points.

**Scope, measured:** 8 of 360 partitions, 1,505 excess rows, 3,010 rows sitting
on a duplicated sequence number. The largest affected partition is
`00000000-0000-4000-8000-0000000f0005`/`enam` with 1,466 of them.

**It is historical and bounded.** Every one of those 3,010 rows was sealed
inside a single 17-minute window, 2026-08-14 01:43:40 → 02:00:50 UTC. The drain
on `origin/main` today carries explicit fork-freedom — it reads the sealed tail,
computes deterministically, and aborts the partition when the head drifted under
it rather than forking (`crates/corelink-container/src/routes/audit_drain.rs`,
the module doc at lines 30-35 and the drift path at 469). So this is the scar of
one past incident, not an open wound, and nothing here suggests tampering: a
tamperer does not helpfully leave both branches behind.

**Why nothing noticed for ten days.** The daily verifier only examines chunks
that reached the bucket, and these never did. The B-021 absence monitor cannot
see it either — its second clause asks whether the archiver wrote ANYTHING
recently, and the healthy partitions keep it false. That is exactly [B-022],
now with a live instance instead of a hypothesis.

**The open decision is remediation, and it is not mine.** Re-sequencing sealed
rows would rewrite the very evidence the chain exists to protect. The plausible
options — archive the clean prefix up to the first fork and quarantine the
remainder; archive both branches with an explicit fork marker; or accept the
partitions as permanently unarchivable and record why — differ in what they
claim to an auditor, so the owner picks.

**Decided (2026-08-24): clean prefix + quarantine.** The archiver now writes
each partition's longest contiguous VERIFYING PREFIX and marks the unarchivable
remainder `quarantined_at` + `quarantine_reason`
(`sequence_gap:expected=11,found=10`), so healthy rows reach R2 instead of being
held hostage by one historical fork. Re-sequencing stays FORBIDDEN — no chain
column is ever UPDATEd, enforced by a test that re-reads the writer's own source.
Migration `d1/0100_audit_outbox_quarantine.sql` adds the columns and a narrowed
work-queue index; the B-021 absence monitor excludes quarantined rows from its
pending clause (they can never clear it) while printing their census every hour;
`RB-AUDIT-ARCHIVE-ABSENT` §5 says what a quarantined row means and how to list
them. **Still open** until the migration is applied to prod D1 and the eight
partitions are observed quarantined with a recorded sequence range per partition.

**Closed 2026-08-24 — the decided policy is applied in prod and observed.**
Migration `0100` is live on `corelink-prod-d1` and the archiver's quarantine pass
fired at 15:00:08 UTC. It archived 45,836 rows and quarantined exactly the eight
forked partitions, 11,818 rows, each with its own recorded reason and sequence
range:

| partition | rows | seq range | reason |
|---|---|---|---|
| `…0f0005`/enam | 11,435 | 9234..20669 | `sequence_gap:expected=9235,found=9234` |
| `3c7d77b1…`/enam | 219 | 4988..5206 | `sequence_gap:expected=4989,found=4988` |
| `_public`/wnam | 120 | 10..130 | `sequence_gap:expected=11,found=10` |
| `bba0ff1d…`/enam | 15 | 1..14 | `chain_head_discontinuity:seq=1` |
| `e51607b0…`/enam | 13 | 0..13 | `sequence_gap:expected=1,found=0` |
| `ce42d194…`/enam | 12 | 1..13 | `sequence_gap:expected=2,found=1` |
| `8873dc37…`/enam | 3 | 0..3 | `sequence_gap:expected=1,found=0` |
| `dd35a645…`/enam | 1 | 0..0 | `sequence_gap:expected=1,found=0` |

Eight partitions, as measured on 2026-08-14 — no drift in the population, and no
row outside the original 17-minute window was ever involved. The remaining 90
unchained rows are ordinary new traffic awaiting the next drain.

One correction to what this item asserted. It claimed the fork "is not an open
wound" because today's drain "aborts the partition when the head drifted under it
rather than forking". Reading the drain again against this data, that is stronger
than the code earns — see [B-038]. The quarantine census above is unaffected
either way; what changes is whether recurrence is prevented or merely unobserved.

```backlog
id: B-026
repo: corelink-server
owner: tl
status: done
verify: |
  test "$(grep -c 'quarantined_at' migrations/d1/0100_audit_outbox_quarantine.sql)" -gt 0
verify-means: |
  done while the quarantine policy the owner decided is present in the applied
  migration. The prod-state half is a one-time observation, recorded in the table
  above rather than re-run: re-measure with SELECT tenant_id, region, COUNT(*),
  MIN(sequence_number), MAX(sequence_number), quarantine_reason FROM audit_outbox
  WHERE quarantined_at IS NOT NULL GROUP BY 1,2,6.
last-verified: 2026-08-24
```

### B-027 — the whole alerting layer runs nowhere and says it does

`dashboards/alerts/` holds ten Prometheus/Alertmanager rule files with
severities, PagerDuty routing labels and runbook pointers. **Nothing validates
them and nothing publishes them.** No workflow invokes `promtool`; no pipeline
ships the rules anywhere. They are a specification of alerting, not alerting.

Worse than inert: `dash-audit-export-alerts.yml` states in its own header that
`promtool test rules` runs "on PR via the existing `dashboards/alerts/*.yml`
validator". That validator does not exist. A file asserting its own gate is
exactly the shape that survives review.

Twenty runbook ids are cited from `runbook:` labels in those rules and
**five have no file**: `RB-AC-CONFORMANCE`, `RB-AC-COST-REGRESSION`,
`RB-AC-HIT-RATIO`, `RB-AC-LATENCY`, `RB-AC-SLO-BURN`.

> **Correction (2026-08-24).** This paragraph first claimed *fourteen*, adding
> the nine `RB-FM-*` ids (059/060/250/253/254/300/303/305/404). Those exist —
> in a SECOND runbook root, `specs/05_quality/runbooks/`, under slugged
> filenames (`RB-FM-059-do-quota-exceeded.md`). The original count came from
> resolving ids against filenames in one directory. All 124 runbooks declare a
> front-matter `id:`, so `scripts/validate_alert_runbook_labels.py` resolves by
> that and falls back to the filename stem. The overcount was mine, and it is
> exactly the failure this backlog exists to prevent: a number asserted from a
> partial scan and then quoted as fact. By contrast the three
runbook pointers in live PagerDuty payloads all resolve — the last dangling one
was written on 2026-08-24.

Two honest readings and the item covers both: either the layer is meant to be
live, in which case it needs a validator, a publish path and its runbooks; or it
is aspirational, in which case the files must say so instead of claiming a gate.

```backlog
id: B-027
repo: corelink-server
owner: tl
status: done
verify: "grep -rq 'promtool' .github/workflows/"
verify-means: |
  open while no workflow runs promtool over dashboards/alerts/. Closes when the
  rules are validated and published, or relabelled non-live with the false gate
  claim removed.

  CLOSED by PR #1264 on the second branch of that condition: `alerts-validate.yml`
  runs `promtool check rules` over all ten files on `corelink` (proven in CI —
  10 files, 125 rules, and proven able to fail: a corrupted `expr:` exits 1), a
  new `scripts/validate_alert_runbook_labels.py` asserts every `runbook:` label
  resolves, and FOUR files carrying a false "validated on PR" header were
  corrected — one more than this item knew about. Nothing publishes these rules
  to an Alertmanager, and the workflow header says so in as many words, so a
  green run cannot be misread as "these alerts are firing". Publishing was never
  part of this item's stated closure condition; if it is wanted, it needs its own
  item rather than holding this one open forever.
last-verified: 2026-08-24
```

### B-028 — six Dependabot alerts have never been triaged

`HuGR-Labs/corelink-server` carries **4 high and 2 moderate** open Dependabot
alerts. Every `git push` prints the banner; nobody has read them. The repo runs
`cargo-audit`, `cargo-deny`, `semgrep`, `trivy` and `gitleaks` on schedules, so
the gap is not tooling — GitHub's own advisory feed simply has no owner.

Triage, not blanket upgrade: each alert needs a verdict (fix / not-reachable /
accepted-with-reason). An unread high advisory on a product that sells storage
governance is a bad look independent of exploitability.

**Triaged 2026-08-24.** Three of the six — `ip-address` ≤ 10.3.0, reached via
`socks` → `proxy-agent` → `@puppeteer/browsers` — had a published patch and are
fixed in `#1259` by a `pnpm.overrides` entry lifting them to 10.5.0. The other
three have **no patched version at all**: `extract-zip` ≤ 2.0.1 (via
`@puppeteer/browsers`) and two `image-size` ≤ 2.0.2 advisories (via
`@docusaurus/mdx-loader`). Both are build-time-only paths; neither package
reaches a Worker or container bundle. They stay OPEN rather than dismissed,
because dismissing removes the reminder to take the patch when one lands — so
this item stays open too, and its verify keeps counting them.

```backlog
id: B-028
repo: corelink-server
owner: tl
status: open
verify: manual
verify-means: |
  open while any Dependabot alert is still open and unadjudicated. NOT
  CI-checkable: the Actions GITHUB_TOKEN cannot read the Dependabot alerts API,
  and a command that returns empty under CI's credentials would make this check
  pass by accident — the failure mode this register exists to prevent. Run it
  where gh is authenticated:
    gh api /repos/HuGR-Labs/corelink-server/dependabot/alerts --paginate \
      -q '[.[]|select(.state=="open")]|length'
last-verified: 2026-08-24
```

### B-033 — the workspace lint gate runs nowhere, and its stated compensation does not hold

`cargo clippy --workspace --all-targets -- -D warnings` appears in exactly one
workflow, `cas_foundation.yml`. That workflow was PARKED on 2026-08-10 (cost
hygiene, owner-authorized): its cron is commented out and it is dispatch-only.
Its last scheduled run was 2026-08-05 and it was CANCELLED; every scheduled run
before that, back through July, FAILED.

The park is documented and the reasoning is sound on cost. What does not hold is
the compensating argument written beside it — that "the per-crate PR lanes gate
the touched crates on every PR". Measured: **8 of 84 workspace members** have a
per-crate clippy lane (`corelink-server`, `-hash`, `-meta`, `-worker`, `-reapi`,
`-adapter-host`, `-client-verify`, `-tenant-path`). The other 76 are linted by
nothing.

This is not hypothetical. On 2026-08-24 an agent working in
`crates/corelink-audit-chain` found `cargo clippy -p corelink-audit-chain
--all-targets -- -D warnings` **already red on origin/main** — 33 errors, every
one in test code missing the workspace lint opt-out. It had been red for an
unknown period and no lane could have said so. `#1252` fixed that crate; the
hole that hid it is untouched.

The fix is not to unpark a heavy weekly lane. The candidates are a lint scoped
to the crates a PR's diff actually touches, or a workspace clippy on push-to-main
only. Choosing between them is a cost call and belongs in the same PR that
measures what each would cost on the fabric.

```backlog
id: B-033
repo: corelink-server
owner: tl
status: open
verify: |
  test "$(grep -rho 'cargo clippy --package [a-z0-9-]*' .github/workflows/*.yml \
    | awk '{print $4}' | sort -u | wc -l | tr -d ' ')" -lt 84
verify-means: |
  exits 0 while fewer workspace members have a per-crate clippy lane than exist
  in the workspace, i.e. while some crate is linted by nothing on a PR. Starts
  failing once coverage is closed — by per-crate lanes, or by whatever
  diff-scoped or push-to-main lane replaces the parked workspace one, in which
  case this verify is what must be rewritten to match the new mechanism.
last-verified: 2026-08-24
```

### B-029 — the load-test regression gate cannot fail

`load-test-nightly.yml` defines `REGRESSION_THRESHOLD = 1.20` and never uses it.
The baseline lookup is a comment — *"Baseline lookup intentionally elided here —
the GHA cache implementation lives in a follow-up WI"* — and the job prints
`advisory mode` and exits 0. The artifact download it does perform fetches the
CURRENT run's own artifacts, so there is nothing to compare against.

A green check that asserts nothing about performance, in a repo whose product
claim is speed.

```backlog
id: B-029
repo: corelink-server
owner: tl
status: open
verify: "grep -qE 'advisory mode|self-hosted, Linux, X64' .github/workflows/load-test-nightly.yml"
verify-means: |
  open while the regression check prints advisory mode instead of comparing
  against a stored baseline, OR while the workflow is pinned to a runner label
  no runner on the fleet holds. Closes when it can go red, or when it stops
  calling itself a regression gate.

  PARTIALLY CLOSED by PR #1263: the comparison is real now (proven both ways —
  exit 2 at +73.9% over a stored baseline, exit 0 at +15.0%, and a failing run
  does not publish a new baseline). What remains is that the workflow still
  cannot execute: both jobs are `runs-on: [self-hosted, Linux, X64]`, a label
  no runner carries, which is why its own header already says the nightly cron
  was removed. The FIRST predicate was widened rather than left to flip on its
  own — closing this item on "advisory mode is gone" would have replaced a gate
  that lied with a gate that is correct and never runs. See B-037.
last-verified: 2026-08-24
```

### B-030 — a Mac CI slot is down, and a stuck fleet is invisible

`corelink-builder-1` is registered in launchd but not running: GitHub lists four
`corelink-builder` runners, not five. Nobody noticed.

Separately, on 2026-08-24 all four live runners reported `busy: true` while
**zero** jobs were in progress across all three repos, with seven runs queued
behind them — one since 05:04 UTC, six hours earlier. A launchd restart plus
cancelling the orphaned queue cleared it, and a dispatched `rustfmt` then ran
green; the `busy: true` flag itself stayed stale on GitHub's side even with an
empty queue, so it is cosmetic rather than load-bearing.

What is missing is not the fix, it is the noticing. Nothing watches whether the
Mac fleet has its expected slot count or whether the queue is draining — and the
queue is the one signal that would have caught both.

```backlog
id: B-030
repo: corelink-server
owner: tl
status: open
verify: manual
verify-means: |
  open while fewer than five corelink-builder runners are registered, AND while
  nothing watches that count. NOT CI-checkable: listing self-hosted runners needs
  `administration: read`, which the Actions GITHUB_TOKEN does not have and cannot
  be granted — the same wall that forced the fleet-busy endpoint. Run it where gh
  is authenticated:
    gh api /repos/HuGR-Labs/corelink-server/actions/runners \
      -q '[.runners[]|select(.name|startswith("corelink-builder"))]|length'
  Closes when the fifth slot is back AND a gate watches the count, since the
  count is precisely what nobody was watching.
last-verified: 2026-08-24
```

### B-032 — every Critical vendor review is past its cadence window

All seven Critical vendors — Cloudflare, Stripe, Clerk, AWS, Google Cloud, Azure
and Drata — were last reviewed on the 2026-05-15 baseline against a quarterly
(90-day) cadence. On 2026-08-24 that is 101 days, 11 days overdue, and the
register's own header names 2026-08-15 as the next full refresh. The reviews
themselves need Drata and the vendors' current SOC 2 / ISO evidence, so this is
the human's.

It went unseen because the thing built to see it said the opposite. Section 3 of
the weekly digest is assembled *only* from vendors already in breach, yet it was
headed "Vendor risk SLAs" and its single verdict column read `2× breach?` — true
only once a review has **doubled** its window. Seven overdue Critical vendors
therefore rendered as seven `no`s. The renderer is fixed (`#1248`: explicit
OVERDUE count, a `Days overdue` column, the column renamed `> 2× cadence?`), so
the next digest states it plainly. Regression semantics were deliberately left
alone: 2× remains the §7 page-worthy trigger, and changing that is a compliance
decision, not a rendering one.

```backlog
id: B-032
repo: corelink-server
owner: owner
status: open
verify: |
  python3 scripts/compliance-weekly-digest.py --dry-run --json \
    | python3 -c "import json,sys; v=json.load(sys.stdin)['vendor_breaches']; \
      print('overdue=%d' % len(v)); sys.exit(0 if v else 1)"
verify-means: |
  exits 0 while at least one vendor review is past its cadence window, and starts
  failing the moment the register is refreshed — so finishing the reviews turns
  this item red until its status is updated to match. `--dry-run` keeps it from
  writing a digest file, so the check leaves no dirty tree.
last-verified: 2026-08-24
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
verify: manual
verify-means: |
  lives in corelink-runners; not automatable until [B-012] lands a cross-repo credential
last-verified: 2026-08-23
```

---

## Cache integrity

### B-024 — Turborepo cache entries stay overwritable, on purpose, without evidence

`specs/_audits/2026-08-23-cache-integrity-coverage.md` (F-2) found Turborepo
artifacts unverifiable, mutable and unpinned at once. Two of the three are now
closed: `R2KvStore` carries CoreLink's own BLAKE3 alongside each object and
serves a MISS on mismatch, and the audit's F-1 containment landed on the sccache
plane. Mutability is the part left open.

The obvious fix — make PUT create-only, as the Action Cache already is — was
**declined for now**, and that decision is the item. The AC can refuse an
overwrite because its protocol says a result for an action digest is final;
Turborepo makes no such promise, and nobody has measured whether real clients
re-PUT an existing key. This repo has already paid once for guessing at a build
tool's behaviour: rejecting `.sccache_check` made sccache deem the backend
unusable and disable itself entirely.

So the missing thing is not a patch, it is evidence: drive a real `turbo` client
through a re-run of an already-cached task against our endpoint and record
whether a PUT is re-issued for a key that exists, and what the client does with a
refusal. Then decide.

Residual until then: a credential with cache-write scope for a tenant can replace
the bytes behind that tenant's own Turborepo keys. Not cross-tenant; the envelope
does not detect it, because the key is not a preimage of the content.

```backlog
id: B-024
repo: corelink-server
owner: tl
status: open
verify: |
  ! grep -q "create_only\|put_if_absent" crates/corelink-container/src/routes/turbo_v8.rs
verify-means: open while Turborepo PUT stays an overwrite; lands red once create-only semantics appear on that surface
last-verified: 2026-08-23
```

### B-031 — SLSA L3 needs a GitHub-hosted builder, which this repo does not spend on

`release-slsa3.yml` now takes its subjects from the published release binaries
(B-016), so job 1 and job 3 run on the self-hosted fleet and are correct. Job 2
is the `slsa-framework/slsa-github-generator` reusable workflow, and **SLSA L3
is defined by that builder being GitHub-managed and isolated** — it cannot be
re-homed onto our own runners without dropping to L2 or below. This repo's
standing rule is zero GitHub-Actions spend.

So the lane is correct and still cannot complete for free. It has run once ever
(2026-05-29) and failed, and only one release exists (`cli-v0.1.0`), so no
provenance bundle has ever been produced for anything we ship.

The decision is a cost one and belongs to the owner: (a) allow hosted minutes
for release provenance only — releases are rare, so the bill is bounded and
small; (b) drop the L3 claim and self-host a weaker attestation (cosign-signed
digests, no isolated builder), stating the level honestly wherever SLSA L3 is
currently asserted; or (c) withdraw the provenance claim entirely. Option (b)
and (c) both require editing the ISO 27001 SoA A.5.21 row and the SOC 2
crosswalk, which cite SLSA L3 today.

```backlog
id: B-031
repo: corelink-server
owner: owner
status: open
verify: |
  grep -q "slsa-github-generator" .github/workflows/release-slsa3.yml
verify-means: |
  open while the lane still depends on the hosted SLSA builder; goes red once
  the dependency is removed, whichever way the decision lands
last-verified: 2026-08-24
```

### B-035 — eight contract lines promise a TLS floor we do not enforce

The DPA (`legal/dpa/v1.0.0.{en-US,pt-BR,es-419}.md`), the EU SCC annex, the
sub-processor commitments and the three privacy notices all state encryption in
transit as **"TLS 1.3+"** or **"(TLS 1.3)"**. The `humangr.com` edge floor is
**1.2** (ADR-0072), so those lines commit us contractually to a control we do
not enforce. Everything editorial — docs site, questionnaires, legal templates,
compliance crosswalks — was corrected in the same sweep; these eight were not,
deliberately.

They are **versioned contract text**. Correcting `v1.0.0` in place would rewrite
a document a customer may have accepted; the honest paths are (a) publish
`v1.0.1` with the corrected clause and, if anyone has executed v1.0.0, give
notice, or (b) raise the zone floor back to 1.3 and accept that `sccache` and
every other `native-tls`/SecureTransport client stops working (ADR-0072's exit
condition). It is a legal and product call, not an editorial one.

`scripts/validate_docs_reality.py` now scans `legal/` and holds these eight
lines as **tracked** drift: visible on every run, fatal under `--strict`, and
impossible to forget. New occurrences anywhere else fail the gate outright.

```backlog
id: B-035
repo: corelink-server
owner: owner
status: open
verify: |
  grep -q "tls-13-floor-claim-executed-contracts" scripts/docs_reality_allowlist.json
verify-means: |
  open while the contract text still needs the tracked suppression; goes red the
  moment the clause is fixed (or the floor raised) and the rule is deleted
last-verified: 2026-08-24
```

### B-034 — docs CI has six hosted-runner jobs and 2 116 broken links

Fixing the three tests that had docs CI failing early (see the `fix(docs)`
commit of 2026-08-24) let the rest of the pipeline run for the first time in a
while, and it surfaced two things the early failure had been masking.

**2 116 broken links.** `lychee` reports them across the built site, including
`https://corelink-api.humangr.com/` returning 404 from the API reference page.
The count is not a regression from any recent change; it is what the site has
been carrying.

**Six jobs on `ubuntu-latest`.** `docs-ci.yml` runs lychee, axe-core (×3),
lighthouse and the a11y baseline on hosted runners, against this repo's standing
rule of zero GitHub-Actions spend. They are also the jobs that never ran while
the early failure short-circuited the workflow, so the spend was invisible.

Both need a decision before this pipeline can be called green: re-home the six
jobs onto `corelink` (axe/lighthouse need a browser — check the image), and
either fix or scope the link check, because a gate reporting 2 116 failures
gates nothing.

**Link half closed by PR #1261 (2026-08-24).** Down to zero non-GitHub errors,
proven by running lychee locally against the real build with the CI's own
arguments. Real rot was repaired, not hidden: a dead GitHub org path in 43 files
including every i18n mirror, the moved BLAKE3 paper, the HubSpot security page,
the ANPD petition URL, and every documented CLI install recipe (see B-036).
Exclusions were added only for category errors and for dead hosts that already
carry a dated suppression in `hostname.tracked_dead`, each annotated in
`apps/docs/lychee.toml` with why it is not a link. The `verify` above is
deliberately unchanged: it tracks the hosted-runner half, which is still open.

That work also surfaced B-037 — the reason the link count looked survivable is
that the gate runs authenticated.

```backlog
id: B-034
repo: corelink-server
owner: tl
status: open
verify: |
  grep -c "runs-on: ubuntu-latest" .github/workflows/docs-ci.yml | grep -qv '^0$'
verify-means: |
  open while docs-ci still schedules hosted jobs; goes red once every job in that
  workflow runs on the self-hosted fleet
last-verified: 2026-08-24
```

### B-036 — every documented CLI install recipe was fiction

The installation tutorial and the 10-minute quickstart offered four
"alternative install paths" for readers who cannot pipe curl to a shell. In all
four locales, every one of them was wrong: `brew install
HumanGuardrail/tap/corelink` (neither `HumanGuardrail/homebrew-tap` nor
`HuGR-Labs/homebrew-tap` exists — both 404), `winget install
HumanGuardrail.corelink` (never published), and two "release tarball" recipes
naming a repo that does not exist (`HumanGuardrail/corelink-cli`), a format that
is not published (`.tar.gz`; the assets are raw binaries) and an architecture
spelling the release does not use (`arm64` vs `aarch64`).

The primary `curl -fsSL https://corelink-get.humangr.com | sh` path was correct
throughout, and its installer already normalises uname's `arm64` to the
published `aarch64` — with a comment explaining that this exact mismatch once
broke every Apple Silicon Mac. That is why this survived: the working path is
the one everybody tests.

Fixed in PR #1261 against the real assets on `HuGR-Labs/corelink-cli`, each
verified 200 anonymously, with the published `.sha256` checked before the binary
is made executable. The security policy's in-scope list, which named the
nonexistent Homebrew formula as a distribution surface a researcher could probe,
was corrected in the same change.

Left open deliberately: nothing gates this. No check installs the CLI the way a
reader would, so the next rename or release-format change reintroduces it
silently.

```backlog
id: B-036
repo: corelink-server
owner: tl
status: open
verify: |
  ! grep -rq 'HuGR-Labs/corelink-cli/releases' .github/workflows/
verify-means: |
  open while no workflow fetches the release assets the MANUAL recipes name.
  The primary `curl | sh` path is already exercised (e2e-prod.yml,
  release-cli.yml) — which is precisely why only the manual recipes rotted.
  Closes when a check downloads the documented assets by name and verifies the
  published checksum, the same way the tutorial tells a reader to.
last-verified: 2026-08-24
```

### B-037 — two gates cannot see what they claim to check

Two independent instances of the same shape, both found on 2026-08-24.

**lychee runs authenticated.** The docs link checker uses a GitHub token, so a
link into a PRIVATE repo resolves for CI and 404s for every actual reader. Run
the identical command locally without a token — the customer's view — and the
same build yields **2 589 GitHub errors that CI reports as OK**. Nearly all are
the trust-centre and compliance pages citing evidence files under
`HumanGuardrail/corelink-server`, i.e. the site invites a prospect to read
audits they cannot open. Two possible resolutions and the item covers both:
publish what the trust pages cite, or stop citing what cannot be published. What
is NOT acceptable is the current state, where the gate is green because it holds
a credential the reader does not.

**Six workflows are pinned to a label no runner holds.** `runs-on:
[self-hosted, Linux, X64]` appears in six workflow files; the fleet has
`corelink` (Firecracker/Linux) and `[self-hosted, mac, corelink-builder]`, and
nothing carries that triple. Those workflows cannot execute. `load-test-nightly`
documents this in its own header and has had its cron removed; the other five
have not been checked. A workflow that cannot be scheduled is indistinguishable
from one that passes, in every view that matters.

```backlog
id: B-037
repo: corelink-server
owner: tl
status: open
verify: |
  test "$(grep -rl 'self-hosted, Linux, X64' .github/workflows/*.yml | wc -l | tr -d ' ')" -gt 0
verify-means: |
  open while any workflow targets a runner label the fleet does not provide.
  Covers only the second half; the authenticated-lychee half has no mechanical
  predicate yet, which is itself the point — write one when the trust-page
  decision lands.
last-verified: 2026-08-24
```

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

### B-013 — three private keys sitting in ~/Downloads

Re-checked 2026-08-23, still present:

```
corelink-app.pk8.pem                              1704 B  2026-06-15
corelink-runners-fleet.2026-06-15.private-key.pem 1675 B  2026-06-15
corelink-runners.2026-07-13.private-key.pem       1679 B  2026-07-13
```

A fourth file, `githugr-clerk-pubkey.pem`, is a **public** key and harmless — it
is listed here only so nobody deletes three and calls it four.

These are GitHub App private keys. Secure deletion is genuinely the owner's: I do
not permanently delete data. `rm -P` overwrites before unlinking.

```backlog
id: B-013
repo: corelink-server
owner: owner
status: open
verify: manual
verify-means: local filesystem state, outside any repo
last-verified: 2026-08-23
```

### B-014 — the leaked Stripe webhook secret was already verified harmless

**Closed 2026-08-23 on re-check, and it was never the owner's to do.** This item
was seeded from a stale index line reading "leaked Stripe whsec — OWNER confirm
rotation". The check had already been done on 2026-07-11: the leaked value was
compared against the live deployed secret and they differ, and the leaked one
belonged to endpoint `we_1Tca…`, which no longer exists. Confirmed again today —
the account has three live webhook endpoints (`we_1Toli…` signup 2026-07-02,
`we_1Tfh8…` api 2026-06-07, and the wallet endpoint `we_1TJ1Y…` which is
`disabled`), and the leaked endpoint is not among them.

So there is nothing to rotate: the live secret was never the leaked one. The only
residue is a dead value in git history, and a history purge was already rejected
because it would orphan the OKF wiki's blob anchors.

Kept as a closed item rather than deleted, per the density rule — and as a record
of the re-check, since "owner must confirm" survived in my notes for six weeks
after it had already been settled.

```backlog
id: B-014
repo: corelink-server
owner: tl
status: done
verify: |
  test "$(curl -s "https://api.stripe.com/v1/webhook_endpoints?limit=20" \
    -u "$(grep -m1 '^STRIPE_LIVE_SECRET_KEY=' .env.local | cut -d= -f2-):" \
    | grep -c 'we_1Tca')" = "0"
verify-means: |
  done while the endpoint the leaked secret belonged to stays absent from the live
  account. Requires .env.local, so it only runs locally — the CI gate treats a
  missing file as a failed check, which is the correct direction.
last-verified: 2026-08-23
```

### B-038 — the drain's drift path rests on a byte-identity claim the code does not guarantee

`archive_partition`'s sibling in the drain seals rows FIRST and only then runs the
compare-and-set on `audit_chain_head`
(`crates/corelink-container/src/routes/audit_drain.rs`, the seal loop then
`advance_head_cas`). The CAS is correct in isolation: a drain that loses it does
not advance the head. But by then it has already written its own `sequence_number`,
`prev_hash` and `chain_hash` onto real rows, and the drift branch does not undo
them. It justifies that with a comment:

> Our sealed rows are byte-identical to that drain's (deterministic), so they are safe

That holds only if both drains sealed the SAME rows in the SAME order from the SAME
head. Nothing enforces it. `read_pending_rows` returns whatever is pending at the
moment it runs, so two drains that overlap read different sets. The prod data from
[B-026] shows exactly that outcome: partition `…0f0005`/enam has TWO rows at
sequence 9234 — one `corelink.cas.write.attempted` sealed at 01:43:40Z with
`prev_hash` equal to 9233's `chain_hash`, and one `corelink.cas.write.committed`
sealed at 02:00:50Z with a `prev_hash` (`a4c578…`) that appears nowhere in the
table as any row's `chain_hash`. Not byte-identical; a second branch.

Two consequences beyond the stall. The branch that WON the CAS is the one the head
still follows (`next_sequence` 20670 today), while the branch the archiver reached
first is the one now sealed into R2 — so the archive holds a row that is not on the
canonical chain. And the 17-minute spacing is the clue to the trigger: the seal
loop writes one row per D1 round trip, so a large partition can still be writing
when the next cron tick starts. Nothing serialises the two.

This item is about recurrence, not repair. The eight historical partitions are
already quarantined and closed under [B-026]; what is unproven is that it cannot
happen again. The fix directions are a partition lease so two drains cannot overlap,
or moving the seal after the CAS so a loser writes nothing. Either is a real change
to the integrity path and wants its own design pass, not a patch.

```backlog
id: B-038
repo: corelink-server
owner: tl
status: open
verify: |
  grep -q "byte-identical to that drain" crates/corelink-container/src/routes/audit_drain.rs
verify-means: |
  open while the drift path still justifies its no-op with the byte-identity claim.
  Goes red once the drain either serialises partitions or seals after the CAS, at
  which point the comment and the assumption both go away.
last-verified: 2026-08-24
```
