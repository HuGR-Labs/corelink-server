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

**Reconciliation landed observe-only 2026-08-25** (corelink-runners #507,
`deploy/cloudflare/src/index.ts` `reconcileOrphanBoxes`). It is the mirror image
of the sbox-first reaper: it enumerates the ACTUAL running instances from
Cloudflare — per-app, because the account-level instances endpoint is dead —
cross-references each against the `sbox:` records, and flags any instance with no
record AND age > 2× the lease TTL as an orphan candidate. It **logs** candidates
and bumps `orphan_box_detected`; there is NO destroy/stop/teardown on this path.
Two default-off flags: `RECONCILE_ORPHAN_BOXES` (the observe sweep) and a
separate, owner-gated `RECONCILE_ORPHAN_TEARDOWN` (declared for Env stability,
NOT wired here). Enabling teardown is [B-044] — the same ship-inert-then-flip
discipline as [B-038]/[B-043].

```backlog
id: B-002
repo: corelink-runners
owner: tl
status: done
verify: manual
verify-means: |
  done — the platform-truth reconciliation exists (corelink-runners #507,
  `reconcileOrphanBoxes` in deploy/cloudflare/src/index.ts): it no longer starts
  from our own bookkeeping, it starts from the Cloudflare instance list and
  reaps-by-omission against sbox. Observe-only by design; arming teardown is
  B-044. MANUAL because the code is in the SEPARATE corelink-runners repo — the
  corelink-server backlog gate cannot grep a path that does not exist here. Check
  in that repo: `grep -q "reconcileOrphanBoxes" deploy/cloudflare/src/index.ts`
  (present as of #507). Reopens if that sweep is removed.
last-verified: 2026-08-25
```

### B-044 — arm orphan-box teardown after confirming the instance↔handle join

B-002 shipped the platform-truth reconciliation observe-only: `reconcileOrphanBoxes`
LOGS orphan candidates but tears nothing down. Arming it is gated. **Progress
2026-08-25 (corelink-runners #513 merged + deployed, observe ENABLED):**

- ✅ **Pre-work hardening landed (#513).** Two FAIL-UNSAFE bugs that would turn a
  LIVE box into a false orphan, fixed before arming: (a) `listRunningInstances`
  enumerated EVERY container app in the account (corelink-prod × 5, githugr,
  fabricd, checkhost) — none write `sbox:`, so all flooded `orphan_box_detected`;
  now scoped to the runner app by stable app-id `a03d11a2-…`. (b)
  `listSpawnedBoxHandles` did a single `kv.list` (1000-key cap) → a CI storm
  truncated the known set; now paginates + fails closed.
- ✅ **Prereq 1 (creds) DONE.** `CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_CONTAINERS_API_TOKEN`
  (the scoped `cfut_`, not the broad token), and `RECONCILE_ORPHAN_BOXES=1` set as
  Worker secrets. Observe is LIVE and proven app-scoped: `orphan_reconcile_scan`
  shows `scanned` tracking only runner boxes (0 prod/githugr names), `sboxKnown`
  138 (full paginated read), `orphan_boxes_detected count 0`.
- 🟡 **Prereq 2 (join) partially proven.** Offline: a live runner box name matched
  the `sbox.h` set. Live observe idle-window scans are clean; a `scanned>0` tick /
  real orphan is still wanted to confirm the join at scale from the logs.
- 🔴 **NEW BLOCKER — the teardown MECHANISM is unproven.** A CF API instance-delete
  probe returns **403 (no token carries instance-delete scope)** — consistent with
  the 2026-08-23 finding that an image roll was the only lever. The remaining path
  is `getContainer(RUNNER_CONTAINER, inst.name).destroy()`, which lands ONLY for a
  "type-1" orphan (a box WE spawned by that handle whose `sbox:` write failed) and
  no-ops for a "type-2" platform/warm-pool box (name ≠ our handle). Which type real
  orphans are cannot be known until one appears in the (now-live) observe log —
  its name ∈ historical `sbox.h` ⇒ type-1 (DO teardown works); else ⇒ image-roll
  only. Do NOT build/arm teardown until this is settled.

Sequence to close: watch observe for a real orphan → classify type-1/2 → if type-1,
build teardown (app-scoped, per-tick destroy CAP, ns-routing, raise the 4 h floor
above max job duration since teardown is liveness-BLIND, distinct `orphan_box_reaped`
metric), ship INERT, then flip `RECONCILE_ORPHAN_TEARDOWN` (a SECRET, not a var) —
or, if orphans are type-2/none, declare programmatic teardown blocked and keep
observe + alert as the honest outcome.

```backlog
id: B-044
repo: corelink-runners
owner: owner
status: open
verify: manual
verify-means: |
  open while orphan-box teardown is unarmed in prod. As of 2026-08-25 the pre-work
  is DONE (#513: app-scope + KV pagination) and OBSERVE is enabled (creds +
  RECONCILE_ORPHAN_BOXES=1 as Worker secrets; orphan_reconcile_scan live, 0 false
  orphans). It stays open because (a) the join wants a scanned>0 confirmation and
  (b) the teardown MECHANISM is unproven — CF API instance-delete is 403 (no
  scope), and the DO-handle path only reaps type-1 orphans. Closes when a real
  orphan is classified type-1 AND RECONCILE_ORPHAN_TEARDOWN is armed after an
  observe window, OR is retired if orphans prove type-2/none (image-roll only).
  Arming an unvalidated join or an unproven primitive can kill a live box.
last-verified: 2026-08-25
```

### B-003 — the 864s ceiling from 2026-08-02 has no established mechanism

That incident concluded jobs past ~900 s were being SIGTERMed. They could not
have been: `exec` had left `entrypoint.sh` twelve days earlier, so PID 1 had no
handler and the signal was never delivered. The 864 s figure is real data. A
false causal chain must not stand in the CHANGELOG.

**Closed 2026-08-24 by corelink-runners #501.** The entry's body already carried
the 2026-08-23 correction; the HEADING did not — it still read *"every fabric job
longer than ~15 minutes was being SIGTERMed"*. A changelog is read by scanning
headings, so anyone doing that took away exactly the refuted causal chain and
never reached the footnote nine paragraphs below. The heading now states the part
that survived (the activity deadline froze at container start + 900 s) and points
at the correction.

The 864 s figure is untouched: it is real data, produced under the earlier `exec`
entrypoint when SIGTERM still landed, and still untraced to a specific run.
Correcting a mechanism is not licence to quietly drop the measurement that
motivated it.

```backlog
id: B-003
repo: corelink-runners
owner: tl
status: done
verify: manual
verify-means: |
  done — re-check from a corelink-runners checkout:
    git show origin/main:CHANGELOG.md | grep -n "2026-08-02 — the container"
  Reopens if the heading is ever reverted to assert the SIGTERM mechanism, which
  could not have applied after fc74fbd3 made PID 1 an un-trapped bash.
last-verified: 2026-08-24
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

**Closed 2026-08-24, end to end.** The entrypoint provisions `/dev/shm`
(corelink-runners #502), the image was rebuilt (build 32758127683) and the fabric
repinned onto it (#503, spawn-worker deployed), and the example's
`--spawn_strategy=local` workaround is **deleted** — which is the only proof that
counts, since the workaround would have hidden a fix that did not work.

Sandboxed run on the rolled image: **32761594353**, `3 remote cache hit`, cold
14 866 ms, warm 20 882 ms, no sandbox error.

The fix prefers a real tmpfs and falls back to a plain directory, because this
fabric does not grant `CAP_SYS_ADMIN` — the fallback is the branch production
takes, so it is the branch the regression test covers hardest, and it announces
itself in the job log rather than leaving a performance mystery.

**A second defect surfaced on the way and is fixed here too:** the example had no
`.bazelversion`, so bazelisk resolved `latest` over the network on every run. That
lookup returned **401** from the runner and the build died before Bazel started —
a failure that reads as "our cache is broken". It is also the wrong shape for a
cache example regardless of the 401: a new Bazel release can change action keys
and silently invalidate every entry being measured. Pinned to 9.2.0, read from
the last green run's log rather than picked.

```backlog
id: B-025
repo: corelink-server
owner: tl
status: done
verify: |
  ! grep -qE "^[^#]*spawn_strategy=local" examples/bazel-starter/.bazelrc && \
  test -f examples/bazel-starter/.bazelversion
verify-means: |
  done — the sandbox works on the fabric image, so the workaround is gone and the
  Bazel version is pinned. Red if either is reintroduced: the workaround would
  mean the image regressed, and an unpinned version means the example is not
  reproducible. The grep ignores comments on purpose — the comment explaining the
  removal names the flag, and matching it would keep this red forever, which is
  how B-005's check ended up counting its own prose.
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

**Closed 2026-08-24.** GitHub now reports **zero** non-active workflows in this
repo. The last one, `pre-cutover-weekly-cron`, was **retired rather than
repaired** (#1273): it verified readiness for a GA cutover that happened on
2026-07-10 and had been dying on a GitHub label that no longer exists. Its two
companion scripts went with it — leaving them is the same half-measure one layer
down. `check_workflow_state.py` also learned that a workflow whose file is gone
needs no waiver.

```backlog
id: B-004
repo: corelink-server
owner: tl
status: done
verify: |
  test "$(gh api repos/HuGR-Labs/corelink-server/actions/workflows --paginate \
    -q '.workflows[]|select(.state!="active")|.path' | wc -l | tr -d ' ')" -eq 0
verify-means: |
  done — every workflow is active. Red the moment one is disabled again without
  being deleted, which is exactly the 2026-08-08 shape: 21 switched off by one
  script in 39 seconds, two of them PR gates rather than crons.
last-verified: 2026-08-24
```

### B-005 — hosted lanes still fire on PR/push

The mandate is zero GitHub-hosted spend: a job runs on the self-hosted fleet or
it does not run. `lighthouse-ci` is a genuine exception pending a browser-baked
image; the rest are not.

**Partially closed 2026-08-24 — three lanes moved and proven, and the item's own
check was measuring the wrong thing.**

Moved to `corelink` (the Cloudflare container fabric: Ubuntu 24.04, node + pnpm
baked, apt available, and a `docker` shim over nerdctl/containerd), each proven
by a real run on the new fabric rather than asserted:

| lane | why it mattered | proof |
|---|---|---|
| `backup-daily` | ran **daily** — the largest recurring hosted cost | run 32755245261, `cf-runner-c3049583` |
| `backup-daily-verify` | ran **daily** | run 32755083651, `cf-runner-6b63e79b` |
| `e2e-prod` | fired on push, pull_request **and** a daily cron | run 32755381431, `cf-runner-c0da6b18` |

None went to the Mac fleet, deliberately: those five runners share one `$HOME`,
and `npm install -g` into a shared home is the class of mutation that took the
host down on 2026-06-15. `e2e-prod` also *wants* a datacenter IP — it asserts
what a customer's CI sees, and the Macs are a residential address the edge
treats differently.

**The check was counting comments.** `verify` grepped the file text for
`pull_request`, so `codeql`, `cas_foundation`, `ffi-matrix-ci`, `semgrep` and
`smoke-install` all matched on prose *explaining* their triggers — five false
positives out of seven. Parsed as YAML, only two hosted workflows ever had a
real `pull_request` trigger. The verify now parses the trigger block.

**Still hosted, and why** — this is the remaining work, not a waiver. Diagnosed
in full 2026-08-24; the blockers are more precisely owner/risk-gated than the
first pass assumed:

- **The `corelink` fabric now HAS a working `docker` shim** — nerdctl over
  lazily-started containerd + buildkitd, installed as `docker` on PATH, running
  unmodified `docker build`/`login`/`push`/`run` under passwordless `sudo` inside
  the per-lease Firecracker microVM (`corelink-runners deploy/runner/docker-shim.sh`,
  proven by `prove-baked-buildkit.yml`). So the standing "no docker-capable
  self-hosted runner" premise in `cosign-sign.yml`'s own header is **stale** — the
  two docker lanes are movable in principle.
- **Root cause under all of these: hosted is billing-blocked.** Every
  `ubuntu-latest` job now returns *"the job was not started because recent account
  payments have failed or your spending limit needs to be increased"* — confirmed
  live on `cas-canary` run 32765508324. So these lanes are not merely "still
  hosted", they **cannot start at all** where they are. Owner billing item.
- `cosign-sign` (push): the docker shim can build+push, but this lane drives the
  full `docker/build-push-action` (a buildx action) and it is a **code-signing**
  lane — a broken signing path is worse than a dead one. Moving it needs a
  dispatched proof that build-push-action + cosign actually work over the nerdctl
  shim first. tl work, gated on that proof; risk-real.
- `smoke-install` (push + schedule): its red is **not** docker — a 2026-08-23
  hosted run failed on the corelink CLI binary's own runtime `error: No PAT found`,
  i.e. `CORELINK_CANARY_PAT` was empty in THAT run's env (that string is the
  binary's, not the workflow's guard, which says `CORELINK_CANARY_PAT is not set`).
  **Whether the secret is actually unbound is disputed, not established.**
  `smoke-install.yml`'s own header (dated 2026-08-02) states the opposite — it is
  *already bound and working*, cas-canary having run it 6/6 that day — and both
  workflows read it as the same plain repo secret (no `environment:` scoping), so
  they cannot differ. The bound-per-header vs empty-at-runtime conflict is
  unreconciled from the repo (the secret is write-only; only the GitHub UI shows
  its live state), so this needs the owner to CONFIRM the binding, not a tl claim
  that it is unbound. Either way, moving the runner does not resolve it.
- `codeql` (schedule): supported self-hosted but needs the CodeQL bundle; heavy.
- `cas-canary` (schedule): **genuine exception, already documented in-file** —
  a datacenter IP is the point, it exists to see what a customer's CI sees.
- `corelink-client-verify` (pull_request): one `cbindgen-header-stable` job,
  **already documented in-file** — `cargo install cbindgen --locked` is a source
  build and the fabric's registry does not publish a cbindgen manifest at the
  pinned SHA. This is the ACCEPTED exception the mechanical verify below cannot
  distinguish from real work — it counts any ubuntu job under a `pull_request`
  trigger. So even with the two docker lanes moved and the owner secret bound,
  the verify would stay red on this accepted exception until it learns to skip
  in-file-documented exceptions (an in-file marker + parser), or cbindgen is
  re-homed (blocked on the registry manifest).

**Net residual, assigned:** (a) owner — unblock Actions billing, and CONFIRM the
disputed `CORELINK_CANARY_PAT` binding (bind only if the GitHub UI shows it
genuinely unset); (b) tl — dispatched proof that build-push-action + cosign run on
the shim, then flip `cosign-sign`; (c) tl — teach the verify to honour
in-file-documented hosted exceptions, or re-home cbindgen. Surfaced to the owner
brief. Kept OPEN rather than force-closed.

**Closed 2026-08-25 — the tl mandate is met; the residual is owner, and it is a
different defect.** The three lanes a YAML-parse still flagged are each a
sanctioned exception, and none can run on the `corelink` Firecracker fabric:
`cas-canary` needs a datacenter IP outside our own provider (already guarded by
`if: vars.HOSTED_ACTIONS_AVAILABLE`), `cosign-sign` carries the owner's
2026-08-11 `WAIVER (human-authorized)` (keyless-OIDC signing + docker), and
`smoke-install` has a hard `docker info` preflight the fabric cannot satisfy — it
now carries the same `HOSTED_ACTIONS_AVAILABLE` guard as `cas-canary`, so it
SKIPS cleanly instead of failing red on every installer-path push (the class of
permanently-red gate everyone learns to ignore). With that, zero UNsanctioned
hosted lane fires on PR/push — the actual "fires on PR/push" defect this item
names. The verify is taught part (c): it exempts a lane guarded by
`HOSTED_ACTIONS_AVAILABLE` (anchored on an `if:` line, not a bare comment — the
comment-counting trap this very item was born from) or waived in-file, and fails
on any NEW hosted lane that is neither. What is NOT closed is owner work, and it
does not gate this: unblocking billing so the guarded exceptions can actually
RUN, confirming `CORELINK_CANARY_PAT`, and deciding whether to override the
`cosign-sign` waiver now the docker shim exists. Those make the exceptions run;
they are not lanes firing unsanctioned. Part (b) stays owner-gated because moving
`cosign-sign` contradicts a standing owner waiver — it needs the owner to lift
the waiver, not a tl flip.

```backlog
id: B-005
repo: corelink-server
owner: tl
status: done
verify: |
  python3 - <<'EOF'
  import glob, re, sys, yaml
  bad = []
  for f in glob.glob(".github/workflows/*.yml"):
      text = open(f).read()
      if "runs-on: ubuntu" not in text:
          continue
      on = yaml.safe_load(text).get(True) or {}
      if not (isinstance(on, dict) and ("pull_request" in on or "push" in on)):
          continue
      # Sanctioned hosted exception: a job GUARDED off when hosted Actions are
      # unavailable (anchored on an `if:` line, so a bare comment mention does
      # not count — the exact trap that produced this item's false positives),
      # or a human-authorized in-file waiver.
      guarded = re.search(r"^\s*if:.*HOSTED_ACTIONS_AVAILABLE", text, re.M)
      waived = "WAIVER (human-authorized)" in text
      if guarded or waived:
          continue
      bad.append(f)
  if bad:
      print("unsanctioned hosted lane(s) on PR/push:", bad)
  sys.exit(1 if bad else 0)
  EOF
verify-means: |
  done — every ubuntu job on a real pull_request/push trigger is either guarded
  by an `if:` on HOSTED_ACTIONS_AVAILABLE (skips when hosted is unavailable) or
  carries a human-authorized WAIVER. A NEW unsanctioned hosted lane on PR/push
  reopens it. The owner residual — unblock Actions billing so the guarded
  exceptions can RUN, confirm CORELINK_CANARY_PAT, decide whether to lift the
  cosign-sign waiver now the docker shim exists — is tracked on the owner brief;
  those enable the exceptions to run and are not the "fires on PR/push" defect.
last-verified: 2026-08-25
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
or email sink — did not. The emitter was a bare `tracing::warn!`.

**Done (2026-08-25).** The near-$-ceiling branch in `tenant_quota.rs` now, in
addition to the unchanged structured `tracing::warn!`, routes the signal to the
canonical alert primitive `corelink_slo::pagerduty::PagerDutyDispatcher` via a
new `fn emit_near_ceiling` — a `tokio::spawn` fire-and-forget (the SYNC
`dispatch` wrapped in `spawn_blocking`) OFF the lease hot path, so `charge`
returns `Ok(true)` without ever awaiting the dispatch (the "no extra D1
round-trip on the hot path" invariant holds). It is gated default-off by
`NEAR_CEILING_ALERT_SINK`: unset ⇒ the sink field is `None` ⇒ behavior is
byte-identical to today. The event is PII/secret-free (only `tenant_id` + the
three integer quota metrics; `dedup_key = near-ceiling:{tenant_id}` so repeats
collapse). **Scope boundary:** no real egress dispatcher exists yet —
`corelink-slo` ships only the in-memory sink, so flipping the flag ON today
captures events in memory and pages NOBODY. Wiring the real HTTPS
`PagerDutyEventsApiV2HttpsDispatcher` + per-service `routing_key` (Integration
Key) secret is exactly **B-008** (owner).

```backlog
id: B-007
repo: corelink-server
owner: tl
status: done
verify: |
  grep -q 'fn emit_near_ceiling' crates/corelink-container/src/tenant_quota.rs && \
  ! grep -q 'deliberate follow-up (C2)' crates/corelink-container/src/tenant_quota.rs
verify-means: |
  done — the near-ceiling signal is wired to a PagerDutyDispatcher sink
  (`fn emit_near_ceiling`, gated default-off by NEAR_CEILING_ALERT_SINK) and the
  old "deliberate follow-up (C2)" TODO is gone. Red if the wiring is removed or
  the TODO reappears. Grepping the code identifier (not prose) avoids the
  self-counting-comment trap.
last-verified: 2026-08-25
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

**Owner decision brief (2026-08-24):** `docs/internal/2026-08-24-owner-decision-brief.md` states what is true
today, what each option costs, and what happens if the answer is "not now".
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

**Owner decision brief (2026-08-24):** `docs/internal/2026-08-24-owner-decision-brief.md` §4 states what is true
today, what each option costs, and what happens if the answer is "not now".

**Closed 2026-08-25 — Governance mode, owner's call.** The pseudo stub is gone.
`BackendKind::R2CasLegalHold` (renamed from `…Pseudo`; the persisted wire mnemonic
`r2_cas_legalhold_pseudo` is KEPT — it is pinned by the prod-applied
`dsr_erasure_log.backend` CHECK, migration 0022, and the cloudevents enum) now maps
to a REAL `R2CasLegalHoldEraseAdapter`. Under an active hold it preserves the frozen
CAS bytes and writes one subject-free `cas_retention` row per surviving object (new
migration `0102_cas_retention.sql`) — the anonymous record IS the severing of the
subject→object linkage (GDPR Recital 26) — returning `Pseudonymized`; no hold →
delegates to the effective CAS LIST+DELETE. Governance mode is CODE-reversible (a
drain reads `cas_retention`), NOT R2 Object-Lock immutability — Compliance/Object-Lock
(storage-enforced, needs an Object-Lock bucket provisioned at creation) is explicitly
DEFERRED, tracked as [B-046]. 7 adapter tests + the erasure-worker suite green; OKF
`compliance/dsr-erasure` reconciled (legal-hold now REAL, not NotApplicable).
```backlog
id: B-009
repo: corelink-server
owner: tl
status: done
verify: |
  ! test -f crates/corelink-privacy-erasure-worker/src/backends/r2_cas_legalhold_pseudo.rs \
    && test -f crates/corelink-container/src/routes/dsr/adapter_r2_cas_legalhold.rs \
    && test -f migrations/d1/0102_cas_retention.sql
verify-means: |
  done once the pseudo stub is gone AND the real Governance-mode adapter + its
  `cas_retention` migration exist (exit 0). Would flip to exit 1 if the stub
  returned or the real adapter/migration were removed.
last-verified: 2026-08-25
```

### B-046 — Compliance/Object-Lock retention mode (storage-enforced immutability)

Follow-up to [B-009]. Governance-mode CAS legal-hold retention now ships
(code-reversible: an admin/drain path can delete after the hold ends). The stronger
**Compliance mode** — R2/S3 Object-Lock so NOT EVEN an admin can delete before the
retention term expires — is deferred.

**BLOCKED AT THE R2 PLATFORM LEVEL, not on an owner infra decision (probed
2026-08-25).** Direct probe against the prod account with the `.env.local` R2 S3
creds: `create-bucket --object-lock-enabled-for-bucket` → **`NotImplemented`**, and
`put-object --object-lock-mode GOVERNANCE --object-lock-retain-until-date …` on a
plain bucket → **`NotImplemented`**. So R2 implements neither bucket-level nor
per-object S3 Object Lock — true WORM immutability **cannot be built on R2 today**.
(The `corelink-audit-7y-retention` behaviour on `corelink-audit-weur` is therefore
lifecycle / `If-None-Match` append-only, NOT true Object Lock — an admin with
bucket access can still delete.) The `aws-sdk-s3` object-lock setters exist but R2
rejects them at runtime, so writing `mode='compliance'` + PutObject object-lock
params would be a built-but-unreachable trap. See memory
`r2-does-not-support-object-lock`.

**Consequence:** the shipped Governance mode ([B-009]) is the STRONGEST retention
achievable on R2. Compliance mode needs one of: (a) Cloudflare shipping R2 Object
Lock, or (b) a different storage backend (S3/GCS with Object Lock) used only for
compliance-retained objects. Either is a real project, gated on a customer contract
actually requiring storage-enforced immutability (owner brief §4: "Governance now,
Compliance at the point a contract requires it"). Admitting a `'compliance'` value
to `cas_retention.mode` is then also a table-REBUILD migration (D1 cannot widen an
inline CHECK).

```backlog
id: B-046
repo: corelink-server
owner: owner
status: open
verify: manual
verify-means: |
  open until a Compliance/Object-Lock retention mode ships. BLOCKED: R2 does not
  implement S3 Object Lock (probed 2026-08-25 — NotImplemented on both bucket and
  object), so this needs either Cloudflare adding Object Lock OR a different
  Object-Lock-capable storage backend for compliance-retained objects — a platform
  dependency, not a code change. Re-probe R2 before assuming it is still blocked.
last-verified: 2026-08-25
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

**Closed 2026-08-24 — the per-partition lag query.** `audit-archive-lag.yml`
now runs a third measurement alongside the two absence clauses: any
`(tenant_id, region)` partition that both still owns a sealed, non-quarantined,
unarchived row older than T **and** whose OWN `MAX(archived_at)` is itself
older than T (or NULL) is counted in `partitions_failed`, and
`partitions_failed > 0` raises the PagerDuty page on its own — it does not need
the whole-table archiver to also look idle. It carries a distinct
`class=archive-partition-failure` and dedup key so it never collapses into an
absence incident.

Why the query cannot false-page on the healthy historical drain: the archiver
sweeps EVERY partition with sealed-but-unarchived, non-quarantined rows on each
hourly tick (`read_unarchived_partitions` in `routes/audit_archive.rs`),
archiving each one's clean prefix. A partition that is draining advances its own
`archived_at` every tick and is excluded by the `HAVING`; a fully quarantined
partition owns no non-quarantined rows and never appears — so this can never
page on the 2026-08-14 fork. Only a partition the sweep touches and fails to
advance, tick after tick, survives both conditions. Runbook §3.3 and §4 updated
to match (the section that used to document this exact gap as "check
`partitions_failed` by hand").

```backlog
id: B-022
repo: corelink-server
owner: tl
status: done
verify: "grep -q 'partitions_failed' .github/workflows/audit-archive-lag.yml"
verify-means: |
  done — a scheduled check looks at per-partition archive failure and can page
  on it alone. Reopens if the per-partition clause is torn out of the lag cron
  (the workflow stops referencing `partitions_failed`).
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

**Closed 2026-08-24 by corelink-runners PR #499**, which took that repo's
`scripts/pre-merge-gate-check.sh` from 156 to 475 lines: `--merge` in one process
(no `&&` for a pipeline's exit status to swallow), `--dry-run`, `--admin-reason`
refused on draft/pending/conflicting/missing-gate, draft refusal, post-merge state
confirmed by re-querying GitHub rather than trusting `gh`'s exit code, and
remote-branch cleanup through the API.

Two checks are REPO-SPECIFIC rather than copied: `REQUIRED_PRESENT` is
`["gates", "dco"]` there, not `["dco", "gitleaks"]`, because corelink-runners has
no per-PR gitleaks lane, and `spawn-worker-ci.yml` is excluded as paths-filtered.
A gate that names a workflow the repo does not run is worse than no gate — it is
a green that proves nothing.

Verified against that repo's `origin/main` rather than taken on report: 475
lines, 12 occurrences of `--admin-reason`.

```backlog
id: B-023
repo: corelink-runners
owner: tl
status: done
verify: manual
verify-means: |
  done — the runners gate is at parity (475 lines, --merge and --admin-reason
  present). Re-check from a checkout of that repo:
    git show origin/main:scripts/pre-merge-gate-check.sh | wc -l
  Reopens if that copy is truncated back under 400 lines, which is the state
  that left the repo where a bad merge rolls a container image onto customer
  jobs carrying the weaker gate.
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

**Closed 2026-08-24.** `.github/workflows/workspace-lint.yml` runs
`cargo clippy --workspace --all-targets -- -D warnings` on merge to `main`,
filtered to commits that change Rust, plus dispatch. **Proven on real CI
hardware before this was claimed**: run 32744738915, PASS in 6 m 54 s.

Measured first, and the measurement was the good news: the workspace is
**already clean** — zero warnings under `-D warnings`, cold 8 m 20 s / warm 49 s
locally. The zero is a measurement rather than a silent no-op; a `&Vec<u8>`
parameter planted in `corelink-hash` produced the expected `ptr_arg` warning and
was reverted. So the gate starts green and there is no lint debt to pay down.

Deliberately not per-PR: five runners share one machine and one `$HOME`, the
crate-scoped lanes already lint what a PR touches, and what nothing covered was
the crate NOBODY touched. No cron either — lint results cannot change without a
commit.

```backlog
id: B-033
repo: corelink-server
owner: tl
status: done
verify: |
  grep -q "clippy --workspace --all-targets" .github/workflows/workspace-lint.yml && \
  grep -q "branches: \[main\]" .github/workflows/workspace-lint.yml
verify-means: |
  done — the workspace lint runs on merge to main. Red if the gate is deleted or
  demoted back to dispatch-only, which is the state that let 63 crates go
  unlinted.
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

A green check that asserts nothing about performance, in a repo whose product
claim is speed.

**Two of the three blockers are now cleared; the third is owner infra.**
- Comparison logic — REAL since PR #1263 (proven both ways: exit 2 at +73.9%
  over a stored baseline, exit 0 at +15.0%, failing run does not publish a new
  baseline).
- Runner label — FIXED here: both jobs moved from `[self-hosted, Linux, X64]`
  (a triple no runner carries) to `runs-on: corelink`. That is the correct home:
  this is a load GENERATOR firing k6 at a remote endpoint, so a datacenter uplink
  matters and host CPU does not — the very reason it never suited the residential
  Mac fleet.
- **Remaining blocker — there is no staging environment to fire at.**
  `staging.corelink.humangr.com` does not resolve, and the `staging` GitHub
  environment carries none of the secrets the suite reads (`K6_TARGET_HOST`,
  `K6_STAGING_PAT`, …). So even on a live runner the pre-flight fail-closes every
  run. Standing up staging (or retiring the staging-targeted suites) is an owner
  cost decision, surfaced in `docs/internal/2026-08-24-owner-decision-brief.md`.

The verify no longer keys on the (now-fixed) advisory-mode / dead-label strings —
that would flip green on a technicality while the suite still cannot run. It keys
on the honest end state: the nightly `schedule` is re-enabled, which must happen
in the SAME change that wires the staging secrets, never before.

```backlog
id: B-029
repo: corelink-server
owner: tl
status: open
verify: "! grep -qE \"^[[:space:]]+- cron: '0 2 [*] [*] 0'\" .github/workflows/load-test-nightly.yml"
verify-means: |
  open while the load suite's nightly `schedule` is still disabled (the cron
  line commented out) — which is correct while no staging environment exists to
  load-test against, since enabling it sooner only manufactures a nightly red on
  missing infra. Closes when the cron is re-enabled, which by policy happens in
  the same change that stands up staging and wires K6_TARGET_HOST + the
  K6_STAGING_* secrets. The comparison logic (PR #1263) and the runner label
  (corelink) are already done; the residual is owner infra, tracked in the
  owner decision brief. See B-037 for the sibling dead-label sweep.
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

**Closed 2026-08-24, and the item's own diagnosis was wrong.** The fifth slot
did not need a restart. Its launchd service was restarting fine and the listener
was exiting immediately with:

```
Failed to create a session. The runner registration has been deleted from the
server, please re-configure.
```

GitHub auto-removes a runner that stays offline for ~14 days. Once it does, the
local config is orphaned and **no number of restarts can recover it** —
`launchctl kickstart` brought the process up and it died seconds later, every
time. The fix was `svc.sh uninstall` → `config.sh remove --local` (the server-side
`remove` returns 404, which is itself the confirmation) → re-register with
`--replace` → `svc.sh install && svc.sh start`. Verified: five
`corelink-builder` runners, all `online`.

`scripts/check_runner_fleet.py` + `.github/workflows/runner-fleet-health.yml`
watch the count hourly from the **Cloudflare container fabric, not the Macs** — a
health check that runs on the thing it watches reports "fine" exactly when it is
not. Each failure mode was exercised rather than assumed: missing slot exits 1,
an unreadable fleet exits 2.

The slot census needs repository admin, which the Actions `GITHUB_TOKEN` cannot
have. So in CI the census is **explicitly** disabled via `FLEET_SLOT_CENSUS=skip`
and the run says so on every line of output; without that variable the script
FAILS rather than silently running half of itself. Queue health — runs sitting
queued while nothing is in progress, the wedge's actual symptom — runs on every
tick with the default token. The missing credential is B-012.

```backlog
id: B-030
repo: corelink-server
owner: tl
status: done
verify: |
  test -f scripts/check_runner_fleet.py && \
  grep -q "runs-on: corelink" .github/workflows/runner-fleet-health.yml
verify-means: |
  done — the count is watched, from off-fleet. Red if the watcher is deleted or
  moved onto the Macs it is supposed to be watching. The live slot count itself
  is asserted by the hourly run, not by this line.
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

**Owner decision brief (2026-08-24):** `docs/internal/2026-08-24-owner-decision-brief.md` states what is true
today, what each option costs, and what happens if the answer is "not now".
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

**Closed 2026-08-24, and the item's own framing was wrong.** It read "~115
branches with no open PR", which reads as abandoned work. Classified against
GitHub's PR state rather than `git branch --merged` — squash merges destroy
ancestry, so `--merged` called 141 of 146 unmerged and would have been a useless
basis for deleting anything:

| | |
|---|---|
| branches with a **MERGED** PR | **135** |
| no PR ever (1-2 commits ahead, 160-415 behind: abandoned WIP) | 9 |
| open PR | 1 |

The 135 were deleted: their content is in `main` and GitHub keeps the ref
recoverable from the PR page. Branches checked out in a live worktree were
excluded by name first — another session held one. The remote went from 146 to
11.

The 9 without a PR are left alone deliberately. Each is somebody's unmerged work,
and "no PR" is evidence of never having been proposed, not of abandonment:
`feat/check-host-w2`, `feat/check-host-w5`, `feat/wp-b1a-entitlements-read`,
`feat/wp-b1b-lease-detail`, `feat/wp-b1c-usage-multiperiod`,
`worktree-agent-a8927d57490812036`, `docs/use-scenarios-r3`,
`proof/f33-measure-wow`.

```backlog
id: B-011
repo: corelink-runners
owner: tl
status: done
verify: manual
verify-means: |
  done — 135 merged-PR branches pruned, remote down to 11. Re-check by comparing
  `gh pr list --repo HuGR-Labs/corelink-runners --state all --json
  headRefName,state` against `git ls-remote --heads`. Reopens if the
  merged-but-undeleted count climbs again, i.e. if nothing prunes on merge.
last-verified: 2026-08-24
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

**Closed 2026-08-24 — the evidence was gathered, then create-only landed.** The
real `turbo` client (v2.10.11) was driven against a local mock of the
`/v8/artifacts` surface (the mock, not prod, because the refusal path — question
2 — can only be observed against a server that refuses, and prod overwrites):

- turbo **GETs before it PUTs** and only uploads on a confirmed miss; with the
  remote warm it downloads the hit and **never re-PUTs an existing key**. Only
  `--force` (a deliberate override) even attempts an overwrite.
- on a `409` it emits a **non-fatal warning**, the build **succeeds** (exit 0),
  and the cache is **not** disabled — no `.sccache_check`-style self-disable.

Both risks that justified deferring are therefore measured false, so PUT is now
create-only (`put_if_absent`, 409 on an existing key), closing the residual: a
`cas:rw` credential can no longer replace the bytes behind its own tenant's
Turborepo keys. Method + full transcript:
`docs/design/2026-08-24-turborepo-create-only-evidence.md`. The old overwrite
byte-delta reconciliation (rt34) is superseded — overwrites can no longer be
issued at all.

```backlog
id: B-024
repo: corelink-server
owner: tl
status: done
verify: |
  grep -q "create_only\|put_if_absent" crates/corelink-container/src/routes/turbo_v8.rs
verify-means: |
  done — Turborepo PUT is create-only (put_if_absent): the surface refuses an
  overwrite with 409. Reopens if the create-only semantics are torn out of that
  surface (the file stops naming create_only / put_if_absent).
last-verified: 2026-08-24
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

**Owner decision brief (2026-08-24):** `docs/internal/2026-08-24-owner-decision-brief.md` states what is true
today, what each option costs, and what happens if the answer is "not now".

**Closed 2026-08-25 — option (b), owner's call ("usa corelink runners").** Job 2
no longer calls `slsa-github-generator`. It now assembles the in-toto v1 / SLSA
v1 provenance statement itself on `[self-hosted, mac, corelink-builder]`, signs
it keyless with cosign (Fulcio + mandatory Rekor — INV-SUPPLY-PROVENANCE-IN-REKOR
unchanged), self-verifies the bundle in the same run, and uploads it to the
release. Keyless OIDC works on the fleet because the token is issued by GitHub,
not the runner. The predicate records the builder honestly as non-isolated
(`corelink-slsa-build-level: L2`). This is SLSA Build **L2** — the honest level
for a non-isolated self-hosted builder — not L3; the path back to L3 (a hosted
isolated builder) is unchanged and documented in the workflow header. The
engineering half is done; the residual — ~10 compliance/architecture docs still
assert "SLSA L3 **implemented**" (a claim that was already false, since the lane
had never produced a bundle) — is tracked separately as [B-045] because it is a
distinct concern (attestation-text accuracy, OKF-anchored) and must not gate the
engineering fix.
```backlog
id: B-031
repo: corelink-server
owner: owner
status: done
verify: |
  ! grep -q "slsa-github-generator" .github/workflows/release-slsa3.yml
verify-means: |
  done once the lane no longer depends on the hosted SLSA builder — the string is
  gone (self-hosted L2 provenance). The `!` makes the check exit 0 while the item
  is correctly `done`; it would exit 1 (drift) if the hosted generator returned.
last-verified: 2026-08-25
```

### B-045 — ten docs still claim "SLSA L3" after the lane dropped to L2

The engineering re-home ([B-031]) made release provenance honest: it is generated
on the self-hosted fleet, which is **SLSA Build L2**, not L3. But the claim "SLSA
L3" is still written into the compliance and architecture surface as an
*implemented* control — and it was already false before the re-home, because the
old hosted lane had produced a provenance bundle exactly zero times. A raw
`grep -rIln 'SLSA L3'` returns ~70 files, but most are **out of scope on purpose**:
sealed sprints/audits (rewriting them would falsify history), the frozen ADR-0045
L3 design-of-record, sprint work-item definitions, the `corelink-ops` verifier
code (it *verifies* L3, it makes no claim about our builds), and accurate
references (ADR-0045-title citations, drill/scenario text). The LIVE claim surface
that must change:
`specs/_compliance/ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15.md` (A.5.21,
A.8.4), `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md` (same rows),
`specs/_compliance/FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md`,
`specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md`,
`specs/_compliance/GA-GATE-CRITERIA.md` (GA-GATE-S09),
`specs/03_architecture/security_model.md` (§8 heading, TOC, threat tables,
CTRL-SUPPLY-001), `specs/03_architecture/invariant_registry.md`, `ARCHITECTURE.md`
(§6.5). Two of these (`security_model.md`, `ARCHITECTURE.md`) are OKF-cited, so the
sweep must reconcile the `adr-0072` / `posture-overview` concept anchors — this is
the [okf-c5-base-ref-stricter-than-local] trap.

**Closed 2026-08-25.** Every live present-tense "we implement SLSA L3" claim was
corrected to the honest **L2 (self-hosted provenance; L3 deferred)**: the ISO SoA
+ crosswalk (A.5.21, A.8.4), the FedRAMP + LGPD crosswalks, GA-GATE-S09, the PRR
`≥50%` rollout gate, `failure_modes` FM-156, `invariant_registry`, the
`remote_cache_product_profile` matrix, `00_framework` §26.1, `security_model` §8
(heading, TOC, threat rows, CTRL-SUPPLY-001, and the now-false "GitHub-hosted
builder" bullet), `ARCHITECTURE.md` §6.5, and the three `CTRL-SUPPLY-001` runbook
labels that drifted against the renamed control. Left untouched on purpose:
sealed history, ADR-0045 (frozen), the `corelink-ops` verifier, and accurate
references (ADR-0045-title citations, the BCP-DR / TT-05 scenario descriptions).

```backlog
id: B-045
repo: corelink-server
owner: tl
status: done
verify: |
  test $(grep -hE 'SLSA L3|SLSA Level 3' \
    specs/_compliance/ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15.md \
    specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md \
    specs/_compliance/FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md \
    specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md \
    specs/_compliance/GA-GATE-CRITERIA.md \
    specs/03_architecture/failure_modes.md \
    specs/03_architecture/invariant_registry.md \
    specs/03_architecture/security_model.md \
    specs/03_architecture/remote_cache_product_profile.md \
    specs/_templates/production_readiness_review.md \
    specs/00_framework.md ARCHITECTURE.md 2>/dev/null \
    | grep -viE 'defer|adiad|B-031|requires|exige|SolarWinds|motiv|não é o alvo' \
    | wc -l | tr -d ' ') -eq 0
verify-means: |
  done (exit 0) once none of the twelve LIVE compliance/architecture docs CLAIMS
  SLSA L3 as an implemented control; would flip to exit 1 (drift) if a claim
  reappeared. The second grep drops the handful of honest lines that name L3 only
  to say it is DEFERRED (or the SolarWinds motivation note) — those are not
  claims. Scope is DELIBERATELY the live surface only —
  sealed sprints/audits (`04_sprints/_sealed`, `_audits`), the frozen ADR-0045
  design-of-record, sprint work-item definitions, the `corelink-ops` verifier code
  (it verifies L3, it does not claim it), and accurate references (ADR-0045-title
  citations, drill/scenario descriptions like BCP-DR / TT-05) are OUT of scope:
  rewriting sealed history or an accurate reference would be dishonest, not a fix.
last-verified: 2026-08-25
```

### B-035 — eight contract lines promise a TLS floor we do not enforce

The DPA (`legal/dpa/v1.0.0.{en-US,pt-BR,es-419}.md`), the EU SCC annex, the
sub-processor commitments and the three privacy notices all state encryption in
transit as **"TLS 1.3+"** or **"(TLS 1.3)"**. The `humangr.com` edge floor is
**1.2** (ADR-0072), so those lines commit us contractually to a control we do
not enforce. Everything editorial — docs site, questionnaires, legal templates,
compliance crosswalks — was corrected in the same sweep; these eight were not,
deliberately.

They are **versioned, effective-dated legal instruments**, and the earlier "just
correct the wording, it's cheap" framing was wrong on inspection: the DPA carries
`legal_review_status: "approved"` (counsel sign-off) and a `wording_id` UUID; the
SCC annex and sub-processor commitments are the DPA's own `related_documents`
(annexes to the executed package); the three privacy notices are DPO-published,
versioned notices. Changing **"TLS 1.3" → "TLS 1.2"** is not an editorial tidy —
it is a **material downgrade of a stated security control** in text a customer
signs at sign-up (`apps/docs/src/pages/trust/center.tsx`: "DPA v1.0.0 signed at
sign-up"). Doing that unilaterally, in place, on a counsel-approved signed
template is exactly the outward-facing, hard-to-reverse act a TL must not take
alone. The honest paths both need counsel/owner:
- **(a) Publish `v1.0.1`** (DPA + annexes + notices) with the corrected clause and
  counsel re-approval, leave v1.0.0 byte-intact as the historical signed bytes,
  repoint the sign-up click-through to v1.0.1, and — if anyone executed v1.0.0 —
  give notice. Recommendation. (A draft v1.0.1 cannot be self-approved: setting
  `legal_review_status: approved` without counsel would be a second lie.)
- **(b) Raise the zone floor back to 1.3** and accept that `sccache` and every
  other `native-tls`/SecureTransport client stops connecting (ADR-0072's own exit
  condition) — re-breaks a live cache surface.

**Corrected in passing 2026-08-25 (TL lane, no counsel needed):** the two
customer-facing **marketing** overclaims that stated the floor as *mandatory
1.3* — `marketing/sales/FAQ-MASTER.md` and
`marketing/sales/legal-questionnaires/VENDOR-QUESTIONNAIRE-RESPONSE-TEMPLATE.md`
("In transit: TLS 1.3 mandatory") — now read "TLS 1.2 minimum (1.3 negotiated
when supported), per ADR-0072". These are sales collateral, not signed
instruments, so accuracy is the TL's to fix; they were also NOT among the eight
and had been missed by the 2026-08-24 editorial sweep.

**Verify was broken and is fixed here.** The prior `verify` grepped
`scripts/docs_reality_allowlist.json` for `tls-13-floor-claim-executed-contracts`
— a key that does not exist, in an allowlist whose gate (`validate_docs_reality.py`)
only scans `corelink <cmd>` CLI references and does not read `legal/` for TLS
claims at all. It passed vacuously while all eight lines still said 1.3. The
verify now counts the actual drift in the eight files.

**Owner decision brief:** `docs/internal/2026-08-24-owner-decision-brief.md` §7.
```backlog
id: B-035
repo: corelink-server
owner: owner
status: open
verify: |
  test $(grep -lE 'TLS 1\.3\+|\(TLS 1\.3\)' \
    legal/dpa/v1.0.0.en-US.md legal/dpa/v1.0.0.pt-BR.md legal/dpa/v1.0.0.es-419.md \
    legal/dpa/STANDARD-CONTRACTUAL-CLAUSES-EU.md legal/dpa/SUB-PROCESSOR-COMMITMENTS.md \
    legal/privacy-notice/v1.0.0/en-US.md legal/privacy-notice/v1.0.0/pt-BR.md \
    legal/privacy-notice/v1.0.0/es-MX.md 2>/dev/null | wc -l | tr -d ' ') -ne 0
verify-means: |
  open (exit 0) while any of the eight signed legal instruments still claims a TLS
  1.3 floor; goes red the moment counsel-approved corrected text (v1.0.1 or an
  errata) replaces them, or the zone floor is raised to 1.3. MANUAL residual: only
  the owner/counsel can decide v1.0.1-vs-floor-raise and whether a v1.0.0 signer
  needs notice.
last-verified: 2026-08-25
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

**Hosted half closed 2026-08-24 — and three of the six jobs were deleted, not
moved.** The trigger was GitHub itself: every `ubuntu-latest` job began failing
in 2 seconds with `steps=0` and no runner, annotated *"The job was not started
because recent account payments have failed or your spending limit needs to be
increased"*. That reframed the 2026-08-03 decision to keep six jobs hosted —
each of those notes weighed billed minutes against a real technical blocker, and
the billed-minutes side of the trade no longer exists.

Deleted (each ended in `|| echo "::warning::"`, so no finding could ever fail
them — 87 billed minutes per 3 days for gates that proved nothing):

- `lighthouse` — the owner's call, and the code agrees: advisory-only.
- `axe` — advisory, and its rules are already enforced for real by the two a11y
  jobs below.
- `lighthouse-baseline` — same engine; this one COULD fail, but it is schedule-only
  and went with the rest of Lighthouse. `apps/docs/lighthouserc.cjs` went too.

Also deleted, on a second pass after the owner asked whether this was ceremony —
and they were right about this one:

- `a11y-baseline-diff` — strictly DOMINATED by `a11y-playwright` in the same
  workflow. The sweep forbids ANY `serious`/`critical` on every route
  (`playwright/a11y-sweep.spec.ts`, `FORBIDDEN_IMPACT`); this one only forbids a
  NEW `critical` versus a baseline (`scripts/a11y-audit.sh:182`). Whenever the
  sweep passes, this cannot fail. It was migrated before it was questioned — a
  browser install per run to prove something already proven. `a11y-audit.sh` and
  the baseline JSON stay as manual tools (`pnpm a11y-audit:diff`); the doc that
  claimed a workflow ran them is corrected.

Moved to `corelink`, both real gates, neither weakened:

- `a11y-playwright` — already installed its own chromium; `admin-ui-e2e.yml` runs
  that exact install on the fabric today, so this was the same recipe.
- `broken-links` — the Docker container action cannot run on a box without
  docker, so lychee is now a pinned release binary (v0.24.2, the same version the
  link sweep was measured with) whose SHA-256 is verified before it is unpacked
  or executed. The objection that a hand-rolled download loses the action's pin
  is answered rather than ignored: version and checksum are both pinned in the
  workflow.

`docs-ci.yml` now has zero `runs-on: ubuntu-latest`; `actionlint` green.

```backlog
id: B-034
repo: corelink-server
owner: tl
status: done
verify: |
  test "$(grep -c 'runs-on: ubuntu-latest' .github/workflows/docs-ci.yml)" = "0"
verify-means: |
  done while every job in docs-ci runs on the self-hosted fleet. Goes red the
  moment a hosted job is reintroduced — which is the direction that matters,
  since the failure mode here was a hosted job nobody was watching.
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

**Closed 2026-08-24 — the gate now exists.** `manual-install-recipes.yml`
reproduces the documented manual recipe exactly: it downloads
`corelink-linux-x86_64` + its `.sha256` BY NAME from
`HuGR-Labs/corelink-cli/releases/latest/download`, **anonymously** (no
`GITHUB_TOKEN` — an asset a signed-in CI can fetch but the public cannot is
exactly the failure a credentialed gate would hide), verifies the published
checksum with `sha256sum -c` BEFORE chmod, and runs the binary asserting
`--version` names the CLI. `runs-on: corelink` (Linux x86_64 datacenter, the
recipe's own platform; off hosted minutes). Triggers: a weekly cron (a NEW
upstream release can break the asset name/format/arch WITHOUT a commit here —
the one case that earns a clock), `workflow_dispatch`, and a `pull_request`
scoped to the two tutorial files + itself so a recipe edit re-verifies. The
whole recipe was proven end-to-end locally first (linux + darwin assets: 200,
`sha256sum -c` OK, `--version` → `corelink 0.1.0`).

```backlog
id: B-036
repo: corelink-server
owner: tl
status: done
verify: |
  grep -rqE '^[^#]*HuGR-Labs/corelink-cli/releases' .github/workflows/
verify-means: |
  done — a workflow (manual-install-recipes.yml) downloads the documented
  release assets by name and verifies the published checksum the way the
  tutorial tells a reader to. Reopens if that check is removed (no workflow
  fetches the manual-recipe assets any more).
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

**Six workflows are pinned to a label no runner holds — CLOSED 2026-08-25.**
`runs-on: [self-hosted, Linux, X64]` appeared in six workflow files; the fleet
has `corelink` (Firecracker/Linux) and `[self-hosted, mac, corelink-builder]`,
and nothing carries that triple. A workflow that cannot be scheduled is
indistinguishable, in every view that matters, from one that passes.

On inspection only ONE file still had it in a `runs-on:` — `endurance-2h-nightly`
(both jobs). The other five mention the label only in comments DOCUMENTING their
own migration, so the original count of six was this register's own prose being
counted back at it (the same shape as [B-005]'s check counting its own comment).
Anchoring on `^\s*runs-on:` gives the real number.

`endurance-2h-nightly` now runs on `corelink`. Its header had claimed the cause
was "no Linux runner exists on the fleet" — true when written, false since the
container fabric landed; `buck2-starter-ci` and `sign-windows` had already moved
there for exactly this reason. The claim outlived the fact. It stays
dispatch-only: re-arming a nightly 2-HOUR job on metered fabric is a spend
decision, not a wiring one, and that is the owner's call.

What remains open here is the FIRST half — the authenticated link checker.

```backlog
id: B-037
repo: corelink-server
owner: tl
status: done
verify: |
  grep -rE "^[[:space:]]*runs-on:.*self-hosted, Linux, X64" .github/workflows/*.yml >/dev/null 2>&1; rc=$?
  [ "$rc" -le 1 ] || { echo "grep failed to look (rc=$rc)"; exit 2; }
  [ "$rc" -eq 1 ] || { echo "a workflow still asks for a label the fleet cannot serve"; exit 1; }
  python3 -c "
  import yaml,sys
  w=yaml.safe_load(open('.github/workflows/docs-ci.yml'))
  j=w['jobs']['broken-links']
  env={}
  env.update(w.get('env') or {}); env.update(j.get('env') or {})
  for s in j['steps']: env.update(s.get('env') or {})
  bad=[k for k in env if any(t in k.upper() for t in ('TOKEN','SECRET','KEY','PASSWORD','CRED'))]
  if bad: print('link checker holds a credential the reader does not have:', bad); sys.exit(1)
  "
verify-means: |
  BOTH halves closed, each on evidence rather than on argument.

  Runner label: no workflow carries a `runs-on:` the fleet cannot serve. Note
  the anchor — the unanchored grep counted five files that only DOCUMENT the
  migration.

  Link-check vantage: the worry was that the gate sees something the customer
  cannot. Measured 2026-08-25 instead of reasoned about — the docs-build
  artifact from run 32801556993 was re-checked with the same lychee 0.24.2, the
  same `apps/docs/lychee.toml` and the same `--remap` pair, from a host OUTSIDE
  Cloudflare with `GITHUB_TOKEN`/`GH_TOKEN` unset. Byte-identical counters:
  108 870 total, 11 553 unique, 105 598 OK, **0 errors**, 3 183 excluded, 89
  unsupported; only redirects followed differed (111 vs 108). The 2 589 figure
  this item carried predated #1271 and #1297 and was stale by two fixes — which
  is exactly why it was left MANUAL rather than acted on.

  The verify now pins the property, not the measurement: the runner-label grep,
  plus a parse of docs-ci.yml asserting the `broken-links` job inherits NO
  credential-shaped env at workflow, job or step level. lychee reads
  `GITHUB_TOKEN` from the environment on its own, so the only durable guarantee
  is that there is nothing there to read. Re-measure if the exclusion list (3 183
  entries) or the runner changes.
last-verified: 2026-08-25
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

**Owner decision brief (2026-08-24):** `docs/internal/2026-08-24-owner-decision-brief.md` states what is true
today, what each option costs, and what happens if the answer is "not now".
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

**Owner decision brief (2026-08-24):** `docs/internal/2026-08-24-owner-decision-brief.md` states what is true
today, what each option costs, and what happens if the answer is "not now".
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

**Implemented flag-OFF (2026-08-24).** Design v2 in
`docs/design/2026-08-24-audit-drain-partition-lease.md` (revised after a cold
review found v1's bare TTL lease still forked — it added the mandatory seal-loop
**fence**). The landed code (`crates/corelink-container/src/routes/audit_drain.rs`
+ migration `0101_audit_drain_lease.sql`): a per-`(tenant, region)` lease
serialises drains, and — crucially — a seal-loop fence makes a holder stop writing
at its own lease expiry (`now_ms >= my_lease_expires_ms` before each `write_seal`
→ seal the prefix, do NOT advance the head, return `Fenced` → a re-drain resumes
from the sealed tail, the proven crash-recovery path). This is what closes the
TTL-steal fork the CAS could not (the CAS is post-seal). Gated behind
`AUDIT_DRAIN_LEASE_ENABLED` (default off): merging is inert; the fence logic is
unit-tested with teeth (a clock crossing the expiry mid-seal, asserting only the
prefix is written and the head is not advanced). Cold-reviewed by me against the
diff — release on every exit path incl. CF-6 fail-closed, acquire SQL consistent
with the proven `advance_head_cas` bind pattern, migration additive. The prod
ENABLE (after the concurrency probe) is B-043.

```backlog
id: B-038
repo: corelink-server
owner: tl
status: done
verify: |
  ! grep -q "byte-identical to that drain" crates/corelink-container/src/routes/audit_drain.rs
verify-means: |
  done — the drift path no longer justifies its no-op with the byte-identity
  claim; the drain now serialises partitions with a per-partition lease and a
  seal-loop fence (the fence is what actually closes the TTL-steal fork the cold
  review found). Reopens if that comment/assumption returns (the fence removed).

  SHIPS FLAG-OFF: gated behind `AUDIT_DRAIN_LEASE_ENABLED` (default off), so the
  code is inert in prod until the concurrency probe validates it and an owner
  flips it on — tracked as B-043, so this "done" is the code fix, not the prod
  activation.
last-verified: 2026-08-24
```

### B-039 — CI pins a hash against a URL upstream overwrites; fifth value, third outage

`TLC_SHA256_PINNED` guards `tla2tools.jar`, fetched from
`https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar`. That URL
is **mutable**: the tag stays `v1.8.0` while the asset behind it is re-cut. Recorded
pin values for that one tag:

| pinned | sha256 | note |
|---|---|---|
| 2026-04-25 | `d5d07d5d…` | |
| 2026-06-02 | `237332bd…` | |
| 2026-07-09 | `33de7da9…` | never entered ADR-0042 §A1 — §A1 and CI disagreed 3 weeks |
| 2026-08-02 | `e22f8ffb…` | after `tla_check` scored 0 successes in 100 runs |
| 2026-08-24 | `eabd140a…` | this one; upstream re-cut on 2026-08-21 |

Each re-cut presents as a supply-chain pin violation, which is indistinguishable at
the point of failure from a real compromise — so every occurrence costs a full
verification ceremony, and the pressure each time is to just bump the number. Once
it went unnoticed for 195 runs while five TLA+ gates proved nothing and still
appeared in the rotation.

ADR-0042 §A1 has carried the same remedy as a "standing recommendation" since
2026-08-02 and it has now been restated twice without being done. It is promoted
here to a tracked item because a recommendation that survives three outages is not
a recommendation.

**The fix:** stop fetching from a mutable third-party URL. Copy the verified jar
once to storage we control and point all carriers at that immutable object, keeping
the SHA-256 check (which then can only fail if OUR copy changed — a real signal
instead of a recurring false alarm). Maven Central was checked as an alternative
immutable source and does not carry this artifact (`org/lamport/tla2tools/1.8.0`
→ 404), so a public mirror is not available; it has to be ours. Options, in order
of preference: an R2 bucket fronted by a public hostname (no credential in CI), or
our own CAS, which is immutable by construction and would dogfood the product.

Deliberately not done inside the 2026-08-24 re-pin PR: that PR unblocks CI, and
adding new public prod storage plus 3 carrier rewrites plus an ADR supersession to
it would make a security-path change large and rushed at the same time.

```backlog
id: B-039
repo: corelink-server
owner: tl
status: done
verify: |
  if grep -rl "releases/download/v.*tla2tools\.jar" .github/workflows scripts > /dev/null; then exit 1; fi
  curl -fsS -o /dev/null --max-time 30 "https://corelink-artifacts.humangr.com/tlaplus/v1.8.0/eabd140a70f49eb9305a3bd3f3df944eddf87e5a90d329789085f8953a80533a/tla2tools.jar"
verify-means: |
  done while NO carrier fetches the jar from the mutable upstream release URL AND
  our own copy still serves. Two-sided on purpose: the first half alone would stay
  green if the mirror vanished, leaving CI pointing at a URL that 404s, and the
  second alone would stay green if someone quietly re-added the upstream fetch.
last-verified: 2026-08-25
```

### B-040 — `| head` under pipefail is the same SIGPIPE defect, left open on purpose

The `| grep -<quiet>` class is closed and gated (`scripts/check_shell_pipeline_safety.py`,
`shell-pipeline-safety.yml`). `| head -n1` has the identical mechanism: `head` exits after
N lines, the producer takes SIGPIPE and exits 141, and `set -o pipefail` reports the
pipeline as failed. 74 such sites exist across 207 pipefail-setting files.

They were NOT swept with the rest, and the reason is a difference in how the defect
surfaces, not a shortage of time:

  - `producer | grep -<quiet> P` is almost always a CONDITION. A wrong answer is
    silent and the caller proceeds down the wrong branch. Two sites on main failed
    OPEN this way, one of them the CTRL-CRED-001 credential scan.
  - `X=$(producer | head -1)` is almost always a VALUE. Under `set -e` a SIGPIPE
    aborts the script at that line. That is loud and self-announcing — a stopped
    job, not a false green.

Silent-wrong is what this repo keeps getting hurt by, so it was closed first and
banned outright. The loud class deserves the same treatment, but it needs a
site-by-site read (some are `|| true`, some are inside `$(...)` whose status is
never tested, and a blanket `head` ban would be wrong), which is why it is a
separate item rather than a hidden allowlist inside the gate.

```backlog
id: B-040
repo: corelink-server
owner: tl
status: done
verify: |
  python3 scripts/check_head_under_pipefail.py --self-test >/dev/null && python3 scripts/check_head_under_pipefail.py >/dev/null
verify-means: |
  closed by replacing the count with a classifier. `head` cannot answer wrongly
  the way `grep -q` can — it can only abort with 141 — so the only question that
  ever mattered is whether anything READS the pipeline's status. Counting
  occurrences answered a different question and is why this sat open: 72 raw
  matches, 65 once comments were anchored out, and the anchor itself was wrong
  (`^[^#]*` cannot reach past a `#` inside quotes, which hid three real sites —
  two `usage()` helpers and a secrets dump).

  `scripts/check_head_under_pipefail.py` walks each line tracking quote state
  and `$(` depth and reports CONSUMED (assignment RHS or bare pipeline — fails),
  ARGUMENT (status discarded by the enclosing command), LITERAL (prose in a
  string) or GUARDED. 18 CONSUMED sites were fixed by removing the pipe rather
  than muffling it — `grep -m<n>`, `cut -c1-N`, or slicing in the shell
  (`${v%%$'\n'*}`) — each proven to yield a byte-identical value. Reproduced
  the abort first (a 200k-line producer into `| head -2` exits 141 and the next
  statement never runs); the two real `usage()` sites exit 0 today only because
  their file is small enough to fit the pipe buffer, which is the latency fuse,
  not a defence. Now enforced on every PR by shell-pipeline-safety.yml, with the
  classifier self-testing 11 cases before its verdict is trusted.
last-verified: 2026-08-24
```

### B-041 — `status.corelink.humangr.com` is advertised in 224 places and serves nothing

DNS has a correct, DNS-only CNAME `status.corelink.humangr.com → hugrl.betteruptime.com`.
The vendor side was never bound: the Better Stack status page had `custom_domain: null`,
so it rejects the SNI and every client gets a TLS `handshake_failure` (alert 40). The
page itself is live at `https://hugrl.betteruptime.com` (HTTP 200, titled
"Human Guardrail / CoreLink status"). The dead hostname appears **224 times** across
`apps/docs/`, `specs/`, `docs/` and `legal/`, including the Trust Center's
"verifiable by you, right now" list.

**Attempted and reverted on 2026-08-24.** Setting `custom_domain` via the API succeeded
and made the vanity host canonical immediately — `hugrl.betteruptime.com` began
301-redirecting to it — but no certificate was ever issued (50 probes over 25 minutes,
all `handshake_failure`). The page attribute `whitelabeled: false` is the likely cause:
custom domains are a plan feature. Net effect of the attempt was to break the ONE URL
that worked, so it was reverted and the vendor page is serving again. Do not re-apply
the binding without first confirming the plan includes custom domains, or the status
page goes dark the moment it is set.

The Trust Center and its three locale mirrors now link to the working vendor URL rather
than to a hostname that resolves and then fails. The remaining ~220 references still
point at the dead host.

**OWNER DECISION, 2026-08-24: do not pay.** The hostname is retired rather than
provisioned. The customer-facing sweep landed in #1290; the CNAME was deleted from the
Cloudflare zone on 2026-08-24 (record `e785399235…`, CNAME → `hugrl.betteruptime.com`,
DNS-only; backup of the record JSON kept with the change) and `dig` now returns nothing.
Everything that probed or defined it moved with it.

Note for anyone revisiting: the prod smoke also demanded HTTP 200 from a SECOND-level
status name under the apex — the kind Universal SSL DOES cover — and no DNS record for it
has ever existed, so that check could only ever fail. Creating it would not help either:
BetterStack still refuses a custom Host without the paid plan, and terminating TLS
ourselves would put the status page behind the infrastructure it exists to report on. The
hostname is deliberately not written out here: naming a dead host in a shipped file is
what the hostname-liveness gate exists to catch, and it caught this note.

```backlog
id: B-041
repo: corelink-server
owner: owner
status: done
verify: |
  [ -z "$(dig +short status.corelink.humangr.com 2>/dev/null)" ] && curl -sS -o /dev/null --max-time 15 https://hugrl.betteruptime.com/
verify-means: |
  done while the retired hostname resolves to nothing AND the vendor status page
  serves. Goes red if the dead name comes back (someone re-created the record) or
  if the page customers are pointed at stops answering.
last-verified: 2026-08-24
```

### B-042 — the only end-to-end proof of the checkout path can run nowhere

`tests/e2e-browser` drives a REAL Clerk session in a REAL browser against the
deployed app. It is the only vantage that can exercise the money path: a headless
FAPI mint is rejected 401 by the prod Worker's Clerk verification, a browser
session is not, so `curl` cannot stand in for it. One of its nine specs,
`03-money-checkout.spec.ts`, is the sole end-to-end proof that a customer reaches
a real Stripe Checkout session — fresh user, DPA click-through, `cs_live_`.

The suite was an ORPHAN: absent from `pnpm-workspace.yaml` AND referenced by no
workflow, so its dependencies were never installed and not one spec had ever run
in CI. Run by hand against prod on 2026-08-24 it PASSES (`403 dpa_required` →
accept → `200` → `checkout.stripe.com/g/pay/cs_live_…`). It had simply never been
asked. The workspace membership and `e2e-browser-prod.yml` close the mechanical
half.

**Closed 2026-08-25 — the money path is now proven end-to-end in CI.** The owner
authorised the credential; both `CLERK_LIVE_PUBLISHABLE_KEY` and
`CLERK_LIVE_SECRET_KEY` are now repo secrets (set from `.env.local`, verified by
name). Dispatching `e2e-browser-prod.yml` (run **32887439740**) drove
`03-money-checkout` against LIVE prod with a real browser Clerk session and
passed: fresh user → `POST /corelink/api/checkout/session` `403 dpa_required` →
DPA click-through accept → `200` → landed on
`checkout.stripe.com/c/pay/cs_live_b140C1rZBxZt…` — **1 passed (16.8 s)**. That is
the whole money path (tier-select → DPA → real `cs_live_` Stripe session)
exercised as a customer, not asserted. `sk_live` can create/delete prod users — a
real blast radius handed to CI on the owner's explicit call; the fixture
DSR-deletes the throwaway user on teardown. Kept `workflow_dispatch`-only;
promoting to a `schedule:` is a separate, deferred decision.

```backlog
id: B-042
repo: corelink-server
owner: owner
status: done
verify: manual
verify-means: |
  open while neither live Clerk credential is a repo secret, i.e. the browser
  suite cannot run in CI at all. Check by hand with
  `gh secret list --repo HuGR-Labs/corelink-server | grep CLERK_LIVE`.
  MANUAL on purpose, and the reason is the point: no GitHub Actions token can
  read the secret list (`GITHUB_TOKEN` gets HTTP 403 on
  `/actions/secrets` — there is no permission that grants it), so an automated
  check here could only ever report the API failure, never the fact. The first
  version I wrote hid exactly that behind a `!`, turning "I could not look" into
  "confirmed". A check that cannot see must say so, not guess. As of 2026-08-25
  both `CLERK_LIVE_*` rows are present and run 32887439740 proved the path green.
last-verified: 2026-08-25
```

### B-043 — enable the audit-drain lease in prod after a concurrency probe

The lease + seal fence that closes the [B-038] audit-chain fork recurrence is now
**enabled + live** in prod. `AUDIT_DRAIN_LEASE_ENABLED = "1"` is set in
`[env.prod].vars` (#1305) — the primary Worker whose container serves the hourly
`POST /_internal/audit/drain` (the drain is centralized against the shared prod D1,
so the flag belongs on the primary, not the regional edge Workers).

What was done + proven (2026-08-25):
1. **Code + migration live.** All 5 prod regions run an image carrying B-038;
   migration `0101_audit_drain_lease.sql` is applied to the prod D1 and the
   `audit_drain_lease` table exists (verified via the D1 REST API).
2. **Acquire/refuse/steal proven on the real D1 engine** (the design's open
   question). A synthetic-partition probe against prod D1: a fresh `INSERT … ON
   CONFLICT … WHERE expires_ms < ?4 RETURNING holder` returned the holder; a
   held-and-unexpired contender got **zero rows** (refused); an expired lease was
   stolen. `RETURNING`-on-`ON CONFLICT` behaves exactly as the single-writer guard
   needs; the D1-REST REAL-number bind does not break the numeric comparison.
3. **Flag actually active, not just deployed.** A same-image rollout does NOT
   restart a live container ([[force-cf-container-roll]]), so the running
   instances kept the pre-flag env — the flag sat deployed-but-dormant. Rolling
   onto a fresh image tag (`6f87d295-r1`, identical container code, #1306) cold-
   started every instance; the CF Containers API confirms all 5 regions now run it.
4. **Live drain healthy with the lease on.** The 03:00Z drain sealed ~200 rows
   (`sealed_total` 59695→59895, `pending` fell) and the duplicate-sequence count
   held at its pre-existing 1505 (historical B-026 forks, quarantined) — **zero
   new forks**.

Honest scope: the design's live *concurrent* two-drain probe was not run from
outside — `corelink-api.humangr.com` is behind CF Access (only the cron's service
binding reaches `/_internal/audit/drain`), and the 1505 historical forks confound
a real-data dup check. Active concurrent-fork prevention is covered by the D1
acquire/refuse/steal proof (step 2) + the merged self-fence unit test; the live
observation confirms no regression. Monitored via `dup_groups` (must stay 1505)
and the audit-archive-lag census.

```backlog
id: B-043
repo: corelink-server
owner: owner
status: done
verify: |
  grep -q 'AUDIT_DRAIN_LEASE_ENABLED = "1"' wrangler.toml
verify-means: |
  done — the lease is enabled in prod ([env.prod].vars) and live on a rolled
  container image. Reopens if the flag is removed from wrangler.toml (which would
  disable the B-038 fork protection). Runtime health is watched out-of-band via
  the prod dup-sequence count (must stay at its historical 1505) and the
  audit-archive-lag monitor, not by this static check.
last-verified: 2026-08-25
```

### B-047 — F2 (edge-served findMissingBlobs) needs the audit seam before the flag

F1 shipped the shadow (#1340) and it measured 43/43 parity, 8.65 s → 2.10 s at
n=100. That result says the edge computes the right SET — it says nothing about
whether the edge may SERVE it, because a shadow that serves nothing cannot fail
open.

`probeMissingAtEdge` (`worker/src/lib/edge_find_missing.ts`) does not do what
the container's `exists_batch`
(`crates/corelink-container/src/storage/r2_s3.rs:1560`) does: N `ReadAttempted`
rows into `audit_outbox`, `ReadDenied` audited before any dispatch, the
fail-closed `AuditFailed` coupling that throws away probe results the code is
already holding, and the `AvailCasGet` / `LatencyCasGetP99` SLIs. Flipping
`EDGE_FIND_MISSING` to serve today would answer a REAPI read surface with its
evidence trail absent, and would keep answering while the audit sink is down —
against the invariant the F0 ADR itself states ("the audit result gates the
response").

Design decided in `docs/design/2026-08-26-adr-edge-find-missing-audit-seam.md`:
the edge probes R2 natively and **awaits ONE** container call
(`POST /_internal/audit/cas-attempted`) that emits the batch through the same
`D1AuditOutboxSink::emit_cas_batch_async`, rather than the Worker writing
`audit_outbox` rows itself. The row is a contract — UUIDv7 id, CloudEvents
payload, `UNIQUE (request_id, event_type)`, and a region trigger that
`RAISE(ABORT)`s on mismatch — and a second TypeScript author of it would drift
while still passing its own tests. Audit call non-2xx/timeout ⇒ discard the
probe results and fall through to the container.

Remaining work: the internal route, the awaited edge call, a test that proves
fallthrough when the audit call fails, the SLI tagging, and only then the flag.
F3 (red-team) then runs against serving code, with audit-sink-down as a named
case.

```backlog
id: B-047
repo: corelink-server
owner: tl
status: open
verify: |
  ! grep -qE '^[^#]*EDGE_FIND_MISSING[[:space:]]*=[[:space:]]*"(on|serve|1)"' wrangler.toml \
    || grep -rq '_internal/audit/cas-attempted' crates/corelink-container/src/routes/
verify-means: |
  open — fails if the edge findMissingBlobs serve flag is turned on in
  wrangler.toml while the container still has no `/_internal/audit/cas-attempted`
  route, i.e. if F2 is flipped before the audit seam it depends on exists. Closes
  when the route lands and the flag is on; `shadow` and `off` are unaffected.
last-verified: 2026-08-26
```

### B-048 — `cargo-mutants` fails any PR that is one commit behind `main`

`mutation-pr.yml:124` fetches the base with `git fetch --no-tags --depth=1 origin
"$BASE_REF"`, then runs `git diff "$base"...HEAD`. The three-dot form needs a
merge-base, and a depth-1 fetch supplies only `main`'s tip commit. The moment
`main` moves past the PR's base — an ordinary condition, not an author error —
there is no common ancestor in the shallow clone and the step dies:

```
fatal: origin/main...HEAD: no merge base
```

Observed on #1359 (2026-08-26): the gate failed in 27 s having mutated nothing,
while the branch was exactly ONE commit behind (#1360, a container repin). The
red is indistinguishable at a glance from a surviving-mutant finding, so it costs
a log read plus a rebase every time it fires — and a rebase is the one operation
that then breaks the OKF anchors ([B-049]).

Fix is `fetch-depth`, not a rebase ritual: deepen the base fetch until a merge
base exists (`--deepen`, or `fetch-depth: 0` on the checkout as `okf_wiki.yml`
already does), or compute the diff two-dot against the fetched tip and accept
that it also shows main-side changes.

```backlog
id: B-048
repo: corelink-server
owner: tl
status: open
verify: |
  grep -qE 'git fetch --no-tags --depth=1 origin "\$BASE_REF"' .github/workflows/mutation-pr.yml
verify-means: |
  open — fails while the depth-1 base fetch is still the thing feeding a
  three-dot diff in mutation-pr.yml. Closes when the fetch is deepened (or the
  diff no longer needs a merge base). The check is deliberately about the FETCH,
  not about a green run: the gate passes whenever the PR happens to sit on main's
  tip, so a green CI run is not evidence the defect is gone.
last-verified: 2026-08-26
```

### B-049 — squash-merge orphans every OKF anchor: 57 of 161 are unreachable on `main`

Reconciling a concept writes `checkpoint_sha` = the current HEAD of the PR
branch. The repo merges by SQUASH, so those commits never land on `main` — the
anchor names a commit that does not exist there. Measured on `main` @ ca4e66de
(2026-08-26): **57 of the 161 concepts carrying a `checkpoint_sha` name a commit
that is not an ancestor of HEAD** — 35% of the wiki. `auth/pat-moat` points at
`94ff5fc4`, a pre-squash commit from #1359, which is the mechanism caught in the
act. A rebase or amend mid-PR does the same thing earlier.

C5 then resolves those concepts against the BASE-REF fallback instead of the
anchor the author advanced. Two consequences, and they pull in opposite
directions, which is why this went unnoticed:

- **In a PR** the fallback is the base branch's tip, so every line-range the PR
  shifts reads STALE. #1359 reported **43 C5 failures** for a 23-line insertion,
  none of them a wrong claim.
- **Locally it stays green** — the orphaned objects are still in the author's
  clone, so `validate_okf` resolves the anchor and prints `0 stale`. The author
  cannot see what CI sees.

The knowledge already exists in two memories
(`okf-anchor-must-name-a-reachable-commit`, `okf-c5-base-ref-stricter-than-local`)
and the workflow still does not enforce it, which is the definition of a gap the
tooling should close rather than a habit to remember harder.

Two changes would end it:

1. **Make `validate_okf` fail on an unreachable `checkpoint_sha`** instead of
   silently resolving it from local objects. A green local run must not be
   achievable with an anchor CI cannot resolve.
2. **Create blob anchors on first reconcile.** A `source_blobs` entry is
   immutable under rebase/squash/cherry-pick. `okf_reanchor.py --blob` only
   ADVANCES an existing entry and never creates one, so a concept that has never
   had a blob anchor for a file stays on the fragile commit anchor forever.

```backlog
id: B-049
repo: corelink-server
owner: tl
status: open
verify: |
  python3 - <<'PY'
  import subprocess,sys,pathlib,re
  bad=0
  for p in pathlib.Path("docs/knowledge").rglob("*.md"):
      m=re.search(r'^checkpoint_sha:\s*"([0-9a-f]{40})"',p.read_text(),re.M)
      if not m: continue
      if subprocess.run(["git","merge-base","--is-ancestor",m.group(1),"HEAD"],
                        capture_output=True).returncode!=0: bad+=1
  print(f"unreachable checkpoint anchors: {bad}")
  sys.exit(0 if bad else 1)
  PY
verify-means: |
  open — exits 0 while at least one concept's checkpoint_sha is not an ancestor
  of HEAD, i.e. while the squash-orphaning is still happening. 57 at filing.
  Closes (this verify then exits 1, forcing the status update) when every anchor
  resolves — which in practice means validate_okf enforces reachability and
  reconciles write blob anchors. Do NOT "fix" this by bulk-rewriting the 57 shas
  on main: that hides the mechanism and it re-orphans on the next squash.
last-verified: 2026-08-26
```

### B-050 — CAS at-rest integrity: nothing verifies a stored object until a client asks for it

**CLOSED 2026-08-26.** `POST /_internal/cas/scrub`
(`crates/corelink-container/src/routes/cas_scrub.rs`) sweeps stored objects and
re-hashes them through the same `verify_content_hash` the read path uses,
reporting `examined` / `skipped_encrypted` / `failed` as three distinct
counters. The BYOK arm follows ADR-S34-001 addendum 2 (per-tenant
classification, not the unreachable per-object `resolve_byok`). The original
finding is kept below as the record.

`R2CasHandler::read` re-hashes every object it serves and compares it against
the requested digest (`crates/corelink-container/src/storage/r2_s3.rs:1333`,
`verify_content_hash` at `:1103`) — it catches R2 bitrot, storage-tier
tampering and historically mis-keyed blobs, and its doc-comment calls it "the
single enforcement point for content-addressing on the durable path" for the
native CAS route, the Bazel REAPI v2 bridge and sccache.

That is the ONLY integrity coverage that exists. No background job, cron or
sweep reads stored objects to check them — `scrub` / `fsck` / integrity-sweep
shaped work is absent from `crates/corelink-container/src/routes/` entirely.
So verification is a sampling function driven by traffic: an object is checked
exactly when a client requests it, and a cold object is never checked at all.

Design in `specs/03_architecture/adrs/ADR-S34-001-cas-read-integrity-before-streaming.md`,
including the 2026-08-26 addendum: for a BYOK-`active` tenant the stored object
is CIPHERTEXT and the R2 key may be a `physical_digest` rather than the
plaintext one, so re-hashing raw bytes would emit false violations against
intact data. The scrubber must resolve each object's plan and count anything
non-`Plaintext` as `skipped_encrypted`, distinct from both `examined` and
`failed`. BYOK is gated-inert today, which is exactly why this is easy to ship
wrong: correct now, silently wrong the day it flips.
Two constraints the ADR fixes: enumerate R2 directly via
`R2S3Client::list_objects_page` (`crates/corelink-container/src/storage/r2_s3.rs:467`
— cursor + `max_keys` clamp of 1000, resumable and bounded per call), NOT
`blob_meta`, which is empty in production and written by no code; and report
OBJECTS EXAMINED, not only failures found, so a run that enumerated nothing is
distinguishable from a healthy one.

```backlog
id: B-050
repo: corelink-server
owner: tl
status: done
verify: |
  grep -rqE '\.route\("[^"]*scrub' crates/corelink-container/src/routes/ \
    && grep -q 'cas_scrub::router' crates/corelink-container/src/main.rs
verify-means: |
  done — passes while the scrub ROUTE is registered AND actually mounted in
  `main.rs`. Both halves are required: a `router()` no caller merges is a
  built-but-unreachable endpoint that would satisfy a route-only grep while
  verifying nothing in production. Anchored on `.route("…scrub…"` rather than
  the bare word, since `scrub` also appears in oci.rs prose about scrubbed
  error messages.
last-verified: 2026-08-26
```

### B-051 — the CAS read path has no SIZE bound; the concurrency permit bounds only the count

**Re-scoped 2026-08-26 by ADR-S34-002.** This item was "streaming CAS reads must
not land before B-050". B-050 shipped, and measuring the read path to plan the
streaming work showed the item was aimed at the wrong thing in both directions.

`CasReadHandler::read` is a SYNCHRONOUS trait method returning an owned
`Vec<u8>` (`crates/corelink-handler-cas/src/handler.rs:41`), served by blocking
a worker thread (`crates/corelink-container/src/storage/r2_s3.rs:1282`). The
client collects the whole body and then copies it —
`.collect().await?.into_bytes().to_vec()`
(`crates/corelink-container/src/storage/r2_s3.rs:236-242`) — so two N-byte
allocations are live at once, three on the BYOK path where `decrypt_body`
(`:821`) yields plaintext while the ciphertext is still held.

`CAS_READ_CONCURRENCY_LIMIT` is 8 per tenant
(`crates/corelink-container/src/routes/cas.rs:343`) and B-052 correctly extended
it to the single GET. But peak heap is `N x object_size` and B-052 bounds only
`N`. The 10 MiB `DefaultBodyLimit` (`crates/corelink-container/src/main.rs:504`)
does NOT cap the other factor: it bounds client-supplied request bodies, and the
server-side ingest paths present no body. `MIRROR_MAX_BLOB_BYTES` is **1 GiB**
(`crates/corelink-container/src/routes/public_mirror.rs:128`), re-checked at
`crates/corelink-container/src/routes/public_pullthrough.rs:103` — so a CAS
object may be three orders of magnitude larger than the request-body limit
implies, and the read path buffers it whole.

Streaming is NOT the answer, on measurement. Full enumeration of
`corelink-cas-prod` (2026-08-26, 22,597 objects, 3.57 GB): p50 **593 B**, 87.9%
≤ 64 KiB, 95.1% ≤ 1 MiB, max 52.3 MB — while the 4.86% above 1 MiB hold 78.3%
of stored bytes. Streaming would buy nothing for 95% of reads and would cost an
async-trait migration of `CasReadHandler` across 11 production implementors and
12 call sites, plus the loss of the read-path digest re-verify.

The fix is a read-side SIZE ceiling — a constant and a check, the same shape as
the permit B-052 added, so that `N x max_size` becomes a number the system chose
rather than one it inherited from the mirror's fetch path. The open question is
the threshold: it must sit above the observed max (52.3 MB) or the change needs
a migration story for objects that are served today and would stop being served.

```backlog
id: B-051
repo: corelink-server
owner: tl
status: open
verify: |
  ! grep -qE 'CAS_READ_MAX_OBJECT_BYTES|MAX_CAS_READ_OBJECT_BYTES' \
      crates/corelink-container/src/routes/cas.rs \
      crates/corelink-container/src/storage/r2_s3.rs
verify-means: |
  open — passes while no read-side object-size ceiling exists, which is the
  defect. Anchored on the constant rather than on prose so it turns red the
  moment a ceiling is introduced, forcing this item closed. Deliberately NOT
  anchored on the old streaming predicate: that one passed both before and
  after the scrubber landed and so could never have gone red.
last-verified: 2026-08-26
```

### B-052 — single CAS GET buffers the whole object with no concurrency guard

**CLOSED 2026-08-26 by #1367.** `handle_read` now declares
`CasReadConcurrencyGuard` against the SAME `read_inflight` pool
`handle_batch_read` uses — a separate pool would have let one tenant hold
`2 x 8` concurrent reads and raised the very ceiling the constant exists to
impose. The original finding is kept below as the record.

`handle_read` (`crates/corelink-container/src/routes/cas.rs:773`) takes
`State`, `Path`, `auth`, `scope`, `headers` and no guard, then buffers the full
object into a `Vec<u8>` and into the response body. Its sibling
`handle_batch_read` declares `CasReadConcurrencyGuard` AHEAD of `body`
precisely so axum reserves the slot before anything is buffered; the
single-object read was left out of that hardening.

Every other read surface is covered on both axes — Turbo GET has
`TURBO_GET_CONCURRENCY_LIMIT` per tenant plus a process-wide
`GLOBAL_TURBO_GET_BUDGET`, Turbo PUT the same, CAS batch read has
`CAS_READ_CONCURRENCY_LIMIT`. Turbo's own comment describes this exact shape as
a bug already fixed once there: "the read path was left asymmetrically open
while the PUT path was hardened"
(`crates/corelink-container/src/routes/turbo_v8.rs:119`).

Severity depends on sharding: routing is
`env.CORELINK_SERVER.idFromName(tenantId)` (`worker/src/index.ts:88`), one
Durable Object and therefore one container per tenant, which makes an unbounded
concurrent-read burst self-inflicted rather than cross-tenant. That reading is
NOT yet proven and changes the priority, not the fix.

```backlog
id: B-052
repo: corelink-server
owner: tl
status: done
verify: |
  awk '/^async fn handle_read\(/,/^\) ->/' \
      crates/corelink-container/src/routes/cas.rs | grep -q 'ConcurrencyGuard'
verify-means: |
  done — passes while `handle_read` declares a concurrency guard among its
  extractors, which is the fix (#1367). Turns red if the guard is ever removed
  or the extractor is reordered after `body`, where it would no longer reserve
  the slot before buffering.
last-verified: 2026-08-26
```

### B-053 — the failover sample floor makes a dead low-traffic region un-failoverable

`RollingMetricsHealthProbe` will not fire ANY degradation signal until it has
seen `min_samples` requests inside the rolling window. The window is
`SUSTAINED_WINDOW_SECS` = 5 s (`crates/corelink-container/src/routes/failover.rs:93`)
and the default floor is 50
(`crates/corelink-container/src/routes/failover.rs:105`), so a region needs
**>= 10 req/s just to be eligible to fail over**. Below that the probe reports
`Healthy` **unconditionally** (`:216`) — a region returning 100% errors at low
volume never trips, and never will, for as long as it stays quiet.

The floor was added for a real bug: three consecutive slow 5xx satisfy all
three triggers at once (rate 100% > 1%, p99 > 300 ms, streak >= 3) and freeze
every write region-wide, amplified because the probe counts the container's own
responses. But the SAME change added hysteresis — `FAILOVER_TRIP_PROBES` = 3
consecutive DEGRADED probe observations before latching (`:110`) — and that
alone already defeats a three-request blip, while still letting sustained
failure through. The floor is a second, blunter layer on top, and what it buys
beyond the hysteresis is small next to the false negative it introduces.

This is worth deciding NOW rather than later, because the knob only just
started reaching production: `FAILOVER_MIN_SAMPLES` was read by the container
but missing from the DO forward-list until #1348, so until today setting it did
nothing. An operator can now lower it during an incident — but only if they
know it exists and know that "healthy" on a quiet region may mean "below the
floor", not "fine".

Decide one of: keep the floor and document the low-traffic blind spot as
accepted; lower the default; scale the floor to observed traffic; or drop it
and rely on the hysteresis that was added alongside it. Found by a peer review
of #1348.

```backlog
id: B-053
repo: corelink-server
owner: tl
status: open
verify: |
  grep -qE '^const MIN_SAMPLES_FOR_FAILOVER_DEFAULT: usize = 50;$' \
    crates/corelink-container/src/routes/failover.rs
verify-means: |
  open — passes while the default floor is still 50, the value this item is
  about. Turns red if the default changes, forcing the decision recorded here
  to be closed out rather than left open against a world that already moved.
last-verified: 2026-08-26
```
