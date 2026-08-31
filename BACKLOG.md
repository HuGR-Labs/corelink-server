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
- **When the status flips to `done`, INVERT the `verify` into a regression
  guard** — it must exit 0 while the world is CORRECT, and fail if someone undoes
  the work. `backlog_verify` requires exit 0 from every item regardless of status;
  `status:` describes, it does not excuse. A `done` item still carrying the
  `open`-polarity check is the nastiest shape this file has: it passes in the PR
  that wrote it (the work is not in the tree yet) and turns the gate RED on the
  merge of the NEXT PR, blaming a change that did nothing wrong. Found on B-060,
  2026-08-29.

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

**DONE 2026-08-27 — F2 is LIVE in prod (`[env.prod]` only).** The seam shipped in
#1359 (container route), #1362 (Worker: consumer key + awaited call + serve
block, 9 tests incl. the one that proves fallthrough when the audit does not
commit), and #1384 flipped `EDGE_FIND_MISSING = "on"`. Proven in production, not
from a green workflow:

- the deployed bundle read back from the Cloudflare API carries
  `EDGE_FIND_MISSING = on` as a `plain_text` binding;
- `/_internal/audit/cas-attempted` answers `204` on a valid body, `401` on a
  wrong key, `422` on a body missing `tenant` — three distinct codes only the
  real handler produces;
- `wrangler tail` on `corelink-prod` captured
  `edge_find_missing_served n=100 edge_ms=1550` for a live n=100 request;
- 12 n=100 requests produced **1200** `corelink.cas.read.attempted` rows in
  `audit_outbox` for the probing tenant — the evidence the edge owes is durable,
  not assumed;
- end-to-end n=100 in prod (MIA colo) is **~2.4 s median** against the F1
  baseline of **8.65 s**.

⚠️ **Note `--search` is a broken instrument here.** `wrangler tail --search
edge_find_missing` returned ZERO bytes while the unfiltered tail on the same
worker captured the line — searching for a string that IS in the URL matched
nothing either. Read the unfiltered stream; a filtered tail that finds nothing
is not evidence of nothing.

Left open deliberately, tracked as B-055: the edge serve path does NOT emit the
`AvailCasGet` / `LatencyCasGetP99` SLIs that `exists_batch`
(`storage/r2_s3.rs:1572`) emits, so those SLOs now under-count the edge-served
fraction. F3 (red-team) still runs against serving code, with audit-sink-down as
a named case.

```backlog
id: B-047
repo: corelink-server
owner: tl
status: done
verify: |
  ! grep -qE '^[^#]*EDGE_FIND_MISSING[[:space:]]*=[[:space:]]*"(on|serve|1)"' wrangler.toml \
    || grep -rq '_internal/audit/cas-attempted' crates/corelink-container/src/routes/
verify-means: |
  done — the flag IS on and the route DOES exist, so the check passes on its
  second arm. It stays as a REGRESSION guard, not a to-do: if anyone ever
  removes the container route while the flag is still on, this flips DRIFTED.
  `shadow` and `off` are unaffected.
last-verified: 2026-08-27
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
status: done
verify: |
  ! grep -qE 'git fetch --no-tags --depth=1 origin "\$BASE_REF"' .github/workflows/mutation-pr.yml
verify-means: |
  done — INVERTED into a regression guard when #1462 fixed the world: it now
  passes while the depth-1 base fetch is ABSENT, and goes DRIFTED if anyone
  reintroduces it. The original polarity was
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

**CLOSED 2026-08-26.** `CAS_READ_MAX_OBJECT_BYTES` = 64 MiB
(`crates/corelink-container/src/routes/cas.rs:156`), enforced by
`R2S3Client::get_capped` (`storage/r2_s3.rs:289`), which reads `Content-Length`
and drops the stream WITHOUT collecting it — an over-size object costs one
round-trip and no heap. Refused as 413 `ObjectTooLarge`, not 404 (which would
tell the client to re-upload bytes we hold) and not 500 (which invites a retry
that cannot succeed). 64 MiB clears the observed 52.3 MB maximum, so nothing
served today stops being served, and the per-tenant worst case becomes
`8 x 64 MiB = 512 MiB` instead of the 8 GiB inherited from the mirror's fetch
cap. The remaining PROCESS-wide half is B-056. Original finding below.

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
status: done
verify: |
  grep -q 'CAS_READ_MAX_OBJECT_BYTES' crates/corelink-container/src/routes/cas.rs \
    && grep -q 'get_capped' crates/corelink-container/src/storage/r2_s3.rs
verify-means: |
  done — passes while the ceiling constant exists AND the read path reaches R2
  through `get_capped`. Both halves are required: the constant alone would be
  a number nothing enforces, and `get_capped` alone could be called with a
  ceiling large enough to be no ceiling. Goes red if either is removed.
last-verified: 2026-08-26
```

### B-056 — the CAS read path has no PROCESS-WIDE byte budget, only a per-tenant one

`CAS_READ_CONCURRENCY_LIMIT` (8) bounds ONE tenant, and B-051 now bounds one
object (`CAS_READ_MAX_OBJECT_BYTES`, 64 MiB). Their product — 512 MiB — is a
per-tenant figure. With N tenants sharing a container the process is still
unbounded: two tenants reading concurrently can claim 1 GiB, which is the whole
container (0.25 vCPU / **1024 MiB**, measured via the Cloudflare Containers API
on 2026-08-26 for all five prod regions).

Turbo already closed exactly this gap and says so in as many words: the
per-tenant `GetConcurrencyGuard` "bounds ONE tenant to
`TURBO_GET_CONCURRENCY_LIMIT x TURBO_BODY_LIMIT_BYTES`, but with N tenants the
process is unbounded" — hence `GLOBAL_TURBO_GET_BUDGET`, a lazily-initialised
process-wide `Semaphore` shared by every `TurboRouteState`
(`crates/corelink-container/src/routes/turbo_v8.rs:374-387`, guard at `:726-733`).
CAS has the per-tenant half and no process-wide half.

The pattern to copy is in the repo, so this is not a design question — it is the
second of the two factors ADR-S34-002 named, and the ADR deliberately scoped
itself to the first. Sizing it needs the same treatment the ceiling got: a
number argued from the 1024 MiB the container actually has.

```backlog
id: B-056
repo: corelink-server
owner: tl
status: open
verify: |
  ! grep -qE 'GLOBAL_CAS_READ_BUDGET|GLOBAL_CAS_GET_BUDGET' \
      crates/corelink-container/src/routes/cas.rs
verify-means: |
  open — passes while no process-wide CAS read budget exists, which is the gap.
  Anchored on the singleton's name (mirroring `GLOBAL_TURBO_GET_BUDGET`) so it
  turns red the moment one is introduced. Deliberately NOT anchored on the
  per-tenant constant, which already exists and would make this read as done.
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

### B-054 — F-001 keyed per-link audit hash: defense-in-depth, deferred as a keyed epoch (WI-S09-007)

The go-live audit (F-001) flagged the per-link audit hash as **un-keyed** BLAKE3
(`link_chain_hash_streaming` uses `Hasher::new()`, not `new_keyed(&key)` —
`crates/corelink-audit-chain/src/chain.rs`), so a party holding the sealed rows
can recompute a self-consistent chain **body**. The audit's remediation offered a
fork: key the per-link hash **or** downgrade the marketing claims to the honest
detect-at-verify posture. Both were ratified.

**The claims downgrade shipped** (PROOF-POINTS + BLOG-POSTS/03 now describe the
linear BLAKE3 chain + JCS + Ed25519-signed head honestly; Object-Lock / Merkle
inclusion+consistency proofs / customer R2 proof-bundle / Rekor are labelled
roadmap). **The keyed per-link is deferred to WI-S09-007**, deliberately, for two
reasons:

1. **CF-6 already carries the load-bearing control.** The chain HEAD is Ed25519-
   signed on every advance and verified fail-closed on drain resume
   (`AUDIT_CHAIN_SIGNING_SEED_HEX`, live in prod). An insider who rewrites a sealed
   row changes `head_hash`, so the old head signature no longer verifies and they
   cannot re-sign without the seed — the drain refuses to extend (SEV-1). Keyed
   per-link is defense-in-depth ON TOP of an already-live tamper-evidence control,
   not the thing that closes the hole.

2. **A naive formula swap is a SEV-0 in prod.** Prod `audit_chain_head` holds
   **365 live per-tenant chain heads** and `audit_outbox` holds **~67k sealed rows**
   (probed 2026-08-26, prod config D1 `d64742ea`). Every sealed link was computed
   un-keyed; flipping `chain.rs` to `new_keyed` makes the daily verifier's recompute
   mismatch **every** existing link → 365 chain breaks. Keying therefore requires a
   proper **epoch cutover** (per-chain algorithm-version checkpoint: verify old
   segments un-keyed, new segments keyed), which belongs with the other WI-S09-007
   audit-immutability work (R2 Object-Lock, write-time chaining that closes the ≤1h
   pre-seal window, the DO verifier cron) — not a per-PR change.

Also record (separate, ops): `AUDIT_CHAIN_SIGNING_SEED_HEX` is committed in
plaintext in `wrangler.toml` (prod vars, ~line 295). A repo reader holds the head-
signing seed, which weakens CF-6's insider guarantee. It should be moved to a
write-only Cloudflare secret and rotated. Tracked here for owner/ops.

Relates to [B-009] and [B-046] (both R2 Object-Lock storage-immutability, platform-
blocked) and the `audit-chain` OKF concept (which honestly documents the un-keyed
per-link + signed head today).

```backlog
id: B-054
repo: corelink-server
owner: tl
status: open
verify: |
  grep -qE 'let mut hasher = Hasher::new\(\);' \
    crates/corelink-audit-chain/src/chain.rs
verify-means: |
  open — passes while the per-link hash is still un-keyed (`Hasher::new()`),
  the state this deferral is about. Turns red the moment someone keys the link
  (`new_keyed`), forcing this item to be closed out with the epoch-cutover design
  recorded rather than left open against a world that already moved.
last-verified: 2026-08-26
```

### B-055 — the edge-served findMissingBlobs emits no CAS SLI

F2 is live: with `EDGE_FIND_MISSING = "on"` the Worker answers
`findMissingBlobs` in-colo and never reaches the container's `exists_batch`
(`crates/corelink-container/src/storage/r2_s3.rs`, `R2CasHandler::exists_batch_inner`). The audit rows still get
written — that seam was the whole point of B-047 — but the SLI emission at
`r2_s3.rs`'s `emit_sli` call inside `exists_batch_inner` (`Sli::AvailCasGet` / `Sli::LatencyCasGetP99`) sits on the
container path only, and the edge serve block emits nothing.

So the edge-served requests are absent from the CAS SLI stream entirely: a
colo-side R2 fault that made every edge probe slow would emit ZERO observations.

⚠️ **Correction 2026-08-27 — the original framing of this item was wrong, and
wrong in the flattering direction.** It said the SLOs "measure a shrinking
sample", which assumes they measure something. They do not. Tracing the sink
before writing the fix found that the CAS/AC SLI stream has no production
consumer at all and never had one — see [B-057]. Emitting an observation from
the edge path as this item originally proposed would have added a second writer
to a stream nothing reads, produced a green PR, and closed a gap that was never
the real one. This item stays open and stays scoped to the edge path, but it is
now BLOCKED on B-057: there is no point instrumenting the edge until the
instrument exists.

Not a launch blocker and deliberately not bundled into F2: emitting an SLI from
the Worker is a different transport (no `emit_sli`, no container metrics
registry) and deserves its own design rather than a lookalike written under a
flag flip. The natural home is the same awaited `/_internal/audit/cas-attempted`
call the edge already makes — it knows `edge_ms` and the outcome, and it is
already the one place the edge and the container agree on what happened.

Relates to [B-047] (which shipped F2 and records this gap in its close-out) and
is BLOCKED by [B-057] (the SLI stream has no consumer).

```backlog
id: B-055
repo: corelink-server
owner: tl
status: done
verify: |
  bash -c 'f=crates/corelink-container/src/routes/audit_cas_attempted.rs
  grep -q "Sli::AvailCasGet" "$f" || { echo "FALHA: a emissao de AvailCasGet sumiu do caminho da costura de auditoria — a regressao desfez o reparo."; exit 1; }
  grep -q "Sli::LatencyCasGetP99" "$f" || { echo "FALHA: AvailCasGet existe mas LatencyCasGetP99 nao — a emissao ficou pela metade; a disponibilidade e observada e a latencia nao."; exit 1; }
  grep -q "edge_ms" "$f" || { echo "FALHA: a emissao existe mas nao carrega edge_ms — a latencia observada seria a do container, nao a da borda."; exit 1; }
  echo "done: a costura /_internal/audit/cas-attempted emite AvailCasGet e LatencyCasGetP99 com a janela edge_ms"'
verify-means: |
  done — o caminho servido pela borda deixou de ser invisivel para os SLOs. O
  `serveEdgeFindMissing` manda `edge_ms` na chamada da costura de auditoria que
  ja fazia, e o handler emite as duas SLIs a partir dela (#1440).

  **A polaridade foi invertida junto com o status, de proposito.** A versao
  `open` passava enquanto `AvailCasGet` estivesse AUSENTE. Mantê-la depois do
  reparo deixaria o item verde no PR que o consertou e vermelho no merge
  seguinte — foi exatamente o que aconteceu: a `main` ficou DRIFTED em B-055 no
  instante em que o #1440 entrou.

  As tres condicoes sao separadas porque falham por motivos diferentes e o
  reparo difere: emissao ausente e regressao; `AvailCasGet` sem
  `LatencyCasGetP99` e emissao pela metade, que observa disponibilidade e nao
  latencia; e emissao sem `edge_ms` mediria o relogio do container num pedido
  que o container nunca viu. Uma condicao unica esconderia as duas ultimas.

  **O que este comando NAO decide, e o item nao fecha isso:** se alguem LE essa
  SLI. O [B-057] segue aberto — o fluxo de SLI de CAS/AC nao tem consumidor de
  producao e nunca teve. A observacao agora existe; continuar sem leitor e o
  problema do B-057, nao deste item. Registrar a distincao importa: a versao
  original deste item foi escrita supondo que os SLOs mediam alguma coisa, e
  essa suposicao era falsa.
last-verified: 2026-08-31
```


### B-058 — the OKF auto-reconcile bot has NEVER executed, and it is pointed at the wrong machine

**Corrected 2026-08-29.** The first version of this item, which I wrote, was
wrong in three ways and every one came from the same mistake: I read the
`conclusion` column of the run list and the `runs-on:` string, without ever
opening a step or the runner registry. Recording the corrections rather than
quietly editing them, because the wrong reading is the interesting part.

- It said "the **hourly** workflow … the schedule is firing on time". There is
  **no schedule**. The triggers are `push` on `main` (paths `crates/**`,
  `worker/**`, `apps/**`, `migrations/**`) and `workflow_dispatch`.
- It said "It is a **REGRESSION**, not a lane that never worked: of the last 60
  runs, 56 succeeded". Those 56 are runs where `stale_count == 0` and the claude
  step is **`skipped`** — 56 no-ops, not 56 executions. Opening the steps of
  three of them shows `Install Claude Code CLI: skipped` /
  `Run the okf-reconcile skill: skipped`. **The bot has never once executed.**
- It treated `runs-on: corelink` as merely "self-hosted, so fine". `corelink` is
  the **ephemeral CF container fleet**, and that is the root cause, not a detail.

The real diagnosis: `claude` authenticates from the CLI logged in on the
founder's Mac (`~/.claude`); an ephemeral container has no such directory. The
`npm install -g` step reports success, its bin never reaches the next step's
PATH (hence `claude: command not found` / exit 127), and even if it did there
would be no login. The lane is aimed at a machine that structurally cannot run
it.

**Decision (owner, 2026-08-29): the OKF robot stays LOCAL/MANUAL for now** —
`scripts/okf-reconcile-local.sh` — and is not automated in CI. So the `push`
trigger is removed in the same change as this correction: it can only ever fire
to fail, and a chronic red nobody acts on trains everyone to ignore the signal.
`workflow_dispatch` is kept, so the lane is one click away if the decision
changes.

If it ever returns to CI, the fix is four lines and is recorded here so the
diagnosis is not re-derived: run on `[self-hosted, macOS, X64]` (the persistent
Mac — the runner process runs as the same user, so it sees the CLI auth); add
`$HOME/.local/bin` to `$GITHUB_PATH`; drop the `npm install -g` (installing over
the owner's binary is needless risk); drop the `env: ANTHROPIC_API_KEY` block
(the secret does not exist and the auth is the CLI's, so it advertises a
dependency that is not real).

Standing cost of leaving it manual: every PR that touches a cited file needs a
hand re-anchor. That was paid three times on 2026-08-29 alone (#1408, #1411,
and the OKF half of the read-ceiling work).

```backlog
id: B-058
repo: corelink-server
owner: owner
status: open
verify: |
  ! grep -qE '^\s+push:' .github/workflows/okf-autoreconcile.yml
verify-means: |
  open — passes while the lane is dispatch-only, i.e. while the OKF robot is
  still the manual/local tool the owner chose and CI is NOT relied on to
  reconcile. Goes red the moment an automatic trigger is added back, which is
  exactly when this item must be revisited: re-arming it without the four-line
  fix below just restores a lane that fails 100% of the time. Deliberately NOT
  anchored on run history — the previous version of this verify was, and it
  would have read "green" off runs whose claude step never ran.
last-verified: 2026-08-29
```

### B-057 — the CAS/AC SLI stream is a dead end: no consumer, no latency, no bound

Tracing where an `AvailCasGet` observation actually goes, before writing the
edge-side emit for [B-055], found that it goes nowhere. Three defects, one root
cause — the "production wiring" the code promises was never built.

**1. No consumer.** The production CAS handler is constructed with
`let sli = Arc::new(InMemorySliObserver::new())`
(`crates/corelink-container/src/storage/r2_s3.rs`, in `build_r2_cas_handler_from_env`; the AC twin in `build_r2_ac_handler_from_env`),
whose doc-comment calls it a "capture-everything in-process observer for tests +
apps/server wire-up" and whose trait doc says "Production wiring adapts this to
the `corelink-slo::BurnRateCalculator` input stream + prometheus histogram
registry". That adaptation does not exist. Every call of `snapshot()` / `count()`
in the entire workspace is inside a test, and `corelink-container` references
`corelink_slo` exactly ONCE — `tenant_quota.rs:59`, for PagerDuty, not the burn
rate. So `AvailCasGet`, `AvailCasPut`, `LatencyCasGetP99`, `LatencyCasPutP99`,
`AvailAcLookup`, `LatencyAcHitP99` and `CorrectnessCas` are computed by nothing,
in every region, on every path.

**2. No latency.** `emit_sli` (`r2_s3.rs`, `impl R2CasHandler`) passes `latency_us: 0` for
both the availability AND the latency SLI, and so does every other CAS/AC call
site — 11 of the 12 `SliObservation::new` call sites in the workspace hard-code
a zero. The field is documented as "wall-clock latency of the handler entry".
The one site that measures anything real is the Stripe webhook dispatcher
(`crates/corelink-stripe-real/src/webhook_dispatch.rs:847`,
`clock.observe_latency_seconds`). A p99-latency SLI whose every sample is 0 is
not a loose measurement — it is not a measurement.

**3. No bound.** `InMemorySliObserver` is a `Mutex<Vec<SliObservation>>` that is
only ever pushed to. Both instances are built at process start, not per request
(`routes/cas.rs::build_handlers` is called from `routes.rs`, and
`routes/public_mirror.rs::build_state_from_env` calls it again for the `_public`
moat), so the Vec
grows monotonically for the life of the container: ~16 bytes per observation,
two observations per CAS operation, never drained. Today's containers recycle
often enough on deploys that this has not surfaced as an incident, which is
exactly why it needs a bound rather than luck — a long-lived instance in a quiet
region is the case that finds it.

**Why this is filed and not fixed inline:** the fix is a design decision about
the observability transport (drain to the existing structured-log stream that
Workers Logs already retains? a bounded ring buffer plus a scrape route? wire
`BurnRateCalculator` for real?), and the honest interim is a BOUNDED sink, since
an unbounded buffer nobody reads is strictly worse than dropping. Whatever is
chosen must also thread a real `latency_us`, or defect 2 survives the fix for
defects 1 and 3.

**Not a customer-facing outage** — no request fails because of this. It is an
alerting and capacity blind spot: the CAS availability SLO cannot page anyone,
so a partial R2 degradation is only visible if a human happens to look.

⚠️ **Line-number provenance.** The `file:line` coordinates this item shipped with
in #1407 were written from a worktree sitting on a stale branch, 297 lines behind
`main` in `r2_s3.rs`, so every one of them was off by roughly 110 lines. The
CLAIMS were verified against real code; the COORDINATES were not. They are
replaced here with SYMBOL references — a symbol does not drift when a file grows
above it, and it survives the rebase that a line number does not.

Relates to [B-055] (which this blocks) and [B-047].

```backlog
id: B-057
repo: corelink-server
owner: tl
status: open
verify: |
  grep -q 'let sli = Arc::new(InMemorySliObserver::new());' \
    crates/corelink-container/src/storage/r2_s3.rs \
    || grep -q 'SliObservation::new(avail, is_error, 0)' \
      crates/corelink-container/src/storage/r2_s3.rs
verify-means: |
  open — passes while EITHER half of the dead end survives: the production CAS
  handler still wired to the in-process capture buffer (no consumer, no bound),
  or `emit_sli` still hard-coding `latency_us: 0` (no latency). Deliberately an
  OR: fixing the transport while leaving every sample at zero would look like a
  closed item and leave the latency SLO exactly as blind as it is today.
last-verified: 2026-08-27
```

### B-059 — the OKF citation regex cannot see abbreviated citations, so a re-anchor can be wrong and green

`CITE_RE` in `scripts/validate_okf.py:108` requires a NON-EMPTY path, so
`_collect_cites` never yields a citation written in the abbreviated
continuation form — a bare `` `:803-831` `` following a full-path citation in
the same sentence, which the wiki uses freely to avoid repeating a long path.

Those citations are therefore invisible to C5/C6/C6b. They are never checked
for freshness, never checked for existence, and never counted. A concept can
carry a citation pointing at the wrong lines while `okf-wiki-validation`
reports the profile valid.

Measured on #1410: shifting `crates/corelink-container/src/main.rs` by one line
moved 21 full-path citations, all corrected — and left **6 abbreviated ones**
wrong (`crates/billing-pipeline.md:119` → `:803-831`, `:881-891`, `:464-489`;
`launch/money-path.md:72` → `:824-862`, `:903-908`, `:875-886`), each needing
the same `−1`. The gate was green on that PR the whole time, and because the PR
also re-pins the `main.rs` blob anchor, the gate would have gone PERMANENTLY
blind to them: the reference point moves past the drift that was never seen.

That is the house's signature failure — the change asserts it checked, the gate
agrees, and it is wrong — so the fix is not "remember to grep for `:N-N`". Make
the abbreviated form a first-class citation: resolve it against the nearest
preceding full-path citation in the same concept, and validate it like any
other. Until then, every re-anchor that touches a file cited in abbreviated
form is silently unverified.

Relates to [B-058] (the OKF reconcile backlog) and to the C5 freshness gate
generally.

```backlog
id: B-059
repo: corelink-server
owner: tl
status: open
verify: |
  grep -qF '(?P<path>[A-Za-z0-9._/\-]+):(?P<l1>' scripts/validate_okf.py
verify-means: |
  open — passes while the citation regex still requires a non-empty path, i.e.
  while abbreviated `:N-M` citations are invisible to the OKF gates. Closes when
  the pattern admits a path-less citation (resolved against the preceding
  full-path one) and the gates validate it like any other.
last-verified: 2026-08-29
```

### B-060 — the DSR registry→migrations mirror gate

CF-1 (`every_migrated_tenant_keyed_table_is_classified`,
`crates/corelink-container/src/routes/dsr/adapter_d1.rs`) only walks
**migrations → registry**: a table on disk that nobody classified fails the
build. Nothing walks **registry → migrations**, so a name in a DSR registry that
no migration creates is structurally invisible — and because `erase()` deletes
in a bare loop with no transaction, such a phantom splits an Art.17 sweep in
half at its own position. That is not theoretical: `devenv_monthly_vcpu`
(#1405, reverted in #1410) sat at entry 18 of 41 in `TENANT_ID_TABLES`,
immediately before the three `byok_*` tables.

The mirror assertion landed with this item's own PR (stacked on #1410) and closes it.
It is filed anyway rather than shipped silently, because the same PR corrects a
`dsr-erasure` claim that the forward gate makes a future tenant-keyed migration
impossible to land unclassified — a claim that is false for reasons BEYOND the
phantom, and the reasons deserve to outlive the PR:

- `find("CREATE TABLE")` is case- and whitespace-sensitive;
- `KEY_COLS` is a closed list of six, so a table scoped by any other column is
  not seen as tenant-keyed at all;
- the test reads only `migrations/d1/`, while `migrations/*.sql` at the root
  also declares tables carrying `tenant_id`.

So the forward direction is weaker than it reads even with the comment-stripper
bug fixed, and the mirror is the check that does not depend on the heuristic:
it asks only whether a name the code will DELETE corresponds to a table that
EXISTS.

### The lesson this item exists to carry

Three of the four defects found in #1410's review were **acquisition** failures:
a stale worktree, a truncated read, a gate that never emitted the citation. The
fourth was different and no gate can catch it.

The PR body stated, in writing, that the CF-1 `debug_assert!` is compiled out of
the release container. The `dsr-erasure` concept asserted, in a sentence
depending on exactly that, that an unaccounted-for table "can never run an erase
that then attests `VerifiedComplete`". Both were on screen at the same time. The
fact was acquired, written down, and then did not **propagate** to the claim
whose truth rested on it — its sibling, in the same paragraph of the same file
the same PR was already editing.

No linter knows that sentence X in the wiki depends on fact Y in a commit
message. The only antidote is a habit, applied whenever you establish that
something does NOT hold — compiled out, not wired, never called, no consumer:
immediately ask **"what in this repo asserts the opposite, or rests on it?"**
and sweep. Do it at the moment of the finding, not at the end, because that is
when the dependency is still in view.

Same shape as [B-055], where the item's own premise assumed the thing it wanted
to fix existed.

Relates to [B-057] and the `compliance/dsr-erasure` concept.

```backlog
id: B-060
repo: corelink-server
owner: tl
status: done
verify: |
  grep -q 'fn every_registry_table_is_actually_created_by_a_migration' \
    crates/corelink-container/src/routes/dsr/adapter_d1.rs
verify-means: |
  done — the mirror assertion is in the tree, so this check now FAILS, which is
  the closure signal. It stays as a regression guard: deleting the assertion
  flips the item back to DRIFTED rather than quietly removing the only check
  that does not depend on the SQL heuristic.
last-verified: 2026-08-29
```

### B-110 — cinco lanes de CI presas em runner GitHub-hosted, que está billing-blocked

`cas_foundation.yml`, `coverage.yml`, `ffi-matrix-ci.yml`, `mutation-nightly.yml`
e `semgrep.yml` somam **1.057 execuções e ZERO sucessos**. A causa é a mesma nas
cinco e é estrutural, não flake: cada uma declara um runner **GitHub-hosted**
(`ubuntu-latest` ou o larger-runner `ubuntu-x64-4core`), e minutos hosted estão
billing-blocked nesta org desde 2026-08-24.

Duas assinaturas distintas, ambas explicadas por isso: `coverage` e
`mutation-nightly` aparecem `cancelled` com `runner_name = NONE` — o job nunca
recebeu máquina; `cas_foundation`, `semgrep` e `ffi-matrix-ci` chegaram a rodar
enquanto ainda havia crédito e falharam no conteúdo (`Run Semgrep`,
`Install valgrind`), mas hoje nem chegam lá.

`semgrep` merece nota: além do job hosted, ela declara
`runs-on: [self-hosted, Linux, X64]`, e **não existe runner Linux self-hosted
neste repo** — o inventário é 14-15 `corelink` (frota efêmera CF) e 5
`self-hosted,macOS,X64,mac,corelink-builder`. Esse job jamais pegaria box mesmo
com crédito.

Não é conserto de CI: ou o owner libera gasto hosted, ou as cinco migram para a
frota self-hosted (e `ffi-matrix-ci` já é reconhecidamente parked), ou são
apagadas. Nenhuma dessas é decisão de higiene.

```backlog
id: B-110
repo: corelink-server
owner: owner
status: open
verify: |
  bash -c 'for f in cas_foundation coverage ffi-matrix-ci mutation-nightly semgrep; do
    grep -qE "runs-on: *(ubuntu-latest|ubuntu-x64-4core)" ".github/workflows/$f.yml" || exit 1
  done'
verify-means: |
  open — passa enquanto TODAS as cinco lanes nomeadas ainda apontam para um
  runner hosted, que é o bloqueio. Vira vermelho assim que QUALQUER UMA for
  migrada ou apagada, forçando a revisão do item em vez de deixá-lo cobrir uma
  lane que já saiu do grupo. O laço é sobre as cinco de propósito: um verify que
  olhasse só uma seria predicado mais fraco que cinco itens separados.
last-verified: 2026-08-30
```

### B-111 — aquisição de certificados Apple/Windows (o resto dos "secrets ausentes" não era isso)

Reescrito 2026-08-30 depois que o owner respondeu. A primeira versão deste item
tratava quatro lanes como um bloqueio só — "os secrets não existem". Ler os
nomes exigidos **por lane**, em vez da lista como bloco, mostrou três coisas
distintas, com donos e horizontes diferentes:

**(a) Aquisição — `notarize-macos` e `sign-windows`.** Não é secret esquecido:
o owner **não possui** os certificados. Notarização exige conta Apple Developer
paga (`APPLE_DEVELOPER_ID`, `APPLE_DEVELOPER_ID_PASSWORD`,
`APPLE_NOTARIZATION_USERNAME`, `APPLE_NOTARIZATION_PASSWORD`, `APPLE_TEAM_ID`) e
Authenticode exige certificado emitido por CA (`WINDOWS_CODE_SIGNING_CERT`,
`WINDOWS_CODE_SIGNING_PASSWORD`). Horizonte de compra, não de configuração —
dizer "secret pendente" sugeriria que alguém só esqueceu de colar.

**(b) RESOLVIDO em 2026-08-30 — `terraform-drift` e `sign-linux`.** O owner
autorizou o mint e a guardiã de merge emitiu os sete: `CF_CLIENT_ID`,
`CF_CLIENT_SECRET`, `TF_BACKEND_BUCKET`, `TF_BACKEND_ENDPOINT`,
`GPG_PRIVATE_KEY`, `GPG_PRIVATE_KEY_PASS`, `GPG_KEY_ID` — o repositório foi de
11 para 18 secrets. Este item deixa de cobrir essas duas lanes: o bloqueio de
credencial acabou.

O que NÃO está provado, e por isso não é fechamento: nenhuma das duas foi
executada desde o mint, então "tem o secret" ainda não é "fica verde". O
`sign-linux` continua atrás do `release-cli` de qualquer forma ([B-112]). A
próxima execução decide, e ela é barata — o `terraform-drift` é um
`plan -detailed-exitcode`, leitura pura.

**(c) Não é bloqueio de secret nenhum — `release-cli`.** O único secret que ela
usa é `CORELINK_CLI_RELEASE_TOKEN`, e **ele já existe**. A falha é o step
`cargo zigbuild`. Ver [B-112].

Consequência enquanto (a) e (b) durarem: sem detecção de drift de
infraestrutura, e nenhum binário de release assinado em plataforma alguma.

```backlog
id: B-111
repo: corelink-server
owner: owner
status: open
verify: |
  bash -c 'have=$(gh api repos/HuGR-Labs/corelink-server/actions/secrets --jq ".secrets[].name" 2>/dev/null)
  [ -n "$have" ] || exit 0
  for s in APPLE_DEVELOPER_ID APPLE_TEAM_ID APPLE_NOTARIZATION_PASSWORD WINDOWS_CODE_SIGNING_CERT WINDOWS_CODE_SIGNING_PASSWORD; do
    echo "$have" | grep -qx "$s" && exit 1
  done
  exit 0'
verify-means: |
  open — passa enquanto NENHUM dos cinco secrets de AQUISIÇÃO existir, que é o
  bloqueio restante (Apple + Windows). Os do grupo (b) saíram da lista porque
  foram emitidos em 2026-08-30 e mantê-los aqui deixaria o item vermelho para
  sempre por uma razão já resolvida.

  Este verify já provou o próprio valor: escrito com os sete do grupo (b) na
  lista, ficou DRIFTED minutos depois, porque o mint aconteceu enquanto o item
  era redigido. Foi o gate que avisou, não uma releitura.

  Sai 0 quando a API não responde, para não ler timeout como "provisionado".
last-verified: 2026-08-30
```

### B-112 — a cadeia de release nunca produziu um artefato verde, e as lanes a jusante herdam isso

`release-cli.yml` dispara em tag `cli-v*` (as tags existem: `cli-v0.1.0`,
`cli-v0.1.1`) e morre no step `Build (cargo zigbuild) — Linux + Windows`, na
frota macOS `corelink-builder`. Como `sign-linux`, `sign-windows` e
`notarize-macos` disparam por `workflow_run` **atrás dela**, as três aparecem
com `runner_name = NONE` na mesma data (2026-05-29) — não são três defeitos,
são um.

`release-slsa3.yml` tem exatamente 1 execução (evento `release`, e existe 1
release publicado) e os logs já expiraram, então a causa dela é a única deste
grupo que permanece **não diagnosticada** — registrada como tal em vez de
suposta.

**`cosign-sign.yml` foi APAGADA em 2026-08-31 (WP-E).** A leitura anterior deste
item — "não é defeito, apenas trigger-starved; não migrar, não apagar" — parou
no gatilho e não abriu o corpo do workflow. É verdade que não havia tag `v*`
(confirmado: 0 contra 4 `cli-v*`), mas isso era a PRIMEIRA de cinco paredes, e as
outras quatro tornavam a lane inexecutável em qualquer cenário. Ver [B-118], que
carrega o inventário completo. Resumo: ela empurrava para
`ghcr.io/HumanGuardrail/*` com o `GITHUB_TOKEN` de um repo em `HuGR-Labs` (e o
próprio arquivo trazia um `TODO(org-rename)` admitindo isso), construía o
`Dockerfile` da raiz — que é o CONTÊINER — e o rotulava `corelink-worker`,
carregava `ZONE_ID_PLACEHOLDER` no payload de deploy, e seu passo de atestação
SLSA era um `echo` declarado como stub. O waiver do owner era real e sua razão
técnica era correta; ele apenas protegia uma lane que não podia funcionar.

**A raiz NÃO é falta de secret** — correção de 2026-08-30. O `release-cli` usa
um único secret, `CORELINK_CLI_RELEASE_TOKEN`, e ele já existe. A falha é
defeito de build no `cargo zigbuild`, e é a única do grupo que não depende de
credencial nenhuma.

A ordem importa, porque muda o que adianta consertar primeiro:

```
release-cli (cargo zigbuild) quebrado
   └─ sign-linux      → destrava quando o GPG for mintado (B-063 grupo b)
   └─ sign-windows    → bloqueado por AQUISIÇÃO de certificado (B-063 grupo a)
   └─ notarize-macos  → bloqueado por AQUISIÇÃO de conta Apple (B-063 grupo a)
```

Consertar o `release-cli` é **condição necessária das três**: mesmo com os
certificados Apple e Windows em mãos, nada seria assinado, porque o artefato
nunca chega a ser produzido. Investigar o que o `zigbuild` reclama antes de
propor conserto — não presumir toolchain ausente.

```backlog
id: B-112
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'grep -q "cargo zigbuild" .github/workflows/release-cli.yml || exit 1
  test -e .github/workflows/cosign-sign.yml && exit 1
  grep -q "GITHUB_REPOSITORY" .github/workflows/release-slsa3.yml || exit 1
  exit 0'
verify-means: |
  open — decide as três alegações estruturais que ainda sustentam o item:

  1. o `release-cli` ainda constrói por `cargo zigbuild`, ou seja a raiz da
     falha de build NÃO foi trocada (e ela continua sem diagnóstico: os logs
     das 8 execuções expiraram com HTTP 410);
  2. a `cosign-sign.yml` NÃO existe mais — se alguém a recriar sem refazer a
     análise das cinco paredes do [B-118], este item fica vermelho de propósito;
  3. o `release-slsa3` ainda baixa os sujeitos de `${GITHUB_REPOSITORY}`, isto é,
     deste repo — enquanto o `release-cli` publica no repo `corelink-cli`
     SEPARADO. Essa é a raiz compartilhada das duas lanes: ninguém decidiu onde
     os releases do CLI moram.

  **A catraca do `corelink-cli` NÃO mora aqui**, e a tentativa de instalá-la neste
  item foi retirada. Ela exige que o repo de publicação esteja repontado para
  `HuGR-Labs/corelink-cli` — mas o repontamento é trabalho de OUTRO PR, e uma
  catraca instalada antes de existir o que ela ratcheia é vermelho verdadeiro sobre
  uma condição que este PR não causou e não pode consertar. Medido no branch deste
  PR: `release-cli.yml` carrega **cinco** ocorrências do literal, das quais **duas
  são uso executável de verdade** (`gh release create --repo …` e
  `gh release upload --repo … --clobber`). A lane publica mesmo num repo que
  responde 404 — e é o PR do repontamento que paga essa dívida, com a catraca
  junto.

  O que este comando NÃO decide, e é a parte maior: POR QUE a matriz de build
  reprova. Um verify sobre histórico de execução leria os logs expirados como
  verde, então ele não ancora ali. O próximo passo é uma tag descartável e ler o
  erro real — a evidência lateral aponta a perna `x86_64-pc-windows-gnu`, a única
  sem artefato no release `cli-v0.1.0`.
last-verified: 2026-08-31
```

### B-113 — seis lanes self-hosted sem sucesso, cada uma por um motivo próprio

Sobram seis do levantamento das 20, e elas NÃO compartilham raiz — agrupá-las
num predicado só produziria um portão dominado, então ficam nomeadas aqui com o
que foi lido de cada uma:

- **`nightly.yml`** (103 runs, cron ativo, falhou 2026-08-29): morre em
  `Install cargo-mutants (pinned, prebuilt)` no `corelink-builder-5`. A lane
  ainda queima hoje.
- **`terraform-drift.yml`**: coberta por [B-111], listada aqui só para a
  contagem das 20 fechar.
- **`sbom.yml`** (16 runs, 2026-08-25): morre em
  `Generate SBOM (CycloneDX 1.5+ JSON)` no `corelink-builder-4`.
- **`buck2-starter-ci.yml`** (77 runs, 2026-08-16): morre em
  `Install Buck2 latest stable` na frota `corelink`. Também declara
  `[self-hosted, Linux, X64]`, que não existe aqui.
- **`fuzz-nightly.yml`** (69 runs, 2026-08-02): `cancelled` no
  `corelink-builder` — cancelamento, não falha de step, então a causa provável
  é timeout/concorrência e **não está confirmada**.
- **`endurance-2h-nightly.yml`** (19 runs, 2026-06-03) e
  **`load-test-nightly.yml`** (3 runs, 2026-05-31): `runner_name = NONE` na
  frota `corelink`. Ambas datam de antes da frota atual; se voltariam a pegar
  box hoje é **não verificado** — provar exige disparar um teste de carga real,
  que não cabe num PR de higiene.
- **`billing-health-daily.yml`** (7 runs, cron ativo, falhou 2026-08-29): morre
  em `Check billing health` num `cf-runner`. Ainda queima hoje.

Três dessas (`nightly`, `billing-health-daily`, e o `terraform-drift` do B-063)
são as únicas do levantamento inteiro que ainda produzem vermelho diariamente;
as outras já não disparam. Priorizar por isso, não por volume histórico.

```backlog
id: B-113
repo: corelink-server
owner: tl
status: open
verify: manual
verify-means: |
  manual e com prazo: as seis têm causas distintas e nenhuma delas é decidível
  por um comando sobre o repositório — `Install cargo-mutants`, `Install Buck2`
  e `Generate SBOM` falham por estado da máquina/rede, `fuzz-nightly` cancela
  sem step, e as duas de carga precisariam de execução real para saber se ainda
  pegam box. Um verify sintético aqui seria teatro. O decaimento de 14 dias é o
  que impede este item de virar gaveta.
last-verified: 2026-08-30
```

### B-061 — o roadmap de remediação é o único artefato sem `verify`, e já vazou número errado

`docs/campaigns/remediation/ROADMAP.md` é o artefato de controle da campanha: define
ordem de merge, declara disjunção de work-packages e congela contratos de descarte.
É também **o único documento operacional deste repo sem um `verify`** — todos os 60
itens deste arquivo carregam um comando que decide se a própria alegação ainda vale, e
o `backlog_verify.py` fica vermelho em DRIFTED. O roadmap não tem nenhum, então **sua
deriva é invisível por construção**.

Isso não é hipótese. Quatro revisores independentes (2 Opus, 2 Sonnet, dois deles
adversariais) encontraram, em uma passada:

- **Três números publicados que não reproduzem.** "5 lanes com 1.057 execuções e zero
  sucessos" — a população real é **19 lanes / 1.539 execuções**; "13 conceitos com
  âncora órfã" — são **63 de 164**, e o 13 contava só os dois SHAs mais frequentes;
  "`tier.rs` tem 6 tiers" — o enum tem **11 variantes**, o 6 era doc-comment obsoleto.
- **Um deles já vazou para fora do documento**: o "1.057" foi repetido no corpo de um PR
  já mergeado, como argumento para escolher um piso de cobertura.
- **Um critério de conclusão que certificava conserto de 5% como pronto**: o WP-5 mandava
  checar 2 SHAs órfãos; existem 32 distintos.
- **Três de oito WPs mandavam criar branch que já existia** — dois mergeados, um em voo.

A ironia é o achado: o documento nomeia *"a prosa é excelente e a implementação não
corresponde"* como o formato de defeito dominante desta casa, e é exatamente o que ele
faz consigo mesmo.

**O `verify` abaixo re-deriva os dois números estruturais do documento** (âncoras OKF
inalcançáveis e lanes com zero sucesso) e falha se o texto divergir da medição — o
mesmo mecanismo que este arquivo aplica a todo o resto.

**Polaridade deliberada:** passa **enquanto** o roadmap declarar números que batem com a
realidade medida. Vira DRIFTED quando alguém consertar as âncoras sem atualizar o
documento, ou quando o documento afirmar um número que a medição não sustenta.

```backlog
id: B-061
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'set -o pipefail
  git rev-parse --is-shallow-repository | grep -qx false || { echo "FALHA: clone raso — ancestralidade nao verificavel. O workflow precisa de fetch-depth: 0."; exit 1; }
  tmp=$(mktemp -d); trap "rm -rf $tmp" EXIT
  git ls-tree -r --name-only HEAD -- docs/knowledge/ | grep "\.md$" | grep -vE "/(index|log)\.md$" > "$tmp/files"
  [ -s "$tmp/files" ] || { echo "INDETERMINADO: nenhum conceito encontrado em docs/knowledge — arvore inesperada."; exit 1; }
  grep -H -m1 -oE "^checkpoint_sha:[[:space:]]*\"?[0-9a-f]{8,40}" $(cat "$tmp/files") 2>/dev/null \
    | sed -E "s|^([^:]+):.*[^0-9a-f]([0-9a-f]{8,40})$|\1\t\2|" > "$tmp/pairs"
  git rev-list HEAD | sort > "$tmp/reach"
  cut -f2 "$tmp/pairs" | sort -u | sed "s|^|^|" > "$tmp/pat"
  grep -hoE -f "$tmp/pat" "$tmp/reach" | sort -u > "$tmp/hit"
  n=$(cut -f2 "$tmp/pairs" | grep -vxF -f "$tmp/hit" | wc -l | tr -d " ")
  [ "$n" -gt 0 ] || { echo "FALHA: zero ancoras inalcancaveis — o apodrecimento acabou, feche o item."; exit 1; }
  blk=$(grep -A1 -F "if not git.is_ancestor(str(prev_sha), args.base_ref):" scripts/validate_okf.py) || {
    echo "INDETERMINADO: o bloco de ancestralidade do C5b nao foi encontrado em validate_okf.py."
    echo "Alguem refatorou o caminho. NAO estou concluindo nada — olhe a mao e reescreva este verify."
    exit 1
  }
  case "$blk" in
    *continue*) : ;;
    *) echo "FALHA: o C5b nao PULA mais ancora inalcancavel — a prevencao existe, feche o item."; exit 1 ;;
  esac
  echo "aberto: $n ancoras inalcancaveis e o C5b ainda PULA em vez de reprovar (prevencao ausente)"'
verify-means: |
  open — existem âncoras inalcançáveis E o `validate_okf.py` ainda passa mesmo
  assim. As duas metades juntas são a definição do item: o apodrecimento é real e
  nada o impede.

  A versão anterior fixava um NÚMERO exato e era instável por construção. Medido em
  2026-08-30, no mesmo dia: 63 → 59 depois de três PRs reancorarem, e 59 → 64 depois
  de UM único squash-merge (#1432), porque o squash orfana as âncoras dos conceitos
  do próprio PR. Um número exato reprova em duas situações opostas — quando alguém
  conserta e quando alguém apenas mergeia — e no meio disso derruba PRs sem relação
  nenhuma, que herdam o vermelho (aconteceu com o #1402, que não toca em âncora).
  Um portão que o trabalho correto derruba não é rigor, é ruído; e ruído é o que faz
  um portão ser ignorado.

  O que este item afirma NÃO é uma contagem — é a AUSÊNCIA DA PREVENÇÃO, que é o que
  o próprio item sempre disse que fecharia ele. Ver [B-049]: o squash orfana a ~1 por
  conceito por merge, então reancorar sem mudar o portão se desfaz na semana seguinte.
  Fechar exige que `validate_okf` REPROVE âncora inalcançável em vez de degradar para
  base-ref (hoje `_check_c5b` faz `if not git.is_ancestor(prev_sha, base_ref): continue`
  — pula a checagem em vez de reprovar), e que o blob anchor (content-addressed,
  sobrevive a rebase E a squash) seja o padrão.

  Vira DRIFTED exatamente quando a prevenção existir: aí `validate_okf` reprova, o
  verify falha, e o item TEM de ser fechado. É o único evento que muda a alegação.

  A contagem continua registrada no ROADMAP como INSTANTÂNEO datado, explicitamente
  não-gateado — número medido apodrece por design, e fingir o contrário foi o defeito.

  NOTA sobre a forma do `verify` (2026-08-30, segunda correção no mesmo dia): a
  primeira versão do predicado novo chamava `python3 scripts/validate_okf.py` e lia
  QUALQUER saída não-zero como "a prevenção existe". Isso é falso: o validador sai
  não-zero por motivos de ambiente também — na CI ele reprovou por resolução de
  base-ref enquanto o check `okf-wiki-validation` do MESMO PR estava verde, duas
  invocações com resultados opostos sobre a mesma árvore. O item passaria a mandar
  FECHAR num momento em que nada foi prevenido: a mesma inversão que o formato novo
  veio consertar, só que por falha de ambiente em vez de número instável.

  Por isso o verify NÃO executa mais o validador. Ele lê o BLOCO de código que
  implementa a tolerância (`if not git.is_ancestor(...): continue` em `_check_c5b`) e
  decide por ele — bloco extraído, não símbolo grepado, porque o símbolo continuaria
  existindo depois da correção e só o corpo diria se ainda PULA ou se passou a
  REPROVAR. Se o bloco sumir (refatoração), o verify sai INDETERMINADO e pede olho
  humano em vez de concluir. Verificado nos três estados: verde hoje; DRIFTED quando
  o `continue` vira `fails.add`; DRIFTED quando o bloco é refatorado.
last-verified: 2026-08-30
# NOTA (aprendida na propria CI): a primeira versao deste verify PASSAVA local e
# REPROVAVA na CI. Causa: `actions/checkout` sem `fetch-depth: 0` clona RASO, e
# `git merge-base --is-ancestor` falha para TODO sha sem historico — medido: 20 de
# 20 conceitos contados como orfaos num clone raso. O guard de shallow acima e
# obrigatorio em qualquer verify que use ancestralidade. E a mesma cegueira que
# este item existe para consertar, mordendo o proprio mecanismo.
```

---

## Superauditoria 2026-08-30 — B-063 … B-102

Os 40 itens abaixo são a transcrição integral da superauditoria de 2026-08-30
(relatório estruturado: `https://claude.ai/code/artifact/c44eb468-1f03-4e37-8b0b-b8f2a197b816`).
Cada achado do relatório vira **um** item aqui, ou uma recusa registrada — a regra
que o próprio [B-101] enuncia e que esta seção existe para não violar.

Referências cruzadas com itens já existentes, para não duplicar: a metade WORM de
[B-085] é [B-046]; a metade process-wide de [B-077] é [B-056]; as cinco lanes
hosted-blocked de [B-066] são o item de lanes hosted-blocked do #1434; a decisão de não gastar em builder
hosted que [B-087] cita é [B-031].

### B-062 — produção roda código 39 commits atrás da main, com correções de GDPR presas fora

As cinco regiões de produção executam a imagem `4f9313e0-r1`, confirmado não pela
configuração mas pela API de Containers da Cloudflare em 2026-08-30, aplicação por
aplicação (`corelink-prod`, `prod-sam`, `prod-lhr`, `prod-nrt`, `prod-syd`). Último
deploy bem-sucedido: `2026-08-27T02:56`.

Dez dos 39 commits não implantados tocam o crate do contêiner. Entre eles `3ec2a76e`
(#1410, o apagamento do Art.17 do GDPR rodando pela metade), `06048762` (#1391, o
caminho de leitura do CAS sem limite de tamanho), `cbd68c73` (#1424, a assinatura do
Turborepo descartada em vez de verificada) e `b1235dbe` (#1398, três coleções sem
limite).

O portão de merge deste repositório é rigoroso e funciona. Toda essa disciplina é
anulada nesta costura: **um merge impecável que não chega à produção não protege
ninguém.** Deploy é decisão do owner; este item existe para que a distância entre
`main` e produção seja rastreada, não para disparar o deploy.

```backlog
id: B-062
repo: corelink-server
owner: owner
status: open
verify: manual
verify-means: |
  MANUAL, e o `verify` NÃO decide a alegação — declaro isso em vez de fingir.

  A alegação é sobre o estado VIVO de produção: qual imagem cada uma das cinco
  aplicações está rodando agora. Isso só se lê na API de Containers da Cloudflare
  (`CLOUDFLARE_CONTAINERS_API_TOKEN`), que não está disponível no runner de CI, e
  a LISTA daquele endpoint serve visão defasada — a leitura confiável é o GET por
  `{id}`, um por aplicação.

  Um `verify` automático que comparasse o pin do `wrangler.toml` com o `HEAD` seria
  um portão dominado: mediria a configuração, não o que executa, e passaria verde
  exatamente no cenário que este item descreve (pin novo declarado, contêiner velho
  ainda vivo, porque um deploy de Worker não reinicia contêiner — só uma imagem NOVA
  substitui).

  Procedimento de reverificação: GET por id nas cinco aplicações, comparar a tag com
  `git rev-parse --short HEAD`, e contar `git log --oneline <tag-commit>..HEAD --
  crates/corelink-container/`. Fecha quando as cinco convergirem para um pin cuja
  origem seja um commit alcançável a partir da `main`.
last-verified: 2026-08-30
```

### B-063 — uma partição da trilha de auditoria não drena há 82h, disparando SEV-0 diário para ninguém

O cron `audit-archive-lag` falhou nas últimas dez execuções, sem sucesso desde
`2026-08-27T03:22`. Ele não está quebrado — está reportando corretamente:
`AUDIT_ARCHIVE_PARTITION_FAILURE — 1 partition(s) stuck past T=3h: 93da3f7a/enam
(n=140, idle=81.91h ago)`, com `PagerDuty SEV-0 dispatched`.

O prefixo `93da3f7a` é o tenant do canary horário (`cas-canary.yml:80`): a partição
travada é justamente a que mais gera linhas, e o canary continua alimentando-a.

Por que ninguém viu: o portão primário `audit-chain-daily-verify` está **verde**,
porque o `MAX(archived_at)` da tabela inteira permanece fresco graças às partições
saudáveis. O runbook `RB-AUDIT-ARCHIVE-ABSENT.md` §3.3 antecipa exatamente este ponto
cego e prescreve tratar partição persistentemente falha como SEV-1 próprio. O detector
por partição foi construído ([B-022]) precisamente para ele, funciona, e é ignorado.

A causa mecânica é [B-112]. Este item cobre o incidente; aquele cobre o defeito.

```backlog
id: B-063
repo: corelink-server
owner: owner
status: open
verify: manual
verify-means: |
  MANUAL, e o `verify` NÃO decide a alegação — declaro em vez de fingir.

  A alegação é sobre estado de produção (linhas não arquivadas numa partição do D1
  de prod) e sobre um alarme externo (PagerDuty). Nenhum dos dois é legível do CI:
  o D1 de prod exige credencial que o runner não tem, e o estado do PagerDuty não
  está no repositório.

  Um `verify` que apenas relesse o log do último `audit-archive-lag` seria dominado
  por [B-112]: assim que o dreno voltar a funcionar o log fica verde, mas o backlog
  acumulado continua lá — mediria o alarme, não a condição.

  Procedimento: rodar a consulta do runbook §3.3 contra o D1 de prod e conferir se
  alguma partição tem `idle > 3h`. Fecha quando a partição `93da3f7a/enam` drenar
  E o `audit-archive-lag` voltar a passar. Consertar [B-112] é pré-requisito para
  que ela drene sozinha.
last-verified: 2026-08-30
```

### B-064 — o selamento da auditoria tem teto de 200 linhas/hora e o laço que o contornaria nunca foi implementado no chamador

Três fatos compõem, e juntos são a causa mecânica de [B-111].

**O orçamento.** `audit_drain.rs:409` é `.unwrap_or(200)`, e o comentário da linha 405
diz textualmente `Global per-call row budget` — é global entre TODAS as partições, não
por partição: `handle_drain` inicializa `let mut remaining = state.batch_limit` e o
decrementa ao longo de todo o laço. A linha 189 do `secrets-checklist.md` confirma que
`AUDIT_DRAIN_BATCH_LIMIT` está `UNSET in prod (default 200)`.

**O contrato quebrado.** O handler responde `rows_sealed`, `partitions_drained` e
`incomplete`. Seu único chamador, `apps/signup-worker/src/webhooks/audit_drain_cron.ts`,
lê `j.sealed` e `j.partitions` — chaves que não existem — e registra `sealed=0
partitions=0` para sempre. O tipo declarado no consumidor sequer inclui `incomplete`,
cujo contrato inteiro, segundo o mesmo `secrets-checklist`, é *"a budget-bounded sweep
returns `incomplete: true` so the hourly cron re-drains until done"*. O cron dispara uma
vez por hora, uma requisição, sem laço.

**A assimetria.** Emissão é em lote e concorrente (256 linhas por statement via JSON1);
selamento é serial, uma `UPDATE` por linha a ~0,3s. E `FIND_MISSING_BLOB_CAP = 4096`,
com o comentário de `r2_s3.rs:1765` registrando que *"N digests still produce N
`ReadAttempted` rows"* — uma requisição Bazel no teto gera 4.096 linhas.

Consequência aritmética: uma leitura de CAS emite duas linhas (`ReadAttempted` antes da
busca, `ReadServed` no acerto). A 200 linhas/hora o selamento acompanha ~100 leituras
por hora **para a plataforma inteira**, somando os cinco ambientes — cerca de 73 mil por
mês. O tier gratuito, sozinho, promete `includedRequests: 500_000` por mês *por tenant*
(`apps/docs/src/lib/pricing.ts:136`). O outbox absorve rajadas, então o produto não
recusa requisições; a defasagem da camada de evidência é que cresce sem limite.

Reparo mínimo: duas linhas no cron — ler as chaves certas e iterar enquanto `incomplete`
for verdadeiro. Reparo estrutural: selar em lote, como a emissão já faz.

```backlog
id: B-064
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'cron=apps/signup-worker/src/webhooks/audit_drain_cron.ts
  h=crates/corelink-container/src/routes/audit_drain.rs
  [ -f "$cron" ] && [ -f "$h" ] || { echo "FALHA: arquivo sumiu — reavalie o item."; exit 1; }
  emite_novo=0; grep -q "\"rows_sealed\"" "$h" && emite_novo=1
  le_velho=0; grep -qE "j\.sealed|j\.partitions" "$cron" && le_velho=1
  le_incomplete=0; grep -q "incomplete" "$cron" && le_incomplete=1
  [ "$emite_novo" = 1 ] || { echo "FALHA: handler nao emite mais rows_sealed — a alegacao mudou, reavalie."; exit 1; }
  if [ "$le_velho" = 0 ] && [ "$le_incomplete" = 1 ]; then
    echo "FALHA: o cron le as chaves certas E consulta incomplete — feche o item."; exit 1; fi
  echo "aberto: handler emite rows_sealed; cron le_chaves_velhas=$le_velho le_incomplete=$le_incomplete"'
verify-means: |
  open — o handler emite `rows_sealed`/`partitions_drained`/`incomplete` E o cron
  continua lendo `j.sealed`/`j.partitions`, ou continua sem consultar `incomplete`.
  As duas metades juntas são a alegação: o contrato existe do lado do servidor e não
  tem implementação do lado de quem chama.

  Vira DRIFTED (e o item TEM de ser fechado) quando o cron passar a ler as chaves
  corretas E a consultar `incomplete` — que é exatamente o reparo de duas linhas.

  O que este verify NÃO decide, e admito: o teto de 200 em si. Ele é o `unwrap_or`
  default e continuará no código mesmo depois do laço existir — corretamente, porque
  com laço o teto por chamada deixa de ser um teto por hora. Medir o `200` daria um
  portão que nunca fecha. A alegação verificável é o contrato quebrado, e é essa que
  o comando decide.
last-verified: 2026-08-30
```

### B-065 — dois endpoints Stripe vivos processam o mesmo evento duas vezes, há mais de sete dias

`billing-health-daily` falhou 8 de 8 execuções, sem nenhum sucesso desde 23 de agosto.
Também não está quebrado: reporta `BILLING HEALTH: 1 anomaly(ies) found` e detalha
`3 event type(s) ingested under BOTH id schemes in the last 30d` —
`customer.subscription.deleted (1/1)`, `customer.subscription.updated (2/2)`,
`invoice.payment_failed (2/2)`.

O detector (`scripts/check_billing_health.py:169`) explica o mecanismo:
`stripe_webhook_events_processed` deduplica por `event_id` como chave primária, o que
só protege retentativas sob o *mesmo* esquema de identificador. Um endpoint grava o
`evt_…` da Stripe, o outro grava um hash derivado da mesma entrega; as duas linhas não
colidem e o evento é processado duas vezes.

Processamento duplicado de `subscription.deleted` e `subscription.updated` afeta estado
de direito de acesso, não apenas contagem. O reparo é no painel da Stripe — aposentar o
endpoint redundante — e portanto é ação exclusiva do owner.

```backlog
id: B-065
repo: corelink-server
owner: owner
status: open
verify: manual
verify-means: |
  MANUAL, e o `verify` NÃO decide a alegação. Declaro em vez de fingir.

  A alegação é sobre a configuração de destinos de webhook na conta Stripe, que não
  está no repositório e não é legível do CI sem a chave da conta. Pior: a lista v1 da
  API Stripe é CEGA a destinos v2, então mesmo com credencial um `verify` ingênuo
  reportaria zero e passaria verde — portão dominado, exatamente o que este item não
  pode ter.

  O sinal correto já existe e é o `billing-health-daily`, que detecta a duplicidade
  pelo lado dos dados. Este item não recria esse detector; ele rastreia a AÇÃO no
  painel da Stripe, que só o owner executa.

  Procedimento: no painel Stripe, manter o destino "Corelink prd" apontando para o
  signup-worker e aposentar o redundante. Fecha quando `billing-health-daily` voltar
  a passar por três execuções consecutivas.
last-verified: 2026-08-30
```

### B-066 — RECUSADO: o `smoke-install` já está portado atrás do gate de Actions hosted

**Este item é uma recusa registrada, não um achado.** A superauditoria de 2026-08-30
listou o `smoke-install` (0 de 8 execuções) junto das demais lanes derrubadas pelo
bloqueio de faturamento do GitHub Actions. Ao escrever o `verify` que decidiria a
alegação, ela caiu.

`smoke-install.yml:112` já traz `if: vars.HOSTED_ACTIONS_AVAILABLE == "true"`, exatamente
o padrão que o `cas-canary` estabeleceu e que a própria auditoria elogiou como tratamento
correto. As execuções recentes aparecem como `skipped`, não `failure` — as falhas são
anteriores ao porte. A lane não está quebrada: está desarmada de propósito, com o custo
explícito, esperando o operador liberar gasto hosted.

O que sobra do achado original pertence ao item de lanes hosted-blocked do #1434 (a
decisão de fundo: liberar gasto, migrar para a frota self-hosted, ou apagar) e não a um
item novo. Registro a recusa em vez de apagar o achado, porque a regra de [B-101] exige
que cada constatação vire item **ou** recusa com motivo — e uma auditoria que só publica
o que confirma não deixa ninguém calibrar quanto acreditar nela.

Lição de método, que vale mais que o item: eu havia contado `smoke-install` como lane
caída **sem ler o `if:` do job**. Contar execuções vermelhas sem ler a condição de guarda
é a mesma cegueira que este repositório documenta em vários lugares — medir o sintoma sem
ler o predicado.

```backlog
id: B-066
repo: corelink-server
owner: tl
status: done
verify: |
  bash -c 'f=.github/workflows/smoke-install.yml
  [ -f "$f" ] || { echo "FALHA: smoke-install.yml sumiu — a recusa perdeu objeto, reavalie."; exit 1; }
  grep -q "HOSTED_ACTIONS_AVAILABLE" "$f" || { echo "FALHA: o job NAO esta mais portado atras do gate — a recusa deixou de valer, REABRA o item."; exit 1; }
  echo "recusa mantida: smoke-install portado atras de HOSTED_ACTIONS_AVAILABLE"'
verify-means: |
  done — polaridade INVERTIDA, como todo item fechado neste arquivo: o comando PASSA
  enquanto o motivo da recusa continuar verdadeiro, e FALHA se alguém remover o gate.

  Concretamente: passa enquanto `smoke-install.yml` portar o job atrás de
  `HOSTED_ACTIONS_AVAILABLE`. Se esse `if:` for removido, a lane volta a queimar minutos
  hosted num bloqueio de faturamento, o achado original passa a valer, e o verify vermelho
  força a REABERTURA — não o fechamento.

  É a polaridade correta para uma recusa: ela não afirma "não há problema", afirma "não há
  problema ENQUANTO esta condição valer", e vigia a condição.
last-verified: 2026-08-30
```

### B-067 — oito crates têm teste executado por PR, a lane do workspace passa `--no-run`, e os dois crates de autenticação não têm execução nenhuma

Os 8.619 testes são reais e a densidade é boa. Esta alegação não é sobre quantos testes
existem, é sobre quantos executam.

**Escopo por PR.** Toda lane de teste disparada por `pull_request` é `--package <um>`.
São oito: `corelink-server`, `-worker`, `-hash`, `-meta`, `-reapi`, `-adapter-host`,
`-client-verify`, `-tenant-path`. (Contando todos os workflows, 28 crates aparecem em
`cargo test -p` — a maioria em lanes noturnas ou mortas.)

**A lane que aparenta cobrir o resto não cobre.** `nightly.yml:218` é
`cargo test --release --workspace --no-run`: compila os testes e não executa nenhum. É
verificação de compilação apresentada como suíte. O único `cargo test --workspace` que
de fato executa está em `cas_foundation.yml:156`, que é dispatch-only e hosted-blocked
(#1434). E a própria `nightly` tem zero verdes em doze execuções.

**Os dois crates de autenticação.** `corelink-auth` (WebAuthn, OTP de recuperação) e
`corelink-pat` (Argon2id, verificação de credencial) aparecem em **exatamente um**
workflow: `mutation-nightly.yml`, como matriz de mutação — não como execução de teste.
Essa lane pede `runs-on: ubuntu-x64-4core`, não tem `schedule`, e acumula 1 cancelamento
e 5 falhas, zero sucessos, nada desde 2026-08-03. Net: **zero execução de teste em CI**
para as primitivas de autenticação do produto.

E o repositório afirma o contrário a quem chega: `welcome-first-pr.yml:90` recebe todo
primeiro contribuidor com *"(`cargo build --workspace`, `cargo clippy --workspace
--tests -- -D warnings`, `cargo test --workspace`). CI runs all three."* A terceira é
falsa.

**Dependência de sequência:** este item vem ANTES de qualquer reparo de credencial
([B-073], [B-074], [B-081]). Consertar autenticação sem execução de teste é apostar.

```backlog
id: B-067
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'norun=0
  grep -rqE "cargo test.*--workspace.*--no-run" .github/workflows/ && norun=1
  authpat=$(grep -rlE "corelink-(auth|pat)" .github/workflows/ 2>/dev/null | grep -v mutation-nightly | wc -l | tr -d " ")
  claim=0; grep -q "CI runs all three" .github/workflows/welcome-first-pr.yml 2>/dev/null && claim=1
  if [ "$norun" = 0 ] && [ "$authpat" -gt 0 ]; then
    echo "FALHA: --no-run sumiu E auth/pat tem lane fora do mutation-nightly — feche o item."; exit 1; fi
  echo "aberto: no-run=$norun  workflows_com_auth_ou_pat_fora_do_mutation=$authpat  afirmacao_ao_contribuidor=$claim"'
verify-means: |
  open — existe uma lane `cargo test --workspace --no-run` E nenhum workflow fora do
  `mutation-nightly` nomeia `corelink-auth`/`corelink-pat`. As duas metades juntas são
  a alegação: a lane que parece cobrir tudo não executa nada, e os dois crates de
  autenticação não têm lane própria.

  Vira DRIFTED quando AMBOS forem resolvidos — o `--no-run` virar execução real E os
  dois crates ganharem lane. Escolhi o AND deliberadamente: resolver só metade deixa a
  alegação verdadeira, e um portão que fecha pela metade do reparo é pior que nenhum.

  O que NÃO decide, e admito: se a lane que passar a nomear `corelink-auth` de fato
  EXECUTA (podia ser outra matriz de mutação, ou uma lane morta). O comando conta a
  presença do nome, não a execução. Quem fechar este item deve confirmar à mão que a
  lane nova roda e é verde — e, se não for, reabrir em vez de fechar.

  A correção da linha 90 do `welcome-first-pr.yml` é reportada mas não gateada: é
  documentação, e travar o item nela atrasaria o reparo que importa.
last-verified: 2026-08-30
```

### B-068 — os testes `#[ignore]` que cobrem D1, R2 e Stripe reais não são executados por nada

Cinco crates carregam testes `#[ignore]` que são, segundo os próprios comentários, a
cobertura real dos caminhos de produção: D1 real, round-trip R2 real, checkout Stripe
real, shadow Neon, e2e de PAT. Exemplo típico em `tier_select_store.rs:131`:
*"behavioural coverage of the real SQL uses the standard `#[ignore]` harness"*.

Procurei quem os executa. `--ignored` e `include-ignored` não aparecem em nenhum
workflow, em nenhum script, e o `.config/nextest.toml` não define `run-ignored` em
profile algum.

Distribuição medida em 2026-08-30: `corelink-container` 28, `corelink-audit-chain` 14,
`corelink-stripe-real` 7, `corelink-pat` 2, `corelink-cas` 1.

O SQL real, o R2 real e o Stripe real têm zero execuções — enquanto os comentários de
código afirmam que essa é justamente a camada onde eles são cobertos. É o padrão que
[B-101] descreve: a evidência de que o caminho é coberto existe em prosa, não em
execução.

```backlog
id: B-068
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'n=$(grep -rl "#\[ignore" crates/ --include="*.rs" 2>/dev/null | wc -l | tr -d " ")
  [ "$n" -gt 0 ] || { echo "FALHA: nao ha mais testes #[ignore] — feche o item."; exit 1; }
  exec_wf=$(grep -rlE "\-\-ignored|include-ignored" .github/workflows/ scripts/ 2>/dev/null | wc -l | tr -d " ")
  exec_nt=0; [ -f .config/nextest.toml ] && grep -q "run-ignored" .config/nextest.toml && exec_nt=1
  if [ "$exec_wf" -gt 0 ] || [ "$exec_nt" = 1 ]; then
    echo "FALHA: existe executor de #[ignore] (workflows/scripts=$exec_wf nextest=$exec_nt) — feche o item."; exit 1; fi
  echo "aberto: $n arquivos com #[ignore] e ZERO executores"'
verify-means: |
  open — existem testes `#[ignore]` no repositório E nenhum workflow, script ou profile
  do nextest os executa.

  Vira DRIFTED quando aparecer um executor — `--ignored` num workflow, num script, ou
  `run-ignored` no `.config/nextest.toml`. Também fecha, legitimamente, se os testes
  `#[ignore]` deixarem de existir (alguém os converteu em testes normais ou os apagou).
  Os dois desfechos são reparos válidos e o comando aceita ambos.

  Escrito para a alegação — a AUSÊNCIA DO EXECUTOR — e não para a contagem. Um número
  exato de testes `#[ignore]` apodrece a cada PR que adiciona um, e derrubaria PRs sem
  relação nenhuma. A contagem por crate fica na prosa como instantâneo datado.
last-verified: 2026-08-30
```

### B-069 — os nove arquivos de E2E da interface autenticada estão em `test.fixme`, inclusive apagamento GDPR e dupla aprovação

Nove de nove specs Playwright em `apps/admin-ui/playwright/e2e/` contêm `test.fixme`.
Não é lacuna pontual, é a suíte inteira: `00-a11y-sweep`, `01-onboarding`,
`02-consent-capture`, `03-consent-withdraw`, `04-dsr-access`, `05-dsr-erasure`,
`06-admin-audit-viewer`, `07-admin-dual-approval`, `08-locale-switch`.

Literal, em `05-dsr-erasure.spec.ts:21`:
`test.fixme("erasure with category selection → receipt + SLA clock", …)`.

Os fluxos sem nenhuma cobertura executando são exatamente os de maior consequência
regulatória e de privilégio: captura e retirada de consentimento, acesso e apagamento
de DSR (Art. 15 e 17), o visualizador de auditoria e a dupla aprovação administrativa.

Fecha o círculo com [B-110]: a correção do apagamento do Art.17 não está implantada
**e** o fluxo de interface que a exercitaria nunca roda.

```backlog
id: B-069
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'd=apps/admin-ui/playwright/e2e
  [ -d "$d" ] || { echo "FALHA: diretorio e2e sumiu — reavalie o item."; exit 1; }
  total=$(ls "$d"/*.spec.ts 2>/dev/null | wc -l | tr -d " ")
  [ "$total" -gt 0 ] || { echo "FALHA: nao ha mais specs — reavalie o item."; exit 1; }
  comfixme=$(grep -l "test\.fixme" "$d"/*.spec.ts 2>/dev/null | wc -l | tr -d " ")
  [ "$comfixme" -gt 0 ] || { echo "FALHA: nenhum spec tem test.fixme — feche o item."; exit 1; }
  echo "aberto: $comfixme de $total specs e2e ainda em test.fixme"'
verify-means: |
  open — pelo menos um spec E2E da admin-ui ainda carrega `test.fixme`.

  Vira DRIFTED quando o último `test.fixme` sair, que é o reparo. Deliberadamente NÃO
  fixo o número nove: um portão que exige exatamente 9 reprova quando alguém conserta
  um só, e punir progresso parcial é como se ensina uma equipe a ignorar o portão.
  O `9 de 9` fica na prosa como instantâneo datado de 2026-08-30.

  O que NÃO decide, e admito: se os specs, uma vez destravados, PASSAM. Tirar o
  `test.fixme` e deixar o teste vermelho fecharia este item sem entregar cobertura.
  Quem fechar deve confirmar que a suíte roda verde em CI; se não rodar, o item certo
  é um novo, não a reabertura deste.
last-verified: 2026-08-30
```

### B-070 — o `[env.staging]` é declarado como espelho 1:1 de produção e nunca recebeu deploy

`wrangler.toml:579` declara `[env.staging]` descrevendo-o como *"um espelho 1:1 da
topologia de prod"* e afirma que *"os fluxos de canary e rollout promovem artefatos de
staging para prod"*.

Nenhum workflow faz deploy com `--env staging`. Toda ida a produção é direta, sem soak.

Isto é o que torna [B-110] mais caro do que precisaria ser: sem um ambiente onde a
imagem nova assente antes de ir para as cinco regiões, cada deploy carrega risco que
um staging absorveria — e é parte de por que o deploy fica represado.

Nota de escopo: as lanes de verificação paradas (`fuzz-nightly`, `mutation-nightly`,
`coverage`, `load-test-nightly`, `endurance-2h`) NÃO estão neste item. As cinco
hosted-blocked são #1434; as demais têm nota de parking justificada e são decisão de
cadência, não defeito. O que este item afirma é especificamente a distância entre o que
o `wrangler.toml` declara sobre staging e o que existe.

```backlog
id: B-070
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'decl=0; grep -q "^\[env\.staging\]" wrangler.toml && decl=1
  [ "$decl" = 1 ] || { echo "FALHA: [env.staging] nao e mais declarado — feche o item."; exit 1; }
  dep=$(grep -rlE "deploy.*--env[= ]staging|--env[= ]staging.*deploy" .github/workflows/ 2>/dev/null | wc -l | tr -d " ")
  [ "$dep" = 0 ] || { echo "FALHA: $dep workflow(s) fazem deploy em staging — feche o item."; exit 1; }
  echo "aberto: [env.staging] declarado no wrangler.toml e ZERO workflows deployam nele"'
verify-means: |
  open — o `[env.staging]` está declarado E nenhum workflow faz deploy nele. As duas
  metades são a alegação: o ambiente é prometido em configuração e não existe de fato.

  Vira DRIFTED por qualquer um dos dois reparos legítimos — alguém passa a deployar em
  staging (o bom), ou alguém remove a declaração e a prosa que promete promoção via
  staging (o honesto). Os dois fecham o item, e é correto que fechem: a alegação é
  sobre a DIVERGÊNCIA, não sobre a ausência de staging.
last-verified: 2026-08-30
```

### B-071 — não existe coleta de lixo nem eviction em produção; o armazenamento é catraca de sentido único sob preço fixo

O crate `corelink-gc` tem ~22,9 mil linhas, é verificado em TLA+, tem proptests, e não
executa em produção. Nem `corelink-gc` nem o binário `gc_sweep` aparecem no `Dockerfile`
ou no `cf-deploy-prod.yml`. A lane `gc-sweep-dry-run.yml` é, como o nome diz, simulação.

Nada recupera espaço em R2 hoje.

Isto compõe com dois outros itens. Com [B-095]: a interface de cliente afirma que o pin
de workspace isenta conteúdo de eviction — não há eviction da qual isentar. E com
[B-079]: o mapeamento de tier seleciona a escada de TTL de retenção, descrita em
comentário como *"customer-visible retention promise"* — promessa hoje inerte.

É o exemplo mais caro do padrão que a auditoria inteira encontrou: trabalho excelente
construído, formalmente verificado, e com a última costura aberta.

```backlog
id: B-071
repo: corelink-server
owner: owner
status: open
verify: |
  bash -c '[ -d crates/corelink-gc ] || { echo "FALHA: crate corelink-gc sumiu — reavalie o item."; exit 1; }
  n=0
  for f in Dockerfile .github/workflows/cf-deploy-prod.yml .github/workflows/container-build-push-prod.yml; do
    [ -f "$f" ] || continue
    grep -qE "gc_sweep|corelink-gc" "$f" && n=$((n+1))
  done
  [ "$n" = 0 ] || { echo "FALHA: $n artefato(s) de build/deploy ja referenciam o GC — feche o item."; exit 1; }
  echo "aberto: corelink-gc existe e nao e referenciado por Dockerfile nem pelas lanes de deploy de prod"'
verify-means: |
  open — o crate existe E nenhum artefato de build ou deploy de produção o referencia.

  Vira DRIFTED quando o `Dockerfile` ou uma lane de deploy passar a construir/embarcar
  o GC, que é o reparo. Também fecharia se o crate fosse removido — desfecho válido se
  a decisão for não ter GC, e nesse caso o item deve ser fechado como recusa registrada,
  não apagado.

  O que NÃO decide, e admito: se o GC, uma vez embarcado, de fato RODA e recupera bytes.
  Presença no build é condição necessária, não suficiente. Quem fechar deve provar com
  bytes recuperados medidos em produção — a mesma exigência de "prove a execução, não a
  ausência de reclamação" que este repositório aplica em todo lugar.

  Owner, não tl: rodar GC pela primeira vez em dados de cliente é decisão de produto e
  de risco, não de higiene de engenharia.
last-verified: 2026-08-30
```

### B-072 — o `wrangler.toml` declara dois crons no Worker e o Worker não tem manipulador `scheduled`

`wrangler.toml:283-284` declara `[triggers]` com `crons = ["0 6 * * 1", "0 14 * * 1"]`.
Não existe `async scheduled(...)` em `worker/src/index.ts` nem em nenhum outro módulo do
Worker — a única ocorrência da palavra é um comentário na linha 100. Os disparos ocorrem
e não encontram destino.

A consequência específica importa mais que o defeito: um desses agendamentos é o drill
de entrega do PagerDuty. Ele nunca executou.

Isso não é independente de [B-111]. A organização acredita ter validado que o alarme
chega a um humano, e essa validação nunca correu. O alarme de [B-111] de fato dispara;
o que nunca foi provado é que alguém o recebe — e três dias de SEV-0 sem resposta são
consistentes com as duas hipóteses.

```backlog
id: B-072
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'temcron=0; grep -qE "^crons[[:space:]]*=" wrangler.toml && temcron=1
  [ "$temcron" = 1 ] || { echo "FALHA: nao ha mais crons declarados no wrangler.toml — feche o item."; exit 1; }
  h=$(grep -rlE "async scheduled[[:space:]]*\(|scheduled[[:space:]]*:[[:space:]]*async" worker/src/ 2>/dev/null | wc -l | tr -d " ")
  [ "$h" = 0 ] || { echo "FALHA: existe manipulador scheduled no Worker ($h arquivo(s)) — feche o item."; exit 1; }
  echo "aberto: crons declarados no wrangler.toml e ZERO manipuladores scheduled no worker/src"'
verify-means: |
  open — há `crons` declarados E nenhum manipulador `scheduled` no código do Worker.

  Vira DRIFTED por qualquer um dos dois reparos: implementar o manipulador (o bom) ou
  remover os crons órfãos (o honesto). A alegação é a DIVERGÊNCIA entre o gatilho
  declarado e o destino ausente, então os dois desfechos a encerram legitimamente.

  Se o reparo for implementar o manipulador, quem fechar deve confirmar que o drill do
  PagerDuty efetivamente entrega — presença do handler não prova entrega, e é
  precisamente a entrega que [B-111] presume e nunca foi provada.
last-verified: 2026-08-30
```

### B-073 — um assento em outro tenant é concedido por hash de e-mail, sem token, sem expiração e sem verificação, e vira PAT `cas:rw` daquele tenant

É o único achado da auditoria que entrega dado de um cliente a outro. O isolamento no
plano de dados é sólido e não cedeu sob ataque; a brecha é em quem recebe um assento.

Convidar um colega grava `team_member` com `status='invited'` e `email_hash`, sem o
e-mail em claro — decisão de privacidade correta. O problema é o resgate. Seis elos,
cada um verificado no código de 2026-08-30:

1. `apps/signup-worker/src/lib/d1.ts:316` — `SELECT tenant_id, user_id FROM team_member
   WHERE email_hash IN (?1, ?2) AND status = 'invited' LIMIT 1`. Sem escopo de tenant,
   sem token, sem nonce, sem expiração: a primeira linha convidada do banco INTEIRO que
   casar com o hash é transferida ao novo usuário Clerk.
2. `grep -n "verification\|email_verified" apps/signup-worker/src/webhooks/clerk.ts`
   retorna **zero linhas**. O tipo do evento sequer modela o campo, então a checagem é
   estruturalmente impossível no código atual.
3. `worker/src/lib/clerk_auth.ts:299` — sessão sem tenant próprio cai no assento
   (`SELECT tenant_id, role FROM team_member WHERE user_id = ?1 AND status = 'active'`).
4. `worker/src/index.ts:3100` — papel `member` vira `x-corelink-scope: read-write`.
5. `crates/corelink-container/src/routes/customer.rs:796` — cunhar PAT exige apenas
   `requires_cache_write(caller_scope)`. O assento passa e recebe `cas:rw` do tenant.
6. `team_member` está em `TENANT_ID_TABLES`, ou seja, o apagamento DSR é chaveado por
   `tenant_id`: um assento mantido em OUTRO tenant sobrevive ao apagamento do próprio.

Toda a segurança dessa transição repousa numa suposição sobre um terceiro que o código
não declara nem impõe: que a Clerk sempre verifica posse do e-mail antes de emitir
`user.created`. E mesmo supondo que sempre verifique, os elos 1, 3 e 6 permanecem — o
convite é um **portador permanente, transferível e reciclável**, e um endereço
corporativo reatribuído entrega o assento antigo ao novo titular. Ao contrário do
convite (que emite `team.invited` na auditoria), a ACEITAÇÃO não emite evento nenhum.

Quatro reparos independentes, cada um quebrando a cadeia sozinho: token de convite
exigido no resgate; escopo de `tenant_id` na consulta; expiração; e leitura do campo de
verificação da Clerk. Somar evento de auditoria na aceitação.

**Sequência:** depende de [B-067]. Não mergear conserto de auth contra CI que não roda
os testes de `corelink-auth`/`corelink-pat`.

```backlog
id: B-073
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'd=apps/signup-worker/src/lib/d1.ts
  c=apps/signup-worker/src/webhooks/clerk.ts
  [ -f "$d" ] && [ -f "$c" ] || { echo "FALHA: arquivo sumiu — reavalie o item."; exit 1; }
  q=$(awk "/acceptTeamInvitation/,/^}/" "$d" 2>/dev/null)
  sel=$(printf "%s" "$q" | awk "/SELECT tenant_id, user_id FROM team_member/,/first</")
  temtoken=0; printf "%s" "$sel" | grep -qiE "invit(e|ation)_token|nonce" && temtoken=1
  temtenant=0; printf "%s" "$sel" | grep -qE "WHERE[^\"]*tenant_id[[:space:]]*=" && temtenant=1
  temexp=0; printf "%s" "$sel" | grep -qiE "expires_at|expiry|invited_at_ms[[:space:]]*>" && temexp=1
  temver=0; grep -qE "email_verified|verification" "$c" && temver=1
  soma=$((temtoken + temtenant + temexp + temver))
  [ "$soma" = 0 ] || { echo "FALHA: $soma de 4 defesas ja presentes (token=$temtoken tenant=$temtenant exp=$temexp verif=$temver) — reavalie e feche ou reescreva o item."; exit 1; }
  echo "aberto: aceitacao de convite sem token, sem escopo de tenant, sem expiracao e sem checagem de verificacao"'
verify-means: |
  open — NENHUMA das quatro defesas existe. Escolhi o "zero de quatro" em vez de "menos
  de quatro" de propósito: cada defesa quebra a cadeia sozinha, então a primeira que
  aparecer já muda a alegação do item, e o item deve ser reavaliado e reescrito para o
  que sobrou — não continuar aberto afirmando algo que deixou de ser verdade.

  Vira DRIFTED assim que qualquer defesa entrar. Isso é intencional e é o oposto de
  ruído: é o sinal de que a alegação precisa ser reescrita, e a mensagem de falha diz
  exatamente qual das quatro apareceu.

  O que NÃO decide, e admito: os elos 3, 4, 5 e 6 (o fallthrough do `clerk_auth`, o
  mapa `member` → `read-write`, o portão do mint, e o escopo do apagamento DSR). Eles
  são comportamento correto isoladamente e só compõem a cadeia junto com o elo 1 —
  gatear neles daria falso positivo permanente. O comando decide a RAIZ, que é a
  aceitação não autenticada; se a raiz for fechada, a cadeia não existe mais.
last-verified: 2026-08-30
```

### B-074 — o caminho do dinheiro aceita chave interna com metade do piso de entropia e não pode ser estreitado

Um red-team anterior quebrou a chave interna compartilhada em chaves por consumidor. O
helper `resolve_internal_auth_key` tenta primeiro a chave dedicada, cai para a
compartilhada, e exige `INTERNAL_AUTH_KEY_MIN_LEN = 32` nas duas. A remediação funcionou:
o mint de PAT any-tenant tem chave dedicada sem fallback (`internal_pat.rs:747`), a
autoridade de apagamento também, e o aprovador dual é dedicado com distinção verificada
no boot.

Dois arquivos ficaram de fora, e são os dois do dinheiro:

- `crates/corelink-container/src/routes/tier_select.rs:601` —
  `let auth_key = std::env::var("CORELINK_INTERNAL_AUTH_KEY").ok()?;` seguido de
  `if auth_key.len() < 16`.
- `crates/corelink-container/src/routes/dpa_accept.rs:585` — idem.

O checkout pago e o aceite do DPA (o registro de consentimento juridicamente vinculante)
leem a chave compartilhada crua, com piso de **16** contra os 32 de todas as outras
superfícies internas. Consequência dupla: não existe caminho de rotação para credencial
própria, porque o helper nunca é chamado; e um segredo com metade da entropia é aceito
precisamente onde o dinheiro e o consentimento passam.

Reparo: os dois arquivos passam a usar `resolve_internal_auth_key` com sua própria
variável dedicada, herdando o piso de 32 e o fallback documentado.

```backlog
id: B-074
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'n=0; det=""
  for f in crates/corelink-container/src/routes/tier_select.rs crates/corelink-container/src/routes/dpa_accept.rs; do
    [ -f "$f" ] || continue
    cru=0; grep -qE "env::var\(\"CORELINK_INTERNAL_AUTH_KEY\"\)" "$f" && cru=1
    helper=0; grep -q "resolve_internal_auth_key" "$f" && helper=1
    if [ "$cru" = 1 ] && [ "$helper" = 0 ]; then n=$((n+1)); det="$det $(basename $f)"; fi
  done
  [ "$n" -gt 0 ] || { echo "FALHA: nenhum dos dois arquivos le a chave compartilhada crua — feche o item."; exit 1; }
  echo "aberto: $n arquivo(s) do caminho do dinheiro leem CORELINK_INTERNAL_AUTH_KEY cru sem o helper:$det"'
verify-means: |
  open — `tier_select.rs` e/ou `dpa_accept.rs` leem `CORELINK_INTERNAL_AUTH_KEY` por
  `env::var` direto SEM chamar `resolve_internal_auth_key`.

  Vira DRIFTED quando os dois passarem pelo helper, que é o reparo — e o helper traz o
  piso de 32 junto, então não preciso medir o `16` separadamente. Medir o literal `16`
  seria frágil: alguém poderia trocar para `32` mantendo a leitura crua, o que conserta
  a entropia e deixa a impossibilidade de rotação intacta. Gatear no HELPER decide as
  duas metades da alegação com um único predicado.

  Conta arquivos em vez de exigir os dois: consertar um só reduz o número e mantém o
  item aberto com o detalhe de qual falta. Progresso parcial não é punido nem escondido.
last-verified: 2026-08-30
```

### B-075 — o plano de computação DevEnv autoriza por omissão, e uma falha do D1 também autoriza

O guard de `/v1/customer/devenv*` e `/v1/devenv*` (`worker/src/lib/devenv_guard.ts`)
consulta `runners_entitlement` no D1 e nega quando encontra linha negativa. Falta o ramo
`else`: **nenhuma linha encontrada** e **exceção do D1** caem ambos no caminho permitido.
O verificador adversarial tentou refutar e não conseguiu — o fluxo de controle falha
aberto nos dois casos.

O plano de computação é a superfície mais cara por requisição do produto. Autorizar por
omissão significa que um tenant sem direito ao SKU, ou qualquer tenant durante uma
indisponibilidade do D1, consome computação faturável.

Nota de colisão: **#1397 está em voo** e toca superfície devenv. Antes de escrever
código para este item, confira se aquele PR já move este guard.

```backlog
id: B-075
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'f=worker/src/lib/devenv_guard.ts
  [ -f "$f" ] || { echo "FALHA: devenv_guard.ts nao existe mais — reavalie o item."; exit 1; }
  temelse=0; grep -qE "^[[:space:]]*\}[[:space:]]*else[[:space:]]*\{|return[[:space:]]+\{[[:space:]]*allowed:[[:space:]]*false" "$f" && temelse=1
  temcatch=0; grep -qE "catch" "$f" && temcatch=1
  falhafechado=0
  if [ "$temcatch" = 1 ]; then
    awk "/catch/,/^[[:space:]]*\}/" "$f" | grep -qiE "allowed:[[:space:]]*false|deny|throw|503|403" && falhafechado=1
  fi
  if [ "$temelse" = 1 ] && [ "$falhafechado" = 1 ]; then
    echo "FALHA: guard tem ramo de negacao explicito E catch que falha fechado — feche o item."; exit 1; fi
  echo "aberto: devenv_guard sem negacao por omissao (else=$temelse) e/ou catch que nao fecha (catch=$temcatch fecha=$falhafechado)"'
verify-means: |
  open — o guard não tem ramo explícito de negação para "sem linha de entitlement", ou
  seu `catch` não fecha o acesso.

  Vira DRIFTED quando AMBOS existirem: negação explícita no caminho sem-linha E `catch`
  que nega. As duas metades são a alegação (falha aberta por omissão E por exceção), e
  consertar só uma deixa o buraco pela outra.

  O que NÃO decide, e admito francamente: este é o `verify` mais frágil do lote, porque
  lê ESTRUTURA de TypeScript por regex em vez de executar o guard. Um refator que mude
  o formato do fluxo pode falsear em qualquer direção. O certo seria um teste unitário
  do guard com D1 ausente e D1 lançando — e é isso que quem consertar deve escrever,
  fechando este item pelo teste e não pelo grep. Registro a fragilidade aqui em vez de
  deixá-la implícita.
last-verified: 2026-08-30
```

### B-076 — o mesmo tenant pode manter duas assinaturas pagáveis abertas, e a segunda apaga o registro da primeira

Cinco elos, todos verificados:

1. **O guard só olha assinaturas ativas.** `HAS_ACTIVE_PAID_CACHE_SUBSCRIPTION_SQL`
   (`tier_select_store.rs:68`) é `subscription_state = 'active' AND tier != 'free'`. Um
   tenant parado em `pending_checkout` não é `'active'` e passa.
2. **As chaves de idempotência divergem no campo errado.** `client.rs:800` é
   `format!("checkout:{}:{}", tenant_id, tier)` — inclui o tier; `client.rs:821` é
   `format!("customer:{}", tenant_id)` — mesmo cliente. Tiers diferentes produzem duas
   páginas hospedadas independentemente pagáveis sobre o mesmo `cus_`, vivas pelas 24h
   padrão de expiração de sessão da Stripe.
3. **O lock não identifica quem o detém.** `release_lock` (`tier_select_store.rs:323`) é
   `DELETE FROM tier_selection_locks WHERE tenant_id = ?1` — a coluna `correlation_id`
   existe e é descartada. Com TTL de 60s, uma chamada à Stripe que passe disso faz a
   requisição A liberar o lock que já pertence à B, no meio da orquestração dela.
4. **O empate é resolvido destruindo o registro.** `upsertBillingPaid`
   (`apps/signup-worker/src/webhooks/stripe.ts:591`) faz
   `ON CONFLICT (tenant_id) DO UPDATE SET stripe_subscription_id = excluded.…` sem
   guarda. Uma linha por tenant: a segunda assinatura sobrescreve o id da primeira, que
   continua cobrando e deixa de existir para a plataforma — inclusive para o
   `deactivateTierSelectionBySubscription`, que resolve o tenant por esse mapa e é um
   no-op documentado quando não acha nada.
5. **Nada detecta.** `FROM stripe_checkout_sessions` em todo `.rs` e `.ts`: zero
   ocorrências — a tabela é escrita e nunca lida. O `tier_select_store.rs:287` se apoia
   numa *"daily reconciliation cron"* chamada
   `corelink_onboarding_stripe_customer_id_drift_total`; esse nome aparece em quatro
   lugares no repositório e **nenhum deles é código executável**.

Ordem importa e piora: pagar Solo e depois Pro faz o guard `subscription_state <>
'active'` bloquear a troca de tier enquanto o `upsertBillingPaid` sobrescreve id E plano
— cliente com direito ao tier barato, pagando dois, com `tenant_billing.plan='pro'`
contradizendo `tier_selections.tier='solo'`. Cancelar a rastreada revoga o direito que a
não-rastreada financia.

**Antes de reparar:** reconciliar as assinaturas vivas na Stripe contra `tenant_billing`
para dimensionar a exposição existente. Colisão: **#1400** está em voo tocando
`billing-stripe-materializer`; confira antes de escrever código.

```backlog
id: B-076
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 's=crates/corelink-container/src/routes/tier_select_store.rs
  c=crates/corelink-stripe-real/src/client.rs
  w=apps/signup-worker/src/webhooks/stripe.ts
  [ -f "$s" ] && [ -f "$c" ] && [ -f "$w" ] || { echo "FALHA: arquivo sumiu — reavalie o item."; exit 1; }
  lock=0; awk "/fn release_lock/,/^    }/" "$s" | grep -q "correlation_id" || lock=1
  idem=0; grep -qE "format!\(\"customer:\{\}\"" "$c" && idem=1
  clob=0; awk "/ON CONFLICT \(tenant_id\)/,/updated_at_ms/" "$w" | grep -q "stripe_subscription_id[[:space:]]*=[[:space:]]*excluded" && clob=1
  leitor=$(grep -rl "FROM stripe_checkout_sessions" --include="*.rs" --include="*.ts" . 2>/dev/null | grep -v "/target/" | wc -l | tr -d " ")
  soma=$((lock + idem + clob))
  if [ "$soma" = 0 ] && [ "$leitor" -gt 0 ]; then
    echo "FALHA: lock por correlation_id, idempotencia por cliente corrigida, clobber guardado e a tabela tem leitor — feche o item."; exit 1; fi
  echo "aberto: lock_sem_correlation=$lock idem_customer_sem_tier=$idem clobber_sem_guarda=$clob leitores_da_tabela=$leitor"'
verify-means: |
  open — pelo menos um dos três defeitos de código persiste, OU a
  `stripe_checkout_sessions` continua sem nenhum leitor.

  Vira DRIFTED só quando os quatro forem resolvidos juntos. Escolhi o AND porque este
  item é uma CADEIA de dinheiro: consertar o clobber sem consertar o lock, ou vice-versa,
  deixa cobrança indevida possível por outro caminho. Um portão que fecha a 1/4 do
  reparo, num caminho de cobrança, é pior que portão nenhum.

  O contador `leitores_da_tabela` é o que decide a metade "nada detecta": hoje é 0, e
  qualquer leitor real (uma reconciliação de verdade, não o comentário fantasma) o
  levanta.

  O que NÃO decide, e admito: a exposição JÁ EXISTENTE em produção — quantas assinaturas
  órfãs estão cobrando agora. Isso só a reconciliação contra a Stripe viva mede, e é
  pré-requisito do reparo, não consequência dele.
last-verified: 2026-08-30
```

### B-077 — o repositório mediu o próprio contêiner e guardou o número numa constante que só um subsistema enxerga

Em 2026-08-10 o commit `9c4ee47a` (#1066, *"perf(container): right-size prod cache
container to basic (1GiB)"*) trocou `instance_type` de `standard-1` para `basic` nos sete
blocos do `wrangler.toml`. Boa decisão de custo.

O `cas.rs` reagiu de forma exemplar: mediu a caixa pela API, gravou
`CONTAINER_MEMORY_BYTES = 1024 MiB` (`cas.rs:164`, com o comentário *"0.25 vCPU / 1024
MiB on all five regions, read from the Cloudflare Containers API"*) e ancorou nela um
assert de **tempo de compilação** (`cas.rs:183`).

O defeito é que essa constante é referenciada em exatamente dois lugares, ambos dentro
do próprio `cas.rs`. O `adapter_pat.rs` nunca a vê: dimensiona `ARGON2_VERIFY_PERMITS = 16`
contra *"a standard-1 instance (~4 GiB) […] ~4x safety headroom"*, e afirma na linha 1328
que o contêiner tem **0,5 vCPU** — o dobro dos 0,25 medidos, número que o `cas.rs` tem
correto no mesmo binário.

Ressalva registrada, porque a primeira versão deste achado errava: permits limitam
concorrência, não reservam memória (os 64 MiB são alocados dentro do `spawn_blocking`);
`ARGON2_PER_TENANT_PERMITS = 4` mais coalescing por chave limitam um tenant sozinho a
256 MiB; e `p_cost = 4` não paraleliza, porque `corelink-pat/Cargo.toml:17` compila
argon2 com `default-features = false`, sem feature de paralelismo. Saturar os 16 exige
quatro tenants distintos com quatro PATs frios cada — condição adversarial, não uma
matriz de CI.

O que sobra e é durável: um teto de DoS cuja aritmética nomeia um tipo de instância que o
repositório não implanta, contradito por uma constante medida no mesmo binário. O
`cas.rs` compile-asserta que pode reivindicar metade da caixa; o Argon2 dimensiona-se
para a caixa inteira; nenhum dos dois referencia o outro. São ~1,5 GiB de orçamento
documentado sobre 1 GiB físico. **A metade do CAS é [B-056]; a metade do Argon2 é este
item.** E não existe portão ligando `instance_type` às constantes derivadas dele: o
próximo redimensionamento repete isto em silêncio.

```backlog
id: B-077
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'a=crates/corelink-container/src/adapter_pat.rs
  c=crates/corelink-container/src/routes/cas.rs
  [ -f "$a" ] && [ -f "$c" ] || { echo "FALHA: arquivo sumiu — reavalie o item."; exit 1; }
  medida=$(grep -c "CONTAINER_MEMORY_BYTES" "$c" 2>/dev/null | tr -d " ")
  [ "$medida" -gt 0 ] || { echo "FALHA: cas.rs nao define mais CONTAINER_MEMORY_BYTES — reavalie o item."; exit 1; }
  fora=$(grep -rl "CONTAINER_MEMORY_BYTES" crates/ --include="*.rs" 2>/dev/null | grep -v "routes/cas.rs" | wc -l | tr -d " ")
  velho=0; grep -qE "standard-1|~4 GiB|0\.5 vCPU" "$a" && velho=1
  if [ "$fora" -gt 0 ] && [ "$velho" = 0 ]; then
    echo "FALHA: CONTAINER_MEMORY_BYTES ja e usado fora do cas.rs E adapter_pat nao cita mais a instancia velha — feche o item."; exit 1; fi
  echo "aberto: arquivos_usando_a_constante_fora_do_cas=$fora  adapter_pat_ainda_cita_standard-1_ou_0.5vCPU=$velho"'
verify-means: |
  open — a constante medida continua confinada ao `cas.rs`, OU o `adapter_pat.rs` ainda
  dimensiona contra `standard-1` / `~4 GiB` / `0.5 vCPU`.

  Vira DRIFTED quando AMBOS forem resolvidos: a constante virar orçamento compartilhado
  E o `adapter_pat` parar de citar a instância antiga. O AND é a alegação: promover a
  constante sem recalcular o pool deixa o número errado, e recalcular sem compartilhar
  deixa o próximo redimensionamento repetir tudo.

  Cross-ref: a metade process-wide do CAS é [B-056]. Este item NÃO a duplica — decide
  especificamente a não-propagação da constante e o dimensionamento órfão do Argon2.

  O reparo estrutural que fecha os dois de vez é um portão ligando `instance_type` do
  `wrangler.toml` às constantes derivadas dele. Se alguém escrever esse portão, escreva
  o item novo em vez de estender este.
last-verified: 2026-08-30
```

### B-078 — `batch-read` materializa todos os blobs em memória antes de aplicar o teto de 8 MiB

Único achado classificado como alto entre os 34 do pen-test de 2026-08-30, sobrevivendo à
refutação adversarial. O `handle_batch_read` (`crates/corelink-container/src/routes/cas.rs`)
dispara até `BATCH_MAX_OBJECTS = 2_000` tarefas em um laço que roda até o fim ANTES de o
teto `BATCH_MAX_BYTES = 8 MiB` ser aplicado. O limite é verificado na saída, não na
entrada.

Uma única requisição autenticada esgota a memória do contêiner multi-tenant compartilhado.
Composto com [B-077] sobre a mesma instância de 1 GiB, a superfície é bem menor do que a
análise original de 4 GiB supunha.

O próprio código conhece a forma do problema: o comentário em `cas.rs:235-239` calcula o
risco de leituras em massa concorrentes como `N × payload heap (≈ N × 8 MiB) on the
shared container`, e por isso existe o `CasReadConcurrencyGuard`. O que falta é aplicar o
teto de bytes na ENTRADA, antes da materialização, e não depois.

```backlog
id: B-078
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'f=crates/corelink-container/src/routes/cas.rs
  [ -f "$f" ] || { echo "FALHA: cas.rs sumiu — reavalie o item."; exit 1; }
  grep -q "BATCH_MAX_BYTES" "$f" || { echo "FALHA: BATCH_MAX_BYTES nao existe mais — reavalie o item."; exit 1; }
  corpo=$(awk "/fn handle_batch_read/,/^async fn |^pub async fn |^fn /" "$f" | head -200)
  linha_cap=$(printf "%s" "$corpo" | grep -n "BATCH_MAX_BYTES" | head -1 | cut -d: -f1)
  linha_fan=$(printf "%s" "$corpo" | grep -nE "spawn|join_all|JoinSet|futures::" | head -1 | cut -d: -f1)
  if [ -n "$linha_cap" ] && [ -n "$linha_fan" ] && [ "$linha_cap" -lt "$linha_fan" ]; then
    echo "FALHA: o teto de bytes e aplicado ANTES do fan-out (cap@$linha_cap fanout@$linha_fan) — feche o item."; exit 1; fi
  echo "aberto: teto de bytes aplicado depois do fan-out (cap@${linha_cap:-ausente} fanout@${linha_fan:-ausente})"'
verify-means: |
  open — dentro de `handle_batch_read`, a primeira menção a `BATCH_MAX_BYTES` aparece
  DEPOIS da primeira construção de fan-out concorrente. Ou seja: materializa, depois
  mede.

  Vira DRIFTED quando o teto passar a ser aplicado antes do fan-out, que é o reparo.

  O que NÃO decide, e admito com todas as letras: isto lê ORDEM TEXTUAL de linhas, não
  ordem de execução. Um refator que extraia o fan-out para uma função auxiliar falsearia
  o resultado nas duas direções. É o `verify` mais fraco deste lote junto com [B-075], e
  registro isso em vez de deixar implícito.

  O reparo correto traz o `verify` correto junto: um teste que envie um lote cuja soma
  declarada exceda 8 MiB e exija 413 ANTES de qualquer leitura do R2. Quem consertar
  deve substituir este comando por esse teste.
last-verified: 2026-08-30
```

### B-079 — o tier Max é publicado a 4.000 rps e limitado a 1.000, e as duas resoluções de tier falham na direção mais generosa

**Na direção do cliente que paga.** `crates/corelink-ratelimit/src/tier.rs:124` mapeia
`"pro" | "org" | "max" => Tier::Business`, que é 1.000 rps / 5.000 burst. A página
publicada (`apps/docs/docs/explanation/rate-limits.mdx:45`) anuncia Max a 4.000 / 20.000.
Um cliente Max paga $149/mês e recebe um quarto da taxa publicada — dano faturável e
diretamente demonstrável por ele. O `tier.rs:97` documenta a decisão deliberadamente
(*"`max` | `Business` — NOT Enterprise (ratified Q5a…)"*); a página nunca acompanhou.

**Na direção oposta.** `tier_for_billing_label` termina em `_ => Tier::Team` (200 rps,
20× o gratuito) e `refill_rate_for_tier` termina em `_ => ENTERPRISE_RATE` (10.000 rps,
1000× o gratuito). Um rótulo de cobrança corrompido, com erro de grafia, ou de um tier
futuro recebe taxa paga; uma variante de enum desconhecida recebe taxa Enterprise. Ambas
documentadas como escolha deliberada (*"most-permissive default"*), postura defensável
para disponibilidade — mas num controle **medido e vendido** significa que o limitador
falha na direção do vazamento de receita.

Nota composta com [B-071]: o comentário do `tier.rs` observa que este mapeamento também
seleciona a escada de TTL de eviction e é portanto uma *"customer-visible retention
promise"* — promessa hoje inerte, porque não há eviction rodando.

Reparo: decidir qual dos dois números é a verdade (a página ou o código) e alinhar; e
decidir se os fallbacks devem cair para `Free` em vez de para tier pago.

```backlog
id: B-079
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 't=crates/corelink-ratelimit/src/tier.rs
  d=apps/docs/docs/explanation/rate-limits.mdx
  [ -f "$t" ] || { echo "FALHA: tier.rs sumiu — reavalie o item."; exit 1; }
  maxbiz=0; grep -qE "\"max\"[^=]*=>[[:space:]]*Tier::Business|\|[[:space:]]*\"max\"[[:space:]]*=>" "$t" && maxbiz=1
  docs4k=0; [ -f "$d" ] && grep -qE "4[ ,.]?000" "$d" && docs4k=1
  fbteam=0; grep -qE "_[[:space:]]*=>[[:space:]]*Tier::Team" "$t" && fbteam=1
  fbent=0; grep -qE "_[[:space:]]*=>[[:space:]]*\(ENTERPRISE_REFILL_RPS" "$t" && fbent=1
  desalinhado=0; [ "$maxbiz" = 1 ] && [ "$docs4k" = 1 ] && desalinhado=1
  soma=$((desalinhado + fbteam + fbent))
  [ "$soma" -gt 0 ] || { echo "FALHA: Max alinhado com a pagina E fallbacks nao caem mais em tier pago — feche o item."; exit 1; }
  echo "aberto: max_mapeado_para_business_com_docs_dizendo_4000=$desalinhado fallback_string_para_Team=$fbteam fallback_enum_para_Enterprise=$fbent"'
verify-means: |
  open — o Max continua mapeado para `Business` enquanto a página publica 4.000, OU
  algum dos dois fallbacks continua caindo em tier pago.

  Vira DRIFTED quando os três forem resolvidos. Aceita QUALQUER das duas correções do
  desalinhamento: mudar o código para `Enterprise`, ou corrigir a página para 1.000 —
  são decisões de produto diferentes com o mesmo efeito sobre a alegação, que é a
  DIVERGÊNCIA, não qual dos lados está certo.

  O que NÃO decide, e admito: se o número novo da página bate exatamente com a constante
  nova do código. O comando detecta a presença de "4.000" na página e o mapeamento para
  `Business`; um terceiro valor em ambos os lados passaria despercebido. Um `verify`
  robusto exigiria parsear a tabela da página e a escada do `tier.rs` e compará-las —
  vale escrever quando alguém consertar, e aí substituir este comando.
last-verified: 2026-08-30
```

### B-080 — metade dos escopos canônicos de PAT não é verificada por nenhum ponto de aplicação

`crates/corelink-pat/src/scopes.rs:157` define doze escopos canônicos. O ponto de
aplicação em `crates/corelink-container/src/scope.rs:77` reconhece seis: `cas:rw`,
`cas:r`, `cas:w`, `read-write`, `read-only`, `admin`.

Os escopos `admin:tenant-read`, `admin:tenant-write`, `admin:tokens`, `admin:billing`,
`admin:audit` e `admin:users` podem ser cunhados num token e nunca são consultados. Um
token emitido com `admin:audit` não recebe menos privilégio que um emitido com `admin` —
recebe o mesmo, porque a distinção não é lida.

Qualquer plano de privilégio mínimo baseado nesses nomes é decorativo. Isso importa em
particular para clientes enterprise, para quem a granularidade de escopo costuma ser
requisito de contrato.

Dois reparos legítimos: implementar a verificação dos seis, ou remover os escopos que
não são aplicados para que ninguém construa política sobre eles. O segundo é honesto e
mais barato; o primeiro é o que o produto promete.

```backlog
id: B-080
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 's=crates/corelink-pat/src/scopes.rs
  e=crates/corelink-container/src/scope.rs
  [ -f "$s" ] && [ -f "$e" ] || { echo "FALHA: arquivo sumiu — reavalie o item."; exit 1; }
  naoaplicados=0; faltando=""
  for sc in admin:tenant-read admin:tenant-write admin:tokens admin:billing admin:audit admin:users; do
    if grep -q "$sc" "$s" 2>/dev/null; then
      grep -q "$sc" "$e" 2>/dev/null || { naoaplicados=$((naoaplicados+1)); faltando="$faltando $sc"; }
    fi
  done
  [ "$naoaplicados" -gt 0 ] || { echo "FALHA: todo escopo admin:* definido tambem aparece no ponto de aplicacao — feche o item."; exit 1; }
  echo "aberto: $naoaplicados escopo(s) admin:* definidos e nunca verificados:$faltando"'
verify-means: |
  open — existe pelo menos um escopo `admin:*` definido em `scopes.rs` que não aparece
  em `scope.rs`.

  Vira DRIFTED por qualquer um dos dois reparos: os escopos passam a ser verificados (o
  bom), ou são removidos da definição (o honesto). O laço só conta escopos que EXISTEM
  na definição, então remover fecha o item naturalmente.

  Conta em vez de exigir zero-ou-tudo: implementar dois dos seis reduz o número e mantém
  o item aberto listando os que faltam. Progresso parcial aparece.

  O que NÃO decide, e admito: se a menção em `scope.rs` é uma verificação REAL ou só o
  nome aparecendo numa lista. É a fraqueza de gatear por presença de string. Quem
  consertar deve acompanhar de teste que prove negação — um token com `admin:audit`
  recusado numa rota de billing — e trocar este comando por ele.
last-verified: 2026-08-30
```

### B-081 — seguir o runbook de rotação da chave de assinatura de PAT causa indisponibilidade

O verificador do contêiner aceita chaves de transição: `adapter_pat.rs:1466` itera sobre
`["PAT_SIGNING_KEY_PREV", "PAT_SIGNING_KEY_NEW"]`. O Durable Object encaminha apenas a
chave corrente — `worker/src/durable_object.ts:848` passa
`PAT_SIGNING_KEY: this.env.PAT_SIGNING_KEY ?? ""` e não encaminha as duas irmãs.

O mecanismo de rotação sem interrupção existe do lado do contêiner e é **inalcançável**,
porque as chaves de transição nunca chegam até ele. Um operador que siga o procedimento
documentado invalida todos os PATs emitidos sob a chave anterior no instante da troca.

É latente: só se manifesta quando alguém rotacionar. Exatamente por isso merece reparo
antes, e não depois — o custo de descobrir isto durante uma rotação de emergência é uma
indisponibilidade autoinfligida no pior momento possível.

Reparo: duas linhas no `container.start({ env })`, mais uma linha na matriz de secrets
para cada uma das duas variáveis.

```backlog
id: B-081
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'a=crates/corelink-container/src/adapter_pat.rs
  d=worker/src/durable_object.ts
  [ -f "$a" ] && [ -f "$d" ] || { echo "FALHA: arquivo sumiu — reavalie o item."; exit 1; }
  aceita=0; grep -q "PAT_SIGNING_KEY_PREV" "$a" && aceita=1
  [ "$aceita" = 1 ] || { echo "FALHA: o container nao aceita mais chaves de transicao — reavalie o item."; exit 1; }
  enc=0
  grep -q "PAT_SIGNING_KEY_PREV" "$d" && enc=$((enc+1))
  grep -q "PAT_SIGNING_KEY_NEW" "$d" && enc=$((enc+1))
  [ "$enc" -lt 2 ] || { echo "FALHA: o DO ja encaminha as duas chaves de transicao — feche o item."; exit 1; }
  echo "aberto: container aceita PREV/NEW e o DO encaminha $enc de 2"'
verify-means: |
  open — o contêiner aceita `PAT_SIGNING_KEY_PREV` E o Durable Object encaminha menos
  que as duas chaves de transição.

  Vira DRIFTED quando o DO encaminhar as duas, que é o reparo. Fecharia também se o
  contêiner deixasse de aceitar chaves de transição — mas nesse caso a rotação sem
  interrupção deixa de existir por decisão, e isso merece item próprio de recusa, não o
  fechamento silencioso deste.

  Conta 0/1/2 em vez de exigir ambas: encaminhar só uma é um reparo pela metade que
  ainda quebra a rotação, e o número mostra isso em vez de esconder.

  Depende de [B-067] na sequência: é reparo de credencial, e `corelink-pat` hoje não tem
  execução de teste em CI.
last-verified: 2026-08-30
```

### B-082 — a sonda profunda de saúde do contêiner existe, funciona, e não é alcançável por ninguém

`worker/src/index.ts:861` documenta que `/_health/container` expõe o campo `storage`, que
revela se um handler caiu para o armazenamento em memória. A linha 2116 do mesmo arquivo
executa `delete raw["storage"]`.

A remoção é deliberada e **correta** — foi reparo de segurança, para não vazar topologia
de armazenamento a um chamador anônimo. O defeito é que nenhuma variante autenticada foi
criada em seu lugar. O sinal existe, é produzido pela sonda profunda que de fato alcança
o DO `_system`, e é descartado antes de chegar a qualquer consumidor, inclusive ao
operador. E o comentário da linha 861 continua descrevendo o comportamento antigo.

Consequência operacional concreta: o modo de falha "o handler subiu com armazenamento em
memória" é indetectável de fora. É precisamente o que uma sonda de saúde existe para
detectar.

Reparo: variante autenticada de `/_health/container` que preserve `storage` atrás do
`CORELINK_ADMIN_AUTH_KEY`, e corrigir o comentário da linha 861.

```backlog
id: B-082
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'f=worker/src/index.ts
  [ -f "$f" ] || { echo "FALHA: index.ts sumiu — reavalie o item."; exit 1; }
  strip=0; grep -qE "delete raw\[\"storage\"\]|delete raw\[.storage.\]" "$f" && strip=1
  [ "$strip" = 1 ] || { echo "FALHA: o campo storage nao e mais removido — feche o item ou reescreva-o."; exit 1; }
  autent=0
  awk "/delete raw\[/{n=NR} n&&NR>=n-40&&NR<=n+40" "$f" | grep -qiE "ADMIN_AUTH_KEY|authenticated variant|health_container_authed" && autent=1
  [ "$autent" = 0 ] || { echo "FALHA: existe caminho autenticado na sonda de container — feche o item."; exit 1; }
  echo "aberto: campo storage removido e nenhuma variante autenticada da sonda"'
verify-means: |
  open — o campo `storage` continua sendo removido E não existe caminho autenticado que
  o preserve. As duas metades são a alegação: a remoção é correta, a ausência de
  alternativa é o defeito.

  Vira DRIFTED quando aparecer a variante autenticada. Se alguém simplesmente parar de
  remover o campo, o primeiro ramo falha e o item também fecha — mas isso seria REGRESSÃO
  de segurança, então a mensagem manda reavaliar em vez de fechar cegamente. Registro
  essa ambiguidade de propósito: prefiro um portão que peça julgamento a um que aprove
  o desfazimento de um reparo.

  A correção do comentário da linha 861 é reportada na prosa e não gateada: é
  documentação, e travar o item nela atrasaria a variante autenticada, que é o que
  importa.
last-verified: 2026-08-30
```

### B-083 — o BYOK é vendido a $99/mês, consta do SLA assinado, e é um `XOR` em memória no binário embarcado

Três elos, todos verificados no código de 2026-08-30:

1. **O binário não ativa as features reais.** `Dockerfile:182` é
   `cargo build --release --locked -p corelink-server --bin corelink-server;` — sem
   `--features`. `crates/corelink-container/Cargo.toml:16` declara `default = []`, e as
   quatro features `byok-aws-real` (:35), `byok-gcp-real` (:47), `byok-azure-real` (:55)
   e `byok-vault-real` (:63) não são ativadas por nenhum caminho de build.
2. **O próprio código admite.** `routes/byok_admin.rs:249` — `if !REAL_KMS_PROVIDER_WIRED
   { tracing::error!(event = "ByokActivateNotAvailable", …); return
   (StatusCode::NOT_IMPLEMENTED, "byok_not_available") }`.
3. **O kill switch não é chamado.** `crates/corelink-byok/src/byok_revocation/detector.rs:147`
   define `pub async fn run_loop(self)`, descrita em comentário como *"the core kill
   switch implementation"*. Suas únicas referências estão dentro do próprio crate e em
   testes; nenhum caminho do binário embarcado a invoca.

O que **S** afirma: `legal/sla/v1.0.0.md:46` compromete, para Enterprise, *"BYOK
kill-switch p99 ≤ 5 min"*. O `marketing/sales/FAQ-MASTER.md:71` vende o add-on a $99/mês
e afirma textualmente *"we don't run BYOK as a marketing checkbox; the kill switch is
exercised on a schedule."*

Este item cobre o defeito de engenharia (o binário). A reconciliação dos instrumentos
assinados é [B-087] e a evidência falsa é [B-084].

```backlog
id: B-083
repo: corelink-server
owner: owner
status: open
verify: |
  bash -c 'd=Dockerfile
  c=crates/corelink-container/Cargo.toml
  [ -f "$d" ] && [ -f "$c" ] || { echo "FALHA: arquivo sumiu — reavalie o item."; exit 1; }
  buildline=$(grep -E "cargo build.*-p corelink-server" "$d" | head -1)
  [ -n "$buildline" ] || { echo "FALHA: a linha de build do corelink-server mudou — reavalie o item."; exit 1; }
  temfeat=0; printf "%s" "$buildline" | grep -qE "byok-(aws|gcp|azure|vault)-real" && temfeat=1
  defvazio=0; grep -qE "^default[[:space:]]*=[[:space:]]*\[\]" "$c" && defvazio=1
  defbyok=0; grep -E "^default[[:space:]]*=" "$c" | grep -q "byok" && defbyok=1
  if [ "$temfeat" = 1 ] || [ "$defbyok" = 1 ]; then
    echo "FALHA: o build embarca alguma feature byok-*-real (cmdline=$temfeat default=$defbyok) — feche o item."; exit 1; fi
  echo "aberto: Dockerfile constroi sem --features byok-*-real e default=[] (default_vazio=$defvazio)"'
verify-means: |
  open — a linha de build do `Dockerfile` não passa nenhuma feature `byok-*-real` E o
  `default` do crate não as inclui. As duas metades cobrem os dois caminhos possíveis de
  ativação, então nenhuma delas sozinha decide.

  Vira DRIFTED quando qualquer um dos dois caminhos passar a embarcar um provedor real,
  que é o reparo.

  O que NÃO decide, e admito: se o provedor embarcado FUNCIONA contra um KMS real, e se
  o `run_loop` do kill switch passa a ser chamado. Compilar a feature é condição
  necessária, não suficiente. Quem fechar deve provar com um drill REAL — que é
  exatamente o que [B-084] cobra — e não com a presença da flag.

  Owner, não tl: embarcar BYOK real toca credencial de KMS de cliente e muda a superfície
  vendida. Não é decisão de engenharia.
last-verified: 2026-08-30
```

### B-084 — o drill do kill switch emite atestado de aprovação a partir de um `sleep`, e o atestado é encaminhado a clientes

`scripts/byok_kill_switch_drill.sh` produz um relatório de aprovação sem exercitar nada:
`:50 # Here: simulated pass in CI.`, `:80-81 # CI: simulate detection after 2s. / sleep 2`,
`:99 # CI: simulated pass.`, `:116 SLA_RESULT="PASS"`, `:126 sleep 1 # simulated`,
`:136 cat > "${REPORT_FILE}" << EOF`.

As chamadas reais ao KMS estão comentadas; todas as credenciais no workflow
(`byok_kill_switch_drill_weekly.yml:48-51`) estão comentadas. O script então escreve
`specs/_audits/AAAA-MM-DD-byok-kill-switch-drill-*.md` com uma tabela de aprovação, e
esse arquivo é encaminhado ao SRE do cliente como evidência de SLA por instrução de
`lighthouse-kit/05-sla-attestation-instructions.md:61`.

É um gerador automático de evidência de segurança falsa. Categoricamente diferente de um
controle ausente ([B-083]): ali falta o controle; aqui se produz prova de que ele existe.

**Reparo mínimo, uma linha:** fazer o script sair com código diferente de zero em modo
simulado, para que não possa emitir `PASS`. É a melhor relação entre exposição removida e
esforço de todo o repositório, e não depende de [B-083] estar resolvido.

```backlog
id: B-084
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 's=scripts/byok_kill_switch_drill.sh
  [ -f "$s" ] || { echo "FALHA: o script do drill sumiu — feche o item ou reescreva-o."; exit 1; }
  hard=0; grep -qE "SLA_RESULT=\"?PASS\"?" "$s" && hard=1
  sim=0; grep -qiE "simulated|simulate detection" "$s" && sim=1
  guarda=0
  awk "/SLA_RESULT=/{n=NR} n&&NR>n&&NR<n+12&&/exit[[:space:]]+[1-9]/{print;exit}" "$s" | grep -q . && guarda=1
  if [ "$hard" = 0 ] || [ "$guarda" = 1 ]; then
    echo "FALHA: PASS hardcoded removido ($hard) ou ha saida nao-zero no caminho simulado ($guarda) — feche o item."; exit 1; fi
  echo "aberto: SLA_RESULT=PASS hardcoded e modo simulado ainda sai zero (sim=$sim)"'
verify-means: |
  open — o script ainda contém `SLA_RESULT="PASS"` fixo E não há saída não-zero no
  caminho simulado.

  Vira DRIFTED por qualquer um dos dois reparos: remover o `PASS` hardcoded (o drill
  passa a derivar o resultado do que mediu), ou fazer o modo simulado sair não-zero (o
  reparo de uma linha). Aceito os dois porque ambos impedem a emissão de atestado falso,
  que é a alegação.

  O que NÃO decide, e admito: se o atestado JÁ EMITIDO e encaminhado a clientes será
  retratado. Arquivos em `specs/_audits/` com tabela de PASS existem e foram distribuídos.
  Retratá-los é decisão de comunicação com cliente, pertence ao owner, e deve virar item
  próprio se a decisão for retratar.
last-verified: 2026-08-30
```

### B-085 — a página LGPD promete São Paulo e imutabilidade à prova de ordem judicial; os dados estão nos EUA e são deletáveis

`apps/docs/docs/explanation/residency/lgpd-brazil.mdx` está publicada, sem marca de
rascunho, e afirma a titulares brasileiros que com o tenant pinado em `sam` os dados
estão *"fisicamente localizados na região da América do Sul (São Paulo) … bucket R2
`cas-sam`"*, e que `audit-sam` tem *"Object Lock 7 anos (imutável; não podemos apagar um
evento de auditoria mesmo sob ordem judicial)"*.

A realidade, em três fontes: `scripts/provision-cf-corelink-prod.sh:475` provisiona
`corelink-ac-sam` com `locationHint=enam` (Eastern North America); `wrangler.toml:792`
liga `[[env.prod-sam.r2_buckets]]` ao `bucket_name = "corelink-cas-prod"`, o bucket dos
EUA; e não existe bucket `cas-sam` em lugar algum.

O `worker/src/region-map.ts:66` é explícito sobre por que `sam` foi excluído do conjunto
provisionável: *"Cloudflare has no SAM region … its data would mis-land in US R2 under a
false residency label."* A página faz exatamente o que esse comentário adverte.

A afirmação de Object Lock aparece também no DPA executado (`legal/dpa/v1.0.0.en-US.md:110`),
nos avisos de privacidade publicados, e nos templates de notificação de violação
endereçados à DPC irlandesa e à ANPD. É representação factual a reguladores.

Cross-ref: a impossibilidade técnica do WORM em R2 já é [B-046] (`NotImplemented` medido
contra a conta de produção). **Este item não a duplica** — cobre a página LGPD publicada
e a propagação da afirmação para os instrumentos.

```backlog
id: B-085
repo: corelink-server
owner: owner
status: open
verify: |
  bash -c 'p=apps/docs/docs/explanation/residency/lgpd-brazil.mdx
  [ -f "$p" ] || { echo "FALHA: a pagina LGPD sumiu — feche o item ou reescreva-o."; exit 1; }
  saopaulo=0; grep -qiE "sao paulo|são paulo|cas-sam" "$p" && saopaulo=1
  lock=0; grep -qiE "object lock|ordem judicial|court order" "$p" && lock=1
  busketeua=0; grep -A6 "env.prod-sam.r2_buckets" wrangler.toml 2>/dev/null | grep -q "corelink-cas-prod" && busketeua=1
  soma=$((saopaulo + lock))
  if [ "$soma" = 0 ]; then echo "FALHA: a pagina nao promete mais Sao Paulo nem Object Lock — feche o item."; exit 1; fi
  if [ "$busketeua" = 0 ]; then echo "FALHA: prod-sam nao aponta mais para o bucket dos EUA — reavalie o item."; exit 1; fi
  echo "aberto: pagina promete sao_paulo=$saopaulo object_lock=$lock e prod-sam ainda liga ao corelink-cas-prod"'
verify-means: |
  open — a página publicada ainda promete localização em São Paulo ou imutabilidade por
  Object Lock, E o `prod-sam` ainda liga ao bucket dos EUA. As duas metades juntas são a
  divergência.

  Vira DRIFTED por qualquer um dos dois reparos legítimos: corrigir a página (barato,
  imediato, e é o que a realidade permite hoje), ou provisionar de fato armazenamento em
  São Paulo com WORM (caro, e hoje impossível em R2 — ver [B-046]).

  O que NÃO decide, e admito: a propagação da mesma afirmação para o DPA executado, os
  avisos de privacidade e os dois templates para reguladores. Esses arquivos são
  instrumentos jurídicos e a correção deles é de [B-087] junto com o resto da
  reconciliação — não porque sejam menos graves, mas porque exigem revisão jurídica e não
  cabem no mesmo PR de documentação.

  Owner: corrigir texto publicado a titulares e reguladores não é decisão de engenharia.
last-verified: 2026-08-30
```

### B-086 — um único D1 global atende as cinco regiões, enquanto o instrumento assinado nomeia o D1 entre os serviços fixados por tenant

Os cinco blocos de produção do `wrangler.toml` — linhas 540, 870, 1036, 1196 e 1352 —
ligam o **mesmo** `database_id = "d64742ea-e102-40b2-a844-ff02e3f94562"`. O que vive nesse
banco inclui `tenant`, `team_member` (identificador Clerk em claro e hash de e-mail),
`pat`, quotas, estado de cobrança e o outbox de auditoria.

O que **S** declara, na tabela de sub-processadores de `legal/dpa-residency-amendment.md`:
*"Cloudflare, Inc. | Infrastructure: Workers, R2, **D1**, KV, Durable Objects, Custom
Domains | … | **Tenant-pinned (Section 7)**"*. E a linha 144: *"WEUR data NEVER replicates
outside the EU jurisdiction. This restriction is enforced at the infrastructure level
(Cloudflare DO `jurisdictional_restriction`)."*

As únicas chaves `jurisdiction = "eu"` em toda a configuração estão nas linhas 958 e 971
do `wrangler.toml`, e ambas são bindings de **bucket R2**. O binding do D1 não tem chave
de jurisdição alguma.

A engenharia de residência é real e cobre bytes. O contrato afirma que cobre o D1. Dois
caminhos: provisionar D1 por jurisdição, ou emendar as cláusulas contratuais e a avaliação
de impacto de transferência para divulgar dados pessoais de plano de controle residentes
nos EUA. É base de transferência do Art. 46 — não é questão que se resolva depois do
lançamento.

```backlog
id: B-086
repo: corelink-server
owner: owner
status: open
verify: |
  bash -c 'ids=$(grep -E "^database_id[[:space:]]*=" wrangler.toml | grep -oE "\"[0-9a-f-]{36}\"" | sort -u | wc -l | tr -d " ")
  ocorr=$(grep -cE "^database_id[[:space:]]*=[[:space:]]*\"[0-9a-f-]{36}\"" wrangler.toml | tr -d " ")
  [ "$ocorr" -ge 2 ] || { echo "FALHA: menos de 2 database_id reais no wrangler.toml — reavalie o item."; exit 1; }
  amend=legal/dpa-residency-amendment.md
  declara=0
  [ -f "$amend" ] && grep -qE "D1" "$amend" && grep -qiE "tenant-pinned" "$amend" && declara=1
  jd=$(grep -c "^jurisdiction" wrangler.toml | tr -d " ")
  if [ "$ids" -gt 1 ] || [ "$declara" = 0 ]; then
    echo "FALHA: ha $ids database_id distintos ou o DPA nao declara mais D1 tenant-pinned — feche ou reescreva o item."; exit 1; fi
  echo "aberto: 1 unico database_id em $ocorr blocos de prod, DPA declara D1 tenant-pinned, chaves jurisdiction no toml=$jd (todas em R2)"'
verify-means: |
  open — existe UM único `database_id` distinto em todos os blocos de produção E o
  aditivo de residência continua declarando D1 como tenant-pinned. As duas metades são a
  divergência entre `S` e `I`.

  Vira DRIFTED por qualquer um dos dois reparos: provisionar D1 por jurisdição (o número
  de ids distintos sobe), ou emendar o instrumento para não afirmar que o D1 é
  tenant-pinned. Os dois encerram a divergência, e a escolha entre eles é jurídica e de
  custo, não técnica.

  O que NÃO decide, e admito: ONDE fisicamente reside o primário desse D1. A alegação
  verificável é a divergência entre um banco único e um contrato que promete fixação por
  tenant; a localização do primário exigiria a API da Cloudflare e não muda a conclusão.

  Owner: base de transferência internacional é decisão jurídica.
last-verified: 2026-08-30
```

### B-087 — o CAIQ v4 entregue a compradores atesta "Y" para três controles que nunca executaram com sucesso

`marketing/sales/legal-questionnaires/CAIQ-V4-pre-filled.md` é documento voltado ao
cliente, mapeado a critérios SOC 2, e escopado pelo próprio cabeçalho para *"enterprise
procurement RFP attachment, or hyperscaler-marketplace listing"*. Três linhas não se
sustentam:

- **STA-08.1** — *"Software supply chain attestation? **Y** | SLSA Level 3 + Cosign +
  Rekor + CycloneDX SBOM + reproducible builds"*.
- **STA-11.1** — *"Build provenance verifiable? **Y** | Rekor public transparency log
  entries; offline verification documented"*.
- **AIS-04.1** — *"Application security testing? **Y** | CodeQL + Semgrep on every PR;
  cargo-fuzz daily"*.

Contra o verificado: `release-cli.yml:254` imprime *"SIGNING PLACEHOLDER: cosign keyless
signing not yet wired"*; `cosign-sign.yml:210` carrega `ZONE_ID_PLACEHOLDER` e a lane
nunca executou; não existe entrada Rekor; SLSA L3 foi explicitamente recusado por exigir
builder hosted ([B-031], done). Dos três controles de AIS-04.1, o CodeQL é noturno e não
por PR e está hosted-blocked (#1434), o Semgrep foi estacionado com zero sucessos, e o
`fuzz-nightly` teve o cron comentado (*"Currently the ONLY trigger"* para o dispatch) com
1 cancelamento e 5 falhas, nada desde 2026-08-03.

Ressalva justa e registrada: a linha CCC-07.1 sobre commits assinados **se sustenta** —
os commits carregam `gpgsig`.

Este item também é o guarda-chuva da reconciliação dos demais instrumentos assinados: o
BYOK do SLA ([B-083]), o Object Lock do DPA e dos templates a reguladores ([B-085]), e a
residência do D1 ([B-086]). Um CAIQ entra no processo de risco do comprador e costuma ser
garantido como verdadeiro no contrato principal — correção é barata antes de assinar e
cara depois.

```backlog
id: B-087
repo: corelink-server
owner: owner
status: open
verify: |
  bash -c 'q=marketing/sales/legal-questionnaires/CAIQ-V4-pre-filled.md
  [ -f "$q" ] || { echo "FALHA: o CAIQ sumiu — feche o item ou reescreva-o."; exit 1; }
  ys=0
  grep -E "^\|[[:space:]]*STA-08\.1" "$q" | grep -q "| Y " && ys=$((ys+1))
  grep -E "^\|[[:space:]]*STA-11\.1" "$q" | grep -q "| Y " && ys=$((ys+1))
  grep -E "^\|[[:space:]]*AIS-04\.1" "$q" | grep -q "| Y " && ys=$((ys+1))
  ph=0
  grep -rqE "SIGNING PLACEHOLDER|ZONE_ID_PLACEHOLDER" .github/workflows/ && ph=1
  if [ "$ys" = 0 ]; then echo "FALHA: as tres linhas do CAIQ nao atestam mais Y — feche o item."; exit 1; fi
  if [ "$ph" = 0 ]; then echo "FALHA: os placeholders de assinatura sumiram — reavalie: talvez o controle exista agora."; exit 1; fi
  echo "aberto: $ys de 3 linhas do CAIQ ainda atestam Y e os placeholders de assinatura seguem no CI"'
verify-means: |
  open — pelo menos uma das três linhas ainda atesta "Y" E os placeholders de assinatura
  continuam nos workflows. As duas metades são a alegação: a afirmação existe E o
  controle que ela nomeia não.

  Vira DRIFTED por qualquer um dos dois reparos: corrigir as linhas para "N"/"P" com
  plano datado (barato, honesto, imediato), ou implementar de fato a assinatura e a
  proveniência (caro, e SLSA L3 já foi recusado em [B-031]).

  Conta 3/2/1 em vez de exigir zero: corrigir uma linha reduz o número e mantém o item
  aberto. Progresso parcial aparece.

  O que NÃO decide, e admito: quais prospects já receberam a versão atual do documento, e
  se precisam ser notificados. Isso é registro comercial fora do repositório e pertence
  ao owner — deve virar item próprio se a decisão for notificar.
last-verified: 2026-08-30
```

### B-088 — o comunicado de lançamento afirma pentest externo limpo; o próprio repositório instrui a não afirmar isso

`marketing/launch/PRESS-RELEASE.md:26` declara *"External pentest, clean"*, e a linha 39
traz citação atribuída a `[CEO_NAME]` — marcador de substituição nunca preenchido.

`reports/pentest-rfp-tracker.json` lista cinco fornecedores, todos com `NOT_CONTACTED` e
`rfp_sent_date: null`.

E `marketing/sales/PROOF-POINTS.md:71` — documento destinado à mesma equipe comercial —
diz: *"**NOT A CLAIM — no external pentest has been commissioned.** … Reps must not assert
any pentest result."*

O aviso correto existe, está escrito, e não alcançou o comunicado. É a forma mais nítida
do padrão de [B-101]: falha de propagação, não de conhecimento. Reparo: remover as
afirmações e o marcador `[CEO_NAME]`, propagando o texto que o `PROOF-POINTS.md` já tem.

```backlog
id: B-088
repo: corelink-server
owner: owner
status: open
verify: |
  bash -c 'p=marketing/launch/PRESS-RELEASE.md
  t=reports/pentest-rfp-tracker.json
  [ -f "$p" ] || { echo "FALHA: o press release sumiu — feche o item."; exit 1; }
  afirma=0; grep -qiE "external pentest|pentest.*clean" "$p" && afirma=1
  ceo=0; grep -q "CEO_NAME" "$p" && ceo=1
  contratado=0
  if [ -f "$t" ]; then grep -qE "\"rfp_sent_date\"[[:space:]]*:[[:space:]]*\"" "$t" && contratado=1; fi
  if [ "$afirma" = 0 ] && [ "$ceo" = 0 ]; then
    echo "FALHA: o comunicado nao afirma mais pentest nem carrega CEO_NAME — feche o item."; exit 1; fi
  if [ "$contratado" = 1 ]; then
    echo "FALHA: o tracker registra RFP enviada — reavalie: talvez o pentest exista agora."; exit 1; fi
  echo "aberto: comunicado afirma_pentest=$afirma tem_CEO_NAME=$ceo e o tracker nao registra nenhuma RFP enviada"'
verify-means: |
  open — o comunicado ainda afirma pentest externo (ou ainda carrega o marcador
  `[CEO_NAME]`) E o tracker não registra nenhuma RFP efetivamente enviada.

  Vira DRIFTED por qualquer um dos dois reparos: remover as afirmações do comunicado (o
  imediato), ou de fato contratar o pentest e registrar no tracker (o caro). A segunda
  saída é legítima e por isso o comando a detecta em vez de ignorá-la.

  O que NÃO decide: se o comunicado já foi distribuído a jornalistas ou prospects. Fora
  do repositório, pertence ao owner.
last-verified: 2026-08-30
```

### B-089 — o SLA promete créditos automáticos como remédio exclusivo e não existe código que emita crédito

`legal/sla/v1.0.0.md:74` promete créditos *"issued automatically against the next
invoice"*, e a §4.5 declara isso o *"sole and exclusive remedy"* do cliente.

Busquei `service_credit`, `sla_credit`, `credit_note` e `balance_transaction` em todo
`crates/`, `worker/` e `apps/` — nada, exceto cupons de lançamento da Stripe no checkout.

Agravantes na mesma cláusula: o SLA define quatro tiers enquanto o produto vende seis, de
modo que um cliente Solo ($15) ou Max ($149) não tem tier no instrumento assinado, embora
o `FAQ-MASTER.md:51` lhes prometa 99,5% e 99,9% com créditos. E os Termos de Serviço
(`terms.tsx:333`) concedem créditos automáticos ao tier Pro, que a própria tabela de
preços marca como `slaCredits: false`.

Uma cláusula de remédio exclusivo que não pode ser cumprida é a primeira a cair, e sua
queda expõe danos sem teto.

```backlog
id: B-089
repo: corelink-server
owner: owner
status: open
verify: |
  bash -c 's=legal/sla/v1.0.0.md
  [ -f "$s" ] || { echo "FALHA: o SLA sumiu — reavalie o item."; exit 1; }
  promete=0; grep -qiE "issued automatically|automatic.*credit" "$s" && promete=1
  [ "$promete" = 1 ] || { echo "FALHA: o SLA nao promete mais credito automatico — feche o item."; exit 1; }
  impl=$(grep -rlE "service_credit|sla_credit|credit_note|balance_transaction" crates/ worker/src/ apps/ --include="*.rs" --include="*.ts" --include="*.tsx" 2>/dev/null | grep -v "/target/" | grep -v node_modules | wc -l | tr -d " ")
  [ "$impl" = 0 ] || { echo "FALHA: $impl arquivo(s) implementam emissao de credito — feche o item."; exit 1; }
  echo "aberto: SLA promete credito automatico como remedio exclusivo e ZERO codigo emite credito"'
verify-means: |
  open — o SLA promete crédito automático E nenhum arquivo de código implementa emissão
  de crédito.

  Vira DRIFTED por qualquer um dos dois reparos: implementar a emissão (o que o contrato
  exige), ou emendar a cláusula para um remédio que a organização consiga cumprir (o
  honesto). A escolha é jurídica e comercial.

  O que NÃO decide, e admito: o descompasso de quatro tiers no SLA contra seis vendidos, e
  a contradição do `terms.tsx:333` com `slaCredits: false`. São três documentos que
  precisam concordar entre si, e um `verify` que os comparasse exigiria parsear a tabela
  de preços — vale escrever junto com o reparo, não antes dele. Ficam registrados na prosa.

  Owner: emenda de instrumento assinado.
last-verified: 2026-08-30
```

### B-090 — o worker que processa Stripe, Clerk e DSR implanta a partir de `npm install` sem lockfile, e o portão de supply-chain é cego a ele

O `signup-worker` é o handler vivo de cobrança (`webhooks/stripe.ts`), identidade
(`webhooks/clerk.ts`), provisionamento (`webhooks/github_provision.ts`) e do consumidor de
DSR (`webhooks/dsr_consumer.ts`).

`signup-worker-deploy.yml:65` roda `npm install --legacy-peer-deps`, com o comentário da
linha 61 explicando: *"`npm ci` cannot run — the lockfile is gitignored"*. Confirmado:
`git check-ignore -v apps/signup-worker/package-lock.json` aponta `.gitignore:95`. Sem
lockfile, cada deploy re-resolve a árvore do zero; sem `--ignore-scripts`, todo
`postinstall` transitivo executa. O runner é auto-hospedado — é o Mac do fundador — e
carrega as credenciais de deploy da Cloudflare no ambiente.

**E o portão não vê.** O `.gitignore:93` diz *"npm lockfiles — this repo uses pnpm"*, e o
`pnpm-audit.yml` varre o grafo resolvido pelo `pnpm`. O worker de cobrança sobe com uma
árvore resolvida pelo `npm`: são grafos diferentes, e o que chega à produção é o que o
portão não varre. As outras lanes acertam — `admin-ui-deploy.yml:93` e
`cf-deploy-prod.yml:307` usam `pnpm install --frozen-lockfile`. O `signup-worker` é a
única exceção, e é justamente o que carrega o segredo do webhook da Stripe. O
`pnpm-audit.yml:24` ainda traz `TODO(flip-to-blocking)`.

Reparo: commitar o lockfile do worker como exceção explícita ao `.gitignore:95`, e trocar
por `npm ci --ignore-scripts`.

```backlog
id: B-090
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'w=.github/workflows/signup-worker-deploy.yml
  [ -f "$w" ] || { echo "FALHA: a lane de deploy do signup-worker sumiu — reavalie o item."; exit 1; }
  solto=0; grep -qE "npm install" "$w" && solto=1
  ci=0; grep -qE "npm ci" "$w" && ci=1
  noscripts=0; grep -q "ignore-scripts" "$w" && noscripts=1
  lock=0; [ -f apps/signup-worker/package-lock.json ] && lock=1
  if [ "$solto" = 0 ] && [ "$ci" = 1 ] && [ "$noscripts" = 1 ]; then
    echo "FALHA: a lane usa npm ci com --ignore-scripts — feche o item."; exit 1; fi
  echo "aberto: npm_install_solto=$solto npm_ci=$ci ignore_scripts=$noscripts lockfile_versionado=$lock"'
verify-means: |
  open — a lane de deploy ainda usa `npm install` solto, ou não passa `--ignore-scripts`.

  Vira DRIFTED quando as três condições do reparo valerem juntas: sem `npm install`
  solto, com `npm ci`, com `--ignore-scripts`. O AND é deliberado — pinar sem desligar
  lifecycle scripts, ou desligar scripts sem pinar, deixa metade do buraco aberto, e num
  worker que carrega o segredo do webhook da Stripe metade não serve.

  O `lockfile_versionado` é reportado mas não gateado: `npm ci` já falha sem lockfile, então
  gatear nele seria redundante.

  O que NÃO decide, e admito: se o `pnpm-audit` passa a cobrir o grafo do signup-worker.
  Essa é a metade "o portão é cego" e depende de decisão de tooling (unificar em pnpm, ou
  adicionar uma lane npm-audit para este worker). Se a decisão for a segunda, vale item
  próprio.
last-verified: 2026-08-30
```

### B-091 — a cadeia de proveniência de release é não-funcional e o SBOM publicado tem três meses

O `sbom.yml` falha em 8 de 8 por um defeito de uma linha: `cargo cyclonedx --all` escreve
um arquivo por crate dentro de cada diretório, e o passo seguinte procura um único arquivo
na raiz — `ls: sbom.cdx.json: No such file or directory`.

No mesmo log, os 75 crates emitem *"invalid license expression (UNLICENSED)"*:
`Cargo.toml:406` define `license = "UNLICENSED"`, que é convenção npm e não identificador
SPDX válido. Para software proprietário o correto é `LicenseRef-…`.

O SBOM que existe em `.sbom/cyclonedx-rust.json` foi commitado em 28 de maio e lista 413
componentes; o `Cargo.lock` hoje tem 652. E `sbom.yml:39` declara
`CYCLONEDX_CLI_LINUX_SHA256: "placeholder-pin-at-first-use"`, variável que não é usada em
lugar nenhum e aparenta ser um pin de integridade.

Cross-ref: a impossibilidade do SLSA L3 sem builder hosted é [B-031] (done, decisão
registrada). Este item cobre o SBOM e o `UNLICENSED`, que são consertáveis sem gasto
hosted.

```backlog
id: B-091
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'unl=0; grep -qE "^license[[:space:]]*=[[:space:]]*\"UNLICENSED\"" Cargo.toml && unl=1
  ph=0; grep -q "placeholder-pin-at-first-use" .github/workflows/sbom.yml 2>/dev/null && ph=1
  velho=0
  if [ -f .sbom/cyclonedx-rust.json ]; then
    comp=$(grep -o "\"bom-ref\"" .sbom/cyclonedx-rust.json 2>/dev/null | wc -l | tr -d " ")
    lock=$(grep -c "^name = " Cargo.lock 2>/dev/null | tr -d " ")
    [ "$comp" -gt 0 ] && [ "$lock" -gt 0 ] && [ "$comp" -lt "$((lock * 8 / 10))" ] && velho=1
  fi
  soma=$((unl + ph + velho))
  [ "$soma" -gt 0 ] || { echo "FALHA: license SPDX valida, sem placeholder de pin e SBOM proximo do Cargo.lock — feche o item."; exit 1; }
  echo "aberto: license_UNLICENSED=$unl placeholder_de_pin=$ph sbom_defasado=$velho (componentes=${comp:-na} vs Cargo.lock=${lock:-na})"'
verify-means: |
  open — o `Cargo.toml` ainda declara `UNLICENSED`, ou o `sbom.yml` ainda carrega o
  placeholder de pin, ou o SBOM commitado tem menos de 80% dos pacotes do `Cargo.lock`.

  Vira DRIFTED quando os três forem resolvidos. Usei o limiar de 80% em vez de igualdade
  exata porque contagem de componentes CycloneDX e linhas `name =` do `Cargo.lock` não
  batem uma a uma; o limiar detecta defasagem estrutural (413 vs 652 é 63%) sem reprovar
  por diferença de método de contagem.

  O que NÃO decide, e admito: se o `sbom.yml` volta a passar. A lane é hosted-blocked por
  #1434, então gatear no verde dela acorrentaria este item a uma decisão de gasto do
  owner. O defeito de uma linha no caminho do arquivo é consertável e testável
  independentemente disso, e é o que este item cobra.
last-verified: 2026-08-30
```

### B-092 — a documentação de entrada ensina SHA-256; o CAS é BLAKE3, e o primeiro PUT de todo cliente novo retorna 422

`apps/docs/docs/intro.md` instrui SHA-256 nas linhas 10, 33 e 39. O
`crates/corelink-hash/src/lib.rs:1` abre com *"CoreLink CAS digest types — BLAKE3"*. Um
digest de algoritmo errado produz `422 HashMismatch`.

O texto correto **já existe no próprio repositório**, em `apps/docs/docs/api/http.md:58`:
*"Compute it with `b3sum` — **not** `sha256sum`."* A página de introdução — a primeira que
um cliente lê — nunca recebeu a correção.

É o defeito de menor esforço de reparo e maior consequência imediata da auditoria: quatro
linhas separam todo cliente novo de um erro no primeiro comando.

```backlog
id: B-092
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'i=apps/docs/docs/intro.md
  [ -f "$i" ] || { echo "FALHA: intro.md sumiu — reavalie o item."; exit 1; }
  sha=$(grep -ciE "sha-?256|sha256sum" "$i" | tr -d " ")
  b3=$(grep -ciE "blake3|b3sum" "$i" | tr -d " ")
  [ "$sha" -gt 0 ] || { echo "FALHA: intro.md nao menciona mais SHA-256 — feche o item."; exit 1; }
  [ "$b3" = 0 ] || { echo "FALHA: intro.md ja menciona BLAKE3/b3sum ($b3x) — reavalie: o reparo pode estar feito."; exit 1; }
  echo "aberto: intro.md menciona SHA-256 em $sha lugar(es) e BLAKE3/b3sum em nenhum"'
verify-means: |
  open — a página de introdução ainda instrui SHA-256 e não menciona BLAKE3 nem `b3sum`.

  Vira DRIFTED quando o BLAKE3 aparecer, que é o reparo. Uso a ausência de BLAKE3 em vez
  de exigir zero menções a SHA-256 porque a página pode legitimamente citar SHA-256 ao
  explicar a superfície Bazel (que aceita sha256 sob `bazel/sha256/`); o que não pode é
  ensinar SHA-256 SEM ensinar BLAKE3.

  Cross-ref: o texto correto já existe em `apps/docs/docs/api/http.md:58` e é de onde a
  correção deve ser propagada, não reescrita.
last-verified: 2026-08-30
```

### B-093 — quatro tetos diferentes para o tamanho de uma entrada de cache, e o do caminho de escrita nativo é o menor

`main.rs:504` aplica `DefaultBodyLimit::max(10 MiB)` sobre o router inteiro. O Turbo se
isenta com override próprio (`turbo_v8.rs:1080`, `TURBO_BODY_LIMIT_BYTES = 100 MiB`); o CAS
nativo e as quatro rotas de escrita do Bazel não têm override — apenas comentários *sobre*
o limite global. Os quatro tetos:

| Superfície | Teto | Origem |
|---|---|---|
| CAS nativo + Bazel (escrita) | 10 MiB | `main.rs:504`, sem override |
| Leitura do CAS (por objeto) | 64 MiB | `CAS_READ_MAX_OBJECT_BYTES` |
| Turborepo | 100 MiB | `TURBO_BODY_LIMIT_BYTES` |
| Bridge Bazel do cliente | 4 GiB | `MAX_BLOB_SIZE_BYTES` |

O cliente Bazel valida um digest declarado de até 4 GiB e recebe 413 aos 10 MiB. O maior
objeto existente hoje em produção tem **52,3 MB** (enumeração completa do
`corelink-cas-prod` em 2026-08-26, registrada em `cas.rs:146`) — cinco vezes o teto de
escrita atual.

**Não é achado novo.** Está em `docs/security/2026-06-15-launch-due-diligence-audit.md`:
*"[MEDIUM] Bazel CAS write inherits the global 10 MiB body limit (no per-route override) —
Bazel build outputs >10 MiB cannot be cached."* Aberto há dois meses e meio. É o exemplar
concreto de [B-101].

O dano não é build quebrado — o Bazel reporta falha de upload como aviso e conclui. O dano
é o cache silenciosamente não cachear exatamente os artefatos que valem a pena: os grandes.

```backlog
id: B-093
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'm=crates/corelink-container/src/main.rs
  [ -f "$m" ] || { echo "FALHA: main.rs sumiu — reavalie o item."; exit 1; }
  glob=0; grep -qE "GLOBAL_BODY_LIMIT_BYTES.*10[[:space:]]*\*[[:space:]]*1024[[:space:]]*\*[[:space:]]*1024" "$m" && glob=1
  [ "$glob" = 1 ] || { echo "FALHA: o limite global de 10 MiB mudou — reavalie o item."; exit 1; }
  ovcas=0; grep -q "DefaultBodyLimit" crates/corelink-container/src/routes/cas.rs 2>/dev/null && \
    grep -qE "layer\(.*DefaultBodyLimit" crates/corelink-container/src/routes/cas.rs 2>/dev/null && ovcas=1
  ovbz=0; grep -qE "layer\(.*DefaultBodyLimit" crates/corelink-container/src/routes/bazel_v2.rs 2>/dev/null && ovbz=1
  if [ "$ovcas" = 1 ] && [ "$ovbz" = 1 ]; then
    echo "FALHA: CAS e Bazel ja tem override proprio de body limit — feche o item."; exit 1; fi
  echo "aberto: limite global 10 MiB ativo; override no cas.rs=$ovcas no bazel_v2.rs=$ovbz"'
verify-means: |
  open — o limite global de 10 MiB continua aplicado E pelo menos uma das duas superfícies
  de escrita (CAS nativo, Bazel) não tem override próprio.

  Vira DRIFTED quando as duas tiverem override, que é o reparo — subir o teto exige
  orçamento de memória, e o precedente no repositório é o guard de concorrência do Turbo.
  Cross-ref [B-077] e [B-056]: o teto novo tem que caber em 1 GiB.

  O que NÃO decide, e admito: se os quatro tetos passam a ser COERENTES entre si. Detecta
  a ausência de override, não o alinhamento com o `MAX_BLOB_SIZE_BYTES` de 4 GiB do bridge
  do cliente. Alinhar os quatro é o reparo completo; este comando cobra o primeiro passo.
last-verified: 2026-08-30
```

### B-094 — o material publicado promete gRPC e compatibilidade com Buck2; o código registra que nenhum dos dois existe

A superfície Bazel é REAPI v2 **sobre REST**. O `crates/corelink-container/src/routes/bazel_v2.rs:331`
anota: *"(Buck2 cannot use these routes — it speaks REAPI over gRPC only.)"* Não há
servidor gRPC no produto.

O material comercial afirma o contrário. Isto compõe com a nota já registrada no
repositório sobre hostnames publicados que não resolvem.

Reparo: alinhar o material ao que existe (REAPI v2 sobre REST, sem Buck2), ou registrar o
gRPC como roadmap explícito em vez de capacidade presente.

```backlog
id: B-094
repo: corelink-server
owner: owner
status: open
verify: |
  bash -c 'b=crates/corelink-container/src/routes/bazel_v2.rs
  [ -f "$b" ] || { echo "FALHA: bazel_v2.rs sumiu — reavalie o item."; exit 1; }
  admite=0; grep -qi "Buck2 cannot use these routes" "$b" && admite=1
  [ "$admite" = 1 ] || { echo "FALHA: o codigo nao admite mais que Buck2 nao conecta — reavalie: o gRPC pode existir agora."; exit 1; }
  promete=$(grep -rliE "buck2|gRPC" marketing/ 2>/dev/null | wc -l | tr -d " ")
  [ "$promete" -gt 0 ] || { echo "FALHA: o material comercial nao promete mais gRPC/Buck2 — feche o item."; exit 1; }
  echo "aberto: bazel_v2.rs admite que Buck2 nao conecta e $promete arquivo(s) de marketing prometem gRPC/Buck2"'
verify-means: |
  open — não há implementação gRPC no workspace E o material comercial ainda menciona
  gRPC ou Buck2.

  Vira DRIFTED por qualquer um dos dois reparos: corrigir o material (imediato), ou
  implementar um servidor gRPC (caro, e é decisão de produto) — neste segundo caso a
  linha de admissão do `bazel_v2.rs` sai junto e o comando falha por ali.

  NÃO tento detectar "existe gRPC no workspace" automaticamente, e a razão é medida: as
  duas tentativas anteriores falsearam. Grepar `tonic` casa os comentários que dizem que
  gRPC NÃO está lá (`INV-BAZEL-NO-GRPC`), e `tonic` é dependência real do
  `corelink-container` e do `corelink-reapi`; grepar `Server::builder()` casa o
  middleware de timing-padding do axum. Nenhum predicado barato separa a máquina de gRPC
  presente no workspace de um servidor gRPC servido ao cliente. Gateio no que é estável e
  autoritativo: a admissão no código e a promessa no material.

  O que NÃO decide, e admito: se a menção em `marketing/` é uma PROMESSA de capacidade
  presente ou uma nota de roadmap corretamente rotulada. Gatear por presença de string dá
  falso positivo se alguém escrever "gRPC está no roadmap para 2027". Quem fechar deve ler
  as ocorrências e julgar; se sobrarem menções legítimas de roadmap, feche com nota em vez
  de reescrever o comando para ignorá-las.

  Owner: material comercial.
last-verified: 2026-08-30
```

### B-095 — três defeitos funcionais na interface do cliente, dos quais o mais grave mente sobre residência

**Pin de workspace.** `apps/admin-ui/src/components/customer/WorkspacesClient.tsx:161`
diz ao cliente que Pin mantém o conteúdo *"exempt from eviction"* e é *"billed as a metered
add-on ($5/mo per 100 GB pinned)"*. O handler
(`crates/corelink-container/src/routes/workspaces.rs:286`) executa
`UPDATE workspaces SET pinned = 1 - pinned` e nada mais; o cabeçalho da própria rota
(:394) admite *"`pinned` = false until snapshot sizing/pinning land"*; e não existe medidor
de $5/100 GB em código, SQL ou Stripe. Somando com [B-071], não há eviction da qual isentar.

**Convite de time.** `TeamClient.tsx:36` oferece quatro papéis
`["Owner","Admin","Developer","Viewer"]`; a migração `0074_team_member.sql:41` aceita
`('owner','admin','member','viewer')`. O `customer_d1.rs:376` mapeia `"owner" | "admin" =>
"admin"` e tudo mais para `member`, enquanto a resposta ecoa de volta o papel pedido. Dois
dos quatro papéis oferecidos não existem, e o usuário vê confirmado o papel errado.

**Flag `--region` do instalador.** `apps/get-corelink-worker/src/install.ts:191` — *"`--region`
remains accepted on the command line as a no-op until a real config key exists for it"*. Um
cliente com requisito de residência passa `--region=weur`, não recebe erro, e acredita ter
fixado a região.

O último é o mais sério: residência é a peça central de compliance do produto ([B-085],
[B-086]), e a interface primária para escolhê-la mente silenciosamente.

```backlog
id: B-095
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'n=0; det=""
  i=apps/get-corelink-worker/src/install.ts
  if [ -f "$i" ] && grep -qi "no-op" "$i" && grep -q "region" "$i"; then n=$((n+1)); det="$det region-noop"; fi
  t=apps/admin-ui/src/components/customer/TeamClient.tsx
  if [ -f "$t" ] && grep -q "Developer" "$t"; then
    m=migrations/d1/0074_team_member.sql
    if [ -f "$m" ] && ! grep -qi "developer" "$m"; then n=$((n+1)); det="$det papel-developer-inexistente"; fi
  fi
  w=crates/corelink-container/src/routes/workspaces.rs
  if [ -f "$w" ] && grep -q "pinned = 1 - pinned" "$w"; then n=$((n+1)); det="$det pin-e-so-toggle"; fi
  [ "$n" -gt 0 ] || { echo "FALHA: os tres defeitos de interface sumiram — feche o item."; exit 1; }
  echo "aberto: $n de 3 defeitos de interface persistem:$det"'
verify-means: |
  open — pelo menos um dos três defeitos persiste. Cada um é detectado pelo seu próprio
  predicado: `--region` ainda documentado como no-op; papel `Developer` oferecido pela
  interface e ausente do CHECK da migração; e o pin ainda sendo puro toggle.

  Conta 3/2/1 em vez de exigir zero: consertar um reduz o número e o item permanece aberto
  listando os que faltam. Progresso parcial aparece, e os três são independentes.

  Vira DRIFTED quando os três forem resolvidos. Se a decisão for remover a promessa em vez
  de implementá-la (remover `--region`, remover `Developer` da interface, remover o texto
  de $5/100 GB), o comando também fecha — desfechos legítimos, porque a alegação é a
  DIVERGÊNCIA entre o que a interface promete e o que o sistema faz.
last-verified: 2026-08-30
```

### B-096 — o moat de efeito de rede existe, mas cobre seis imagens curadas, não o cache que é vendido

Substitui um candidato refutado: o namespace `_public` é real e está armado em produção —
`OCI_PUBLIC_DEDUP_ENABLED = "1"` nos cinco blocos, ao lado de `OCI_UPSTREAM_ON_MISS = "1"`.

O que é verdade é mais estreito. A dedup entre tenants cobre **apenas a superfície OCI**, e
apenas digests de uma allowlist assada no binário
(`crates/corelink-container/src/public_base_allowlist.manifest`): seis pins curados —
alpine, debian 12, ubuntu 24.04, node 22-slim, python 3.12-slim, mais o layer alpine
original. Todo o resto — CAS nativo, Bazel, Turborepo, sccache, npm, pip, brew — é chaveado
por `blob_key(region, tenant_prefix, digest, algo)` com
`tenant_prefix = derive_prefix(secret_tdk, tenant_uuid)`, HMAC por tenant. Bytes idênticos
em dois tenants ocupam duas chaves; o mesmo blob em cinco regiões ocupa cinco. É deliberado
(camada 5 de `INV-TENANT-ISOLATION`) e correto do ponto de vista de isolamento.

A tese comercial registrada no `CLAUDE.md` e no brief de expansão — *"mais clientes → cache
mais cheio → mais rápido e mais barato para todos"* — descreve um efeito que hoje opera
sobre seis imagens base. No caminho realmente vendido, o custo de armazenamento cresce com
tenants × regiões, sem amortização.

Não é defeito de código. É afirmação de estratégia que a arquitetura ainda não sustenta, e
a diferença deveria estar escrita onde a tese está.

```backlog
id: B-096
repo: corelink-server
owner: owner
status: open
verify: |
  bash -c 'm=crates/corelink-container/src/public_base_allowlist.manifest
  hmac=0
  grep -rq "tenant_prefix" crates/corelink-container/src/storage/r2_s3.rs 2>/dev/null && hmac=1
  [ "$hmac" = 1 ] || { echo "FALHA: a chave do CAS nao usa mais tenant_prefix — reavalie o item."; exit 1; }
  pins=0
  [ -f "$m" ] && pins=$(grep -cE "^[[:space:]]*sha256:" "$m" 2>/dev/null | tr -d " ")
  tese=0
  grep -qiE "network-effect|efeito de rede|moat" CLAUDE.md 2>/dev/null && tese=1
  ressalva=0
  grep -qiE "(apenas|somente|only|restrito|limited).{0,40}(imagens base|OCI base|_public)" CLAUDE.md 2>/dev/null && ressalva=1
  if [ "$tese" = 0 ] || [ "$ressalva" = 1 ]; then
    echo "FALHA: a tese saiu do CLAUDE.md ou ja traz a ressalva de escopo — feche o item."; exit 1; fi
  echo "aberto: CAS chaveado por HMAC per-tenant, allowlist com $pins pin(s), e a tese do moat no CLAUDE.md sem ressalva de escopo"'
verify-means: |
  open — o CAS continua chaveado por HMAC por tenant (logo sem dedup cross-tenant fora do
  `_public`) E o `CLAUDE.md` afirma o efeito de rede sem registrar que ele hoje cobre só a
  allowlist OCI.

  Vira DRIFTED por qualquer um dos dois reparos: escrever a ressalva de escopo onde a tese
  está (barato e honesto), ou ampliar a dedup para além da allowlist (decisão de
  arquitetura que tensiona com `INV-TENANT-ISOLATION`).

  O que NÃO decide, e admito: se a redação da ressalva é ADEQUADA. Detecta a presença de
  palavras de escopo, não a qualidade da qualificação. Quem fechar deve ler.

  Owner: é afirmação de estratégia, não de engenharia.
last-verified: 2026-08-30
```

### B-097 — teto de escala em 200 tenants ativos por região, com o orçamento de vCPU da conta já comprometido

A arquitetura é um contêiner `basic` (0,25 vCPU) por tenant ativo. O comentário do
`wrangler.toml:503` documenta o teto com honestidade exemplar: *"The BINDING account limit
is vCPU, NOT instance count […] against total_vcpu=1500 […] A first attempt at 2000
(=500 vCPU/region) was REJECTED by CF ("Surpassed total account limits: ... vcpus"). Going
higher — toward real 2000-user scale — requires a Cloudflare account-limit increase […];
no config can exceed it."*

Contabilidade atual: 5 regiões × 200 × 0,25 = 250 vCPU para o cache, mais 250 × 4 = 1000
vCPU para a frota de runners. São **1250 de 1500 já alocados**. Ocioso escala a zero, então
o teto é de tenants *concorrentemente* ativos, não de clientes totais.

Não é defeito. É dependência de plataforma no caminho crítico do crescimento, já testada e
recusada uma vez. Existe como item porque o tempo de resposta de um aumento de limite da
Cloudflare não é controlado por nós, e descobrir isso quando o teto for atingido é tarde.

```backlog
id: B-097
repo: corelink-server
owner: owner
status: open
verify: manual
verify-means: |
  MANUAL, e declaro que o `verify` não decide a alegação.

  A alegação tem duas metades e nenhuma é legível do repositório: o limite de conta vigente
  na Cloudflare (`total_vcpu`, hoje 1500) e o número de tenants concorrentemente ativos em
  produção. A primeira só a API/o painel da Cloudflare respondem; a segunda exige medir
  produção.

  Um `verify` que somasse `max_instances × 0.25` do `wrangler.toml` mediria a RESERVA
  declarada, não o teto nem o consumo — passaria verde com o teto atingido e passaria
  vermelho se alguém baixasse o `max_instances` por outro motivo. Portão dominado.

  Procedimento de reverificação: confirmar `total_vcpu` da conta no painel; somar as
  reservas declaradas (cache + frota de runners); e medir tenants ativos concorrentes em
  produção. Fecha quando o aumento de limite for concedido, ou quando a arquitetura deixar
  de ser um contêiner por tenant ativo.

  Owner: pedir aumento de limite à Cloudflare é relação comercial com fornecedor.
last-verified: 2026-08-30
```

### B-098 — dezoito worktrees vivem num diretório que o sistema operacional apaga, e uma delas tem trabalho não enviado

Dezoito worktrees estão sob `/private/tmp`, sujeitas à limpeza periódica do macOS. O
trabalho do PR #1439 está fisicamente num desses diretórios. Não toquei, porque é trabalho
em andamento de outra sessão — mas precisa sair de lá.

No mesmo eixo, medido em 2026-08-30: 154 branches locais (129 não mergeadas, 24 mergeadas e
não apagadas), 87 remotas, e a seção `[Unreleased]` do CHANGELOG com 9.911 linhas sem que
uma release jamais tenha sido cortada — zero tags semver, versão do workspace em `0.1.0`.
Isto explica parte de [B-091]: a lane `cosign-sign` dispara em tag `v*`, e nunca houve uma.

Itens menores que o mandato de impecabilidade cobre: o `CLAUDE.md` diz "~73 crates" (são
75), "160 conceitos OKF" (são 165) e "485 specs" (são 486); a conta `gmhelmold` tem token
inválido no keyring e está marcada como ativa no `gh`; e `corelink-runbook-tracker` é o
único crate cujos lints copiados divergiram do workspace — faltam `print_stdout` e
`print_stderr`.

```backlog
id: B-098
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'tags=$(git tag -l "v*" 2>/dev/null | wc -l | tr -d " ")
  lints=0
  f=crates/corelink-runbook-tracker/Cargo.toml
  if [ -f "$f" ] && grep -q "workspace.lints" -r crates/corelink-runbook-tracker 2>/dev/null; then lints=0; else
    if [ -f "$f" ] && grep -q "\[lints" "$f" && ! grep -q "print_stdout" "$f"; then lints=1; fi
  fi
  crates_reais=$(ls -d crates/*/ 2>/dev/null | wc -l | tr -d " ")
  crates_doc=$(grep -oE "~?[0-9]+ Rust crates" CLAUDE.md 2>/dev/null | grep -oE "[0-9]+" | head -1)
  drift=0; [ -n "$crates_doc" ] && [ "$crates_doc" != "$crates_reais" ] && drift=1
  soma=$((lints + drift))
  if [ "$tags" -gt 0 ] && [ "$soma" = 0 ]; then
    echo "FALHA: existe tag semver, lints alinhados e CLAUDE.md com contagem correta — feche o item."; exit 1; fi
  echo "aberto: tags_semver=$tags lints_divergentes=$lints claude_md_desatualizado=$drift (doc=${crates_doc:-na} real=$crates_reais)"'
verify-means: |
  open — não existe nenhuma tag semver, OU os lints do `corelink-runbook-tracker` divergem,
  OU o `CLAUDE.md` declara um número de crates que não bate com a contagem real.

  Deliberadamente NÃO gateia as worktrees em `/private/tmp` nem a contagem de branches: são
  estado da máquina do desenvolvedor, não do repositório, e um `verify` que os medisse
  falharia ou passaria conforme QUEM roda o CI — portão que depende do ambiente é ruído.
  Ficam na prosa como instantâneo datado e como instrução operacional.

  O que este comando decide é a parte que vive no repositório e é verificável em qualquer
  clone. A parte das worktrees exige ação humana na máquina e está registrada acima.
last-verified: 2026-08-30
```

### B-099 — o `CODEOWNERS` atribui revisão a dez times que o próprio arquivo admite não existirem

O `.github/CODEOWNERS` nomeia dez handles distintos — `@HumanGuardrail/` seguido de
`appsec`, `architects`, `docops`, `engineering`, `finance`, `founders`, `legal`,
`marketing`, `privacy`, `sre-leads`. O cabeçalho do arquivo, linha 8, admite: *"Placeholder
team handles (HumanGuardrail/*) — replace with concrete reviewer names per team once the
GitHub org has the teams provisioned."* E a regra catch-all é rotulada *"Default owner —
catch-all so nothing merges unreviewed."*

Uma lane de diligência reportou `gh api orgs/HumanGuardrail/teams` retornando lista vazia;
**não reconfirmei de forma independente** porque a cota da API do GitHub estourou durante a
auditoria — registro como pendente de reverificação em vez de afirmar. A evidência local,
porém, basta para o item: o arquivo diz que os handles aguardam provisionamento.

Combinado com `required checks = []` na proteção de branch (registrado no próprio
`CLAUDE.md`), a frase *"catch-all so nothing merges unreviewed"* descreve intenção, não
controle. Importa porque [B-087] atesta "Y" em CCC-07.1 citando *"branch protection;
CODEOWNERS"* como evidência.

```backlog
id: B-099
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'c=.github/CODEOWNERS
  [ -f "$c" ] || { echo "FALHA: CODEOWNERS sumiu — reavalie o item."; exit 1; }
  admite=0; grep -qi "placeholder team handles" "$c" && admite=1
  times=$(grep -oE "@[A-Za-z0-9-]+/[A-Za-z0-9-]+" "$c" | sort -u | wc -l | tr -d " ")
  [ "$admite" = 1 ] || { echo "FALHA: o arquivo nao admite mais handles placeholder — reavalie: os times podem existir agora."; exit 1; }
  [ "$times" -gt 0 ] || { echo "FALHA: nao ha mais handles de time no CODEOWNERS — feche o item."; exit 1; }
  echo "aberto: CODEOWNERS admite handles placeholder e atribui revisao a $times time(s)"'
verify-means: |
  open — o `CODEOWNERS` ainda carrega a admissão de que os handles são placeholder E ainda
  atribui revisão a handles de time.

  Vira DRIFTED por qualquer um dos dois reparos: provisionar os times na org (e remover a
  nota de placeholder), ou substituir os handles por nomes concretos de revisores, que é o
  que a própria nota instrui.

  O que NÃO decide, e admito explicitamente: se os times EXISTEM na organização do GitHub.
  Isso exige `gh api orgs/HumanGuardrail/teams`, que não roda no `backlog_verify` e cuja
  cota estourou durante a auditoria. O comando decide a admissão registrada no arquivo, que
  é evidência local suficiente para manter o item aberto, e não finge decidir o resto.

  Reverificação pendente: rodar a chamada de API quando a cota permitir e anotar aqui.
last-verified: 2026-08-30
```

### B-100 — os avisos de privacidade publicados, nos quatro idiomas, dirigem titulares a endereços de domínio reservado

`apps/admin-ui/src/content/privacy-notice.{en,pt,es,de}` instruem contato em
`privacy@corelink.example` e `dpo@corelink.example`. O TLD `.example` é reservado pela
RFC 2606 e não entrega correio.

Um titular exercendo direito do Artigo 15 ou 17, ou um regulador em contato inicial,
escreve para o vazio. É reparo de substituição de string em quatro arquivos, e é visível a
quem menos deveria vê-lo.

```backlog
id: B-100
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'n=$(grep -rlE "[a-z]+@corelink\.example" apps/ --include="*.ts" --include="*.tsx" --include="*.md" --include="*.mdx" 2>/dev/null | grep -v node_modules | wc -l | tr -d " ")
  [ "$n" -gt 0 ] || { echo "FALHA: nenhum endereco @corelink.example resta em apps/ — feche o item."; exit 1; }
  echo "aberto: $n arquivo(s) publicados ainda dirigem titulares a @corelink.example (TLD reservado, RFC 2606)"'
verify-means: |
  open — ainda existe pelo menos um arquivo sob `apps/` com endereço `@corelink.example`.

  Vira DRIFTED quando o último sair, que é o reparo. Conta arquivos em vez de exigir os
  quatro idiomas de uma vez: corrigir um reduz o número e o item permanece aberto — mas
  corrigir só um idioma seria pior que não corrigir nenhum, porque cria divergência entre
  as versões publicadas. Quem consertar deve fazer os quatro no mesmo commit.

  Escopo deliberado em `apps/`: o endereço pode legitimamente aparecer em exemplos de
  documentação interna ou em fixtures de teste, e reprovar por isso seria ruído. O que não
  pode é estar no texto publicado ao titular.
last-verified: 2026-08-30
```

### B-101 — auditorias anteriores levantaram 89 achados que nunca entraram no backlog, e o portão não pode enxergá-los

O `BACKLOG.md` é a fonte declarada de verdade, cada item carrega um `verify`, e o
`backlog_verify.py` falha em DRIFTED ou STALE. É um bom mecanismo. O problema é o que fica
fora dele.

Medido em 2026-08-30 — achados contados no documento, depois grepado o nome do arquivo no
`BACKLOG.md`:

| Documento | Achados | Refs no BACKLOG |
|---|---|---|
| `docs/security/2026-06-15-launch-due-diligence-audit.md` | 66 | 0 |
| `reports/audits/2026-08-26-go-live-readiness.md` | 20 | 0 |
| `docs/security/2026-07-02-pilot-identity-brutal-audit.md` | 3 | 0 |

Verifiquei um deles até o fim: o teto de 10 MiB na escrita do Bazel ([B-093]), classificado
MEDIUM em 15 de junho, continua verdadeiro dois meses e meio depois — sem item, sem prazo,
sem dono.

O portão está verde e continuará verde, porque só verifica itens que **estão** no backlog.
Um achado nunca transcrito é invisível para ele por construção. Não é defeito do portão — é
o limite dele, e explica por que tantos achados de qualquer auditoria nova já estavam
escritos em algum lugar do repositório.

**A regra que falta:** nenhuma auditoria fecha sem que cada achado vire item do backlog ou
seja explicitamente recusado com motivo registrado. Os itens B-063 … B-102 são a aplicação
dessa regra à auditoria de 2026-08-30 — quarenta achados, quarenta decisões.

```backlog
id: B-101
repo: corelink-server
owner: tl
status: open
verify: manual
verify-means: |
  MANUAL, e declaro explicitamente que **nenhum comando automático decide esta alegação**
  — a regra deste arquivo exige admitir isso em vez de fabricar um portão conveniente.

  A primeira versão que escrevi grepava o nome dos três documentos no `BACKLOG.md` e
  contava os que não apareciam. Ela reprovou no primeiro `backlog_verify`, e reprovou pelo
  motivo mais instrutivo possível: **os próprios itens B-062 … B-101 citam esses arquivos
  em prosa**, então o grep passou a encontrá-los e o item se declarou resolvido sem que
  nenhum dos 89 achados tivesse virado item. Um `verify` que conta a própria menção é
  exatamente o portão dominado que este item existe para denunciar.

  A alegação real é de COBERTURA: os 66 achados do documento de 2026-06-15, os 20 do de
  2026-08-26 e os 3 do de 2026-07-02 estão rastreados como itens? Decidir isso exige
  parsear os achados de cada documento e casá-los um a um com itens do backlog. Esse
  casador não existe, e escrevê-lo é trabalho de verdade — provavelmente o reparo certo
  para este item, e nesse dia este `verify: manual` deve ser substituído por ele.

  Enquanto não existir, `manual` honesto é melhor que automático que decide outra coisa.

  Procedimento de reverificação: abrir cada documento, listar seus achados, e conferir
  quantos têm item correspondente. Fecha quando os três estiverem transcritos ou
  explicitamente recusados. Decai em 14 dias como todo item manual — e é correto que decaia,
  porque a resposta muda a cada auditoria nova que ninguém transcreve.

  Nota: este item se aplica a si mesmo. A auditoria de 2026-08-30 está em B-062 … B-101
  justamente para não virar a quarta linha daquela tabela.
last-verified: 2026-08-30
```

### B-102 — um PUT quente no `/cargo` custa 1,38s contra um alvo de 30-50 ms, e o armazenamento real é só metade disso

Medido contra produção em 2026-08-30, tenant de dogfood `ee30f7ba-…`, PAT `cas:rw`.

**⚠️ Correção de causa registrada, não apagada.** A primeira versão deste item atribuía o
custo a uma assimetria entre caminho de leitura e de escrita: o `Server-Timing` mostrava
`auth desc="d1"` 711ms no PUT contra `desc="kv"` 8ms no GET, e a leitura natural foi "a
escrita não usa o cache L2 de PAT". **Essa causa está refutada.** O `desc` é a CAMADA DE
CACHE que serviu a linha, não o caminho de código: `worker/src/lib/pat_verify_cache.ts:146`
declara `source: "l1" | "kv" | "d1"`, e `worker/src/index.ts:1550` diz textualmente
*"Observability only — which tier served the row."* Existe **um único** call site de
`verifyPatRowCached` (`index.ts:1481`); GET e PUT compartilham o roteamento
(`index.ts:955`); e a cascata de camadas não olha o método HTTP. A medição original foi
**PUT frio seguido de GET quente**, e a razão de 89x era miss-contra-hit da mesma função.

O mesmo confounder atingia `qtier` e `qresid`: os três caches de quota
(`tenant_tier_cache`, `quota_storage_cache`, `tenant_residency_cache`) têm arquitetura
idêntica L1-memória + L2-KV e nenhum deles referencia `method`.

**Perfil quente, três PUTs seguidos com o mesmo token dentro de 60s:**

```
PUT #1  auth 10ms desc="kv"   qtier 4  qbatch 131  qresid 3   →  1,53s
PUT #2  auth  0ms desc="l1"   qtier 0  qbatch 122  qresid 0   →  1,39s
PUT #3  auth  6ms desc="kv"   qtier 2  qbatch 116  qresid 2   →  1,38s
```

Auth caiu de 711ms para **0-10ms**; `qtier` e `qresid` de 433/332ms para **0-4ms**. A
alegação de custo fixo por requisição **sobrevive** — 1,38s quente, contra o alvo do owner
de 30-50 ms, é **28-46x fora** — mas a causa é outra.

**Decomposição quente, e é aqui que o trabalho mora:**

```
PUT quente = 1,38s
  origin  1007ms
    ostore   647ms   → [B-107]  armazenamento real
    ohop     153ms
    oother   139ms   → [B-109]  trabalho sem fase nomeada
    opat      72ms
  wdb      278ms
    qbatch   116ms   → [B-108]  única quota que não cacheia
```

De 1,38s, o armazenamento real é 647ms: **mais de metade do tempo não é armazenar**. Este
item é o guarda-chuva; os três alvos têm item próprio, e o custo de verify frio que a
refutação revelou é [B-106].

**Proveniência.** Medições do Mac do owner, não de dentro da frota. O trajeto Mac→borda
custa 0,08s (401 rejeitado na borda: 0.084/0.090/0.081s; `/_health`: 0.078/0.085/0.079s),
ou ~6% do total — objeção testada, não descartada. E o pin era `4f9313e0`, 43 commits
atrás; remedir após o repin (#1445).

```backlog
id: B-102
repo: corelink-server
owner: tl
status: open
verify: manual
verify-means: |
  MANUAL — a alegação é latência de produção sob credencial, e o gate roda sem credencial
  de plano de dados. Um `verify` que cronometrasse a borda sem autenticar mediria 0,08s e
  passaria verde para sempre, medindo o trajeto e não o caminho de escrita.

  Procedimento: PAT `cas:rw` no tenant de dogfood; **três PUTs de 1 KiB em sequência,
  dentro de 60s**, e ler o `Server-Timing` do terceiro. A sequência é obrigatória, não
  cosmética: foi exatamente a medição a frio que produziu a causa errada da primeira
  versão deste item. Um PUT isolado mede cache frio e mente sobre o estado permanente.

  Fecha quando o PUT quente cair para a ordem de 30-50 ms. Enquanto estiver em segundos,
  os alvos são [B-107], [B-108] e [B-109], e o de maior alcance é [B-106].

  **Caminho para automatizar:** `cargo-cache-latency-probe.yml` já é `workflow_dispatch`
  com acesso ao `CORELINK_SCCACHE_TOKEN` e hoje só faz GETs. Estendê-lo com três PUTs
  cronometrados e um teto sobre o terceiro transforma isto em portão de verdade, medido de
  dentro da frota. Quem fizer deve substituir este `manual`.
last-verified: 2026-08-30
```

### B-103 — o caminho de escrita do `/cargo` falha em 87% sob paralelismo e satura em ~2 req/s

Dois fatos que precisam ser explicados juntos. Em série, cinco PUTs, cinco 200, zero
falhas. Na CI, com o sccache escrevendo em paralelo (run 33326194312, `tenant-path`,
PR #1443): **191 erros de escrita em 220 misses**, com `read errors = 0`. Mesmo token,
mesmo host, mesmo TLS — isso exclui rede genérica e credencial.

**Medição de concorrência, 2026-08-30, contra produção:**

```
conc=4    4/4   ok    wall  5,4s  →  0,74 req/s
conc=16   16/16 ok    wall 10,2s  →  1,57 req/s
conc=64   60/64 ok    wall 33,4s  →  1,92 req/s   (4 × HTTP 429)
```

**As falhas são 429, não 5xx** — o caminho de escrita não quebra; o nosso próprio
limitador recusa. Mata "escrita quebrada" de vez.

**O número não fecha com a configuração.** `crates/corelink-ratelimit/src/tier.rs` declara
`TEAM_REFILL_RPS = 200` e `TEAM_BURST = 1000`; 64 requisições não deveriam encostar em
nada. Ou o tenant de dogfood não resolve para o tier Team, ou o balde não é por tenant como
se presume. Ver [B-080], que documenta fallbacks de tier em ambas as direções — inclusive
rótulo desconhecido caindo em `Tier::Team`.

**O achado maior é a vazão.** Concorrência multiplicada por 16 e a vazão subiu 2,6x;
satura em **~2 req/s**, que é **1% dos 200 rps autorizados**. Mais nítido no degrau barato:
em série a vazão é `1/1,41 = 0,71 req/s` e com concorrência 4 é 0,74 — **ganho
praticamente zero desde o primeiro degrau**, assinatura de serialização quase total ANTES
do limitador. Um limitador recusa rápido com 429; ele não enfileira.

Cadeia que os dois conjuntos sustentam: serialização ⇒ ~2 req/s ⇒ as 220 escritas paralelas
do sccache levam ~110s ⇒ timeout do cliente estoura ⇒ os 191 erros.

**⚠️ Duas causas candidatas foram REFUTADAS e ficam registradas.** *Quota estourada:*
`cargo.rs:133` chama `resolve_storage_cap` em todo PUT e um teto atingido explicaria
read=0/write=191 — refutada pelos PUTs em série, que deram 200 no mesmo tenant. *Consultas
de quota síncronas no caminho quente:* os `qtier`/`qresid` de centenas de ms eram **cache
frio**, e caem para 0-4ms quentes (ver a correção registrada em [B-102]). Sobram como
candidatos vivos: o `qbatch` de 116ms que **não** cacheia ([B-108]), o `ostore` de 647ms
([B-107]), e a corrida no auto-seed da linha de quota (`cargo.rs:104`), não testada.

**Nenhuma causa nomeada, de propósito.** O que serializa continua não identificado, e a
primeira tentativa de nomear já errou uma vez.

```backlog
id: B-103
repo: corelink-server
owner: tl
status: open
verify: manual
verify-means: |
  MANUAL — exige credencial de produção, e admito uma segunda razão mais séria: **a causa
  ainda não está nomeada**, e um `verify` escrito para a causa errada passa verde com o
  defeito vivo. Já erramos a causa uma vez neste mesmo item.

  A etapa de concorrência já foi executada e está registrada acima: o joelho fica em
  ~2 req/s e as falhas são 429. Falta a etapa que decide o que serializa — ler o log do
  servidor durante a rajada, **SEM filtro**. `wrangler tail --search` retorna zero com a
  linha presente, então filtrar aqui esconde exatamente a evidência.

  Ordem correta de ataque: fechar [B-107] e [B-108] primeiro. Se a serialização for o
  `ostore` ou o `qbatch` disputando a mesma linha de tenant, ela desaparece junto e este
  item fecha sem conserto próprio. Só se sobreviver aos dois é que merece investigação
  independente.

  ⚠️ **A medição é do pin `4f9313e0`, não da `main`** — produção estava 43 commits atrás
  (ver [B-110]). O fix do mapa de tombstone (#1431) NÃO estava em produção; repin em #1445.
  Remedir após o roll antes de fixar teto ou nomear causa.

  Fecha quando N PUTs concorrentes (N na ordem dos 220 do sccache) tiverem taxa de falha
  zero e a vazão escalar com a concorrência.
last-verified: 2026-08-30
```

### B-104 — o 404 autenticado tem mediana de 0,32s e cauda de 2,47s; a mediana é o resíduo que o #1033 nunca explicou

**⚠️ Correção de magnitude registrada, não apagada.** A primeira versão deste item afirmava
`GET → 404 em 3.318s` como o custo do caminho de miss, a partir de **uma** amostra. Dez
amostras desmentem o número:

```
mediana 0,32s   ·   p90 0,48s   ·   max 2,47s
```

Os 3,3s eram **cauda**, não típico. O item foi registrado desde o início como uma amostra
e com a instrução explícita de medir dez antes de fixar qualquer teto — e foi essa
ressalva que impediu um teto errado de entrar no backlog. Fica aqui como precedente:
fixar limiar a partir de uma amostra é escolher o número que confirma a suspeita.

**O que sobra, e é real em duas frentes.**

A **mediana de 0,32s** casa quase exatamente com o resíduo de ~300ms que o #1033 registrou
num 404 autenticado puro, dizendo explicitamente que **não era Argon2id** (o #1027 mediu, o
#1022 memoizou) e que **não estava medido**. Ou seja: o resíduo não piorou dez vezes, mas
**continua lá e continua sem causa nomeada**, agora confirmado com dez amostras em vez de
uma. Um 404 não grava bytes, não toca contabilidade, não semeia linha de quota — 320ms para
concluir que algo não existe é trabalho não enumerado, o mesmo padrão de [B-109].

A **cauda de 2,47s contra p90 de 0,48s** é a segunda frente: um fator de 5 entre p90 e
máximo, num caminho que deveria ser o mais barato da superfície. Cauda dessa largura é
sintoma, não ruído.

```backlog
id: B-104
repo: corelink-server
owner: tl
status: open
verify: manual
verify-means: |
  MANUAL — exige credencial de produção, como [B-102] e [B-103].

  Procedimento: dez ou mais GETs autenticados em caminhos inexistentes, cronometrados,
  reportando **mediana e p90** — nunca máximo isolado. O teto certo sai do p90; foi o
  máximo que produziu a primeira versão errada deste item.

  Fecha quando a mediana cair para a ordem de dezenas de ms, que é o que um caminho que
  apenas conclui inexistência deveria custar. NÃO fecha por a cauda melhorar sozinha: os
  0,32s medianos são o resíduo do #1033 e são a alegação principal.

  Se a causa dos 320ms medianos for identificada e for a mesma de [B-109] (`oother`, fase
  não nomeada), este item deve ser fechado apontando para lá em vez de receber conserto
  próprio — é provável, e enumerar antes de otimizar decide isso.
last-verified: 2026-08-30
```

### B-105 — o cache de build custa mais do que economiza, e a causa NÃO é o caminho de escrita

`docs/internal/secrets-checklist.md`, linha 188, registrado em 2026-08-03 sobre a
superfície `/cargo` com o token de dogfood:

> engaged runs recorded 189 hits / 638 misses and then **827 hits / 0 misses (100 % hit
> rate) with 0 cache/read/write errors** […] even at a 100 % hit rate the lane ran **631 s
> against a 409-423 s no-cache baseline**, so the cache cost more than it saved.

É item de **produto**, não de CI: o CoreLink é vendido como cache de build e, no caso
perfeito — hit total, erro zero — perdeu para compilar frio por ~210s.

**A aritmética, e por que ela desmente a explicação óbvia.** Dos números da própria CI:

| Operação | Custo |
|---|---|
| Compilar o artefato (`Average compiler`) | 0,678 s |
| Ler do cache num hit (`Average cache read hit`) | 0,527 s |
| Escrever no cache num miss (`Average cache write`) | 1,346 s |

Um hit economiza `0,678 − 0,527 = **0,151 s**`. Um miss custa `+1,346 s`. A taxa de acerto
de equilíbrio é `1,346 / (1,346 + 0,151) ≈ **90%**` — só para empatar.

Agora o ponto que a revisão adversarial encontrou e que **derruba a explicação natural**:
na corrida de agosto foram **827 hits e ZERO misses**. Zero misses significa **zero
escritas**. Um custo de escrita de 1,4s, por maior que seja, não pode explicar uma perda de
210s numa corrida onde nada foi escrito. **[B-102] não é a causa deste item.** O modelo
acima previa uma ECONOMIA de `827 × 0,151 ≈ 125 s`; a realidade foi uma perda de ~210s.
Sobram **~335s de custo que nenhuma das três médias explica.**

E o número mais desconfortável do conjunto está na tabela e não depende nem de escrita nem
de concorrência: **ler do cache custa 0,527s contra 0,678s para simplesmente compilar** —
o cache aparentaria ser apenas **22% mais barato que fazer o trabalho**.

**⚠️ Essa razão de 22% NÃO é confiável, e o viés corre a favor do cache.** Verificado no log
bruto do run 33326194312, não na citação:

```
Compile requests            337
Compile requests executed   259
Cache hits                   35     Cache misses          220
Non-cacheable calls          74     Forced recaches         0
Cache read errors             0     Cache write errors    191
Average cache write       1.346 s
Average compiler          0.678 s
Average cache read hit    0.527 s
```

As duas médias percorrem **populações disjuntas e de tamanhos muito diferentes**: uma
unidade de compilação ou acerta o cache ou é compilada, nunca as duas. `Average cache read
hit` é a média sobre **35** unidades; `Average compiler` é a média sobre as **~220-259** que
executaram. Comparar as duas pressupõe que os dois conjuntos têm a mesma distribuição de
custo — e não há razão para terem. Numa lane de PR, o que acerta são dependências estáveis
inalteradas; o que erra é o crate em trabalho e o que depende dele.

E há um segundo viés, específico deste run: **os 35 acertos aconteceram contra um cache com
191 falhas de escrita**. O que está no cache é, por construção, o subconjunto cuja escrita
teve sucesso. Se a falha de escrita correlaciona com tamanho — plausível e não medido — o
cache contém preferencialmente objetos **pequenos**, os acertos são preferencialmente
unidades **pequenas**, e 0,527 s para buscar uma unidade pequena está sendo comparado com
0,678 s para compilar uma unidade média. **A razão real por unidade seria PIOR que 22%, não
melhor.**

O que sobrevive à objeção: a corrida de agosto, com 827 acertos, 0 erros e 100% de hit,
ainda perdeu ~210 s para compilar frio. Essa é a evidência sólida do item, porque não
depende de comparar médias de populações diferentes — compara a **duração da mesma lane**
com e sem cache. A razão 0,527/0,678 deve ser tratada como indício, não como medida, até
alguém comparar as MESMAS unidades nos dois regimes.

```backlog
id: B-105
repo: corelink-server
owner: owner
status: open
verify: manual
verify-means: |
  MANUAL, e é o item onde um `verify` automático seria mais perigoso: a alegação é uma
  comparação entre a duração de uma lane COM e SEM cache, e nenhuma das duas está no
  repositório — vivem no histórico de execuções do GitHub.

  Procedimento: rodar a mesma lane com `CORELINK_SCCACHE_PILOT` ligado e desligado, na
  mesma máquina, e comparar a duração total. Fecha quando a corrida com cache for
  consistentemente MAIS RÁPIDA que a sem, por margem que sobreviva à variância do runner.

  **Não fecha por conserto de [B-102] nem de [B-103].** Está registrado acima por que: a
  corrida de agosto teve zero escritas e ainda assim perdeu 210s. Levar a escrita a custo
  zero deixa este item intacto.

  **E não fecha pela razão 0,527/0,678 melhorar**, porque essa razão compara médias sobre
  populações disjuntas de tamanhos muito diferentes (35 acertos contra ~220 compilações) e
  está enviesada a favor do cache. A medida que decide é a duração da MESMA lane com e sem
  cache — que é o que a corrida de agosto já fez e o que qualquer fechamento tem de refazer.

  **Piso irredutível de uma leitura: hoje NÃO é calculável, e a razão é instrumental.** Na
  decomposição medida de um GET (total 1001 ms): `auth` 8, `wdb` 391 dos quais só 7 são
  nomeados (`qtier` 3 + `qresid` 4), e `origin` 602 dos quais 372 são nomeados (`ostore`
  371 + `oother` 1). Sobram **384 ms no resíduo do `wdb`** e **230 ms no `ohop`** — ou seja,
  **61% da leitura está em dois resíduos**, e o `qother` que nomearia o primeiro é publicado
  só atrás de flag. Não dá para dizer se o piso é rede, servidor ou cliente enquanto a maior
  parcela do caminho não tem nome.

  O que JÁ dá para excluir: **não é rede.** O trajeto até a borda medido do pior ponto de
  vista disponível (o Mac do owner, fora da frota) é 0,08 s — 8% de um GET de 1001 ms, e da
  frota seria menos. A rede não é o termo dominante, então o piso mora no servidor ou no
  cliente, e distinguir os dois exige nomear o `qother`.

  Owner, não tl: a decisão que este item alimenta é se o produto vendido como cache de
  build entrega aceleração no caso perfeito. É pergunta de produto.
last-verified: 2026-08-30
```

### B-106 — um verify de PAT frio custa 711 ms e o TTL do KV é 60 s, então todo cliente paga isso continuamente

Achado que só apareceu porque a causa do [B-102] foi refutada: ao provar que o `desc` era
camada de cache e não caminho de código, o número que sobrou deixou de ser um artefato de
medição e passou a ser o defeito.

Um verify servido pelo D1 custa **711 ms**; servido pelo KV, **8 ms**; pela L1 de memória,
**0 ms**. Medido em produção, 2026-08-30.

O alcance é o ponto. `worker/src/lib/pat_verify_cache.ts` documenta as camadas: L1 é
per-isolate com TTL de ~5 s, e o L2 KV tem TTL de **60 s** — o comentário na linha 92 diz
que 60 s é o **piso do KV**, não uma escolha. Então a cada 60 segundos, para cada token, em
cada colo, o primeiro request paga 711 ms. Isso não é caso excepcional: é o estado
permanente de qualquer tráfego que não seja uma rajada contínua sobre o mesmo token no
mesmo colo.

Um cliente que faça uma build por minuto paga 711 ms em **toda** build. Um cliente com
tráfego distribuído por colos paga por colo. Um cliente novo paga sempre.

Atinge **todo cliente autenticado, em toda superfície** — não só o `/cargo`, não só o CI.
É o item de maior raio do pacote de performance, e o único que não é específico da escrita.

O comentário em `index.ts:1460` já registra a origem: a leitura vai por
`withSession("first-unconstrained")` para a réplica mais próxima com fallback ao primário,
*"~tens of ms globally"* — mas o número medido é 711 ms, uma ordem de grandeza acima do que
o próprio comentário prevê. Ou a sessão de réplica não está sendo usada em produção, ou a
réplica não existe na região que serviu, ou o fallback ao primário está sendo tomado sempre.
**Três hipóteses, nenhuma medida.**

```backlog
id: B-106
repo: corelink-server
owner: tl
status: open
verify: manual
verify-means: |
  MANUAL — exige credencial de produção e, pior, exige medir um estado FRIO, que é
  destrutivo de si mesmo: a primeira medição aquece o cache e a segunda mede outra coisa.

  Procedimento: um request autenticado com um token que não seja usado há mais de 60 s (ou
  recém-mintado), lendo o `Server-Timing`. Confirmar `auth;desc="d1"` e cronometrar. Repetir
  em intervalos maiores que 60 s para amostrar — nunca em sequência, que é exatamente o erro
  que produziu a causa errada do [B-102].

  Fecha quando um verify frio cair para a ordem de dezenas de ms, que é o que o comentário
  do `index.ts:1460` já promete (*"~tens of ms globally"*) e que a medição desmente.

  As três hipóteses a separar, antes de qualquer conserto: (a) a sessão de réplica não está
  ativa em produção; (b) não há réplica na região que serviu; (c) o fallback ao primário é
  tomado sempre. São consertos diferentes e só uma medição as distingue — não escolha por
  intuição, que já custou uma causa errada neste pacote.

  NÃO fechar aumentando o TTL do KV. 60 s é o piso do KV, e alongar a janela de cache
  alarga a janela de revogação — a ADR-0030 fixa 60 s de p99 para revogação, e trocar
  latência por janela de revogação é trocar performance por furo de segurança.
last-verified: 2026-08-30
```

### B-107 — `ostore` custa 654 ms para gravar 1 KiB, e NAO e otimizavel como uma coisa so

No perfil quente do PUT (ver [B-102]), com todo cache aquecido e a autenticacao em 0-10 ms,
o `ostore` — nomeado "armazenamento" — e **654 ms de mediana** (min 568, max 714, n=10) para
um objeto de **1 KiB**. O alvo declarado pelo owner e 30-50 ms: **13 a 21 vezes** fora.

A linha de base e confiavel, e isso foi testado em vez de suposto. A suspeita de que
estivesse inflada por dupla contagem foi levantada e **refutada**: `MoatCache::put` envolve a
escrita em `timed(Phase::Store)` e o `R2CasHandler::write` reentra na mesma fase, mas o
`spawn_blocking` do `put_untimed` troca de task e o ledger e task-local, entao o escopo
interno nao registra. Medido: `ostore` 33 537 us contra 33 639 us de relogio de parede do
`put` inteiro (`adapter_cache::tests::put_records_the_store_phase_exactly_once`).

**Correcao de FORMA, e e o que muda este item.** A versao anterior pressupunha que os 654 ms
fossem um alvo unico a reduzir. **Nao sao.** O `ostore` agrega, sob um so nome, o PUT no R2
**e** ate cinco idas sequenciais ao D1: as ate quatro consultas de `check_and_accrue`
(`byte_accounting.rs:306-486`) mais a do `map.put` (`adapter_cache.rs:393`). O
`byte_accounting.rs` nao abre fase nenhuma — **zero** referencias a `origin_timing` — entao
todo esse D1 cai dentro do `ostore`.

E a agregacao **nao e descuido**: e a forma do decorator. O accounting **envolve** a escrita
em vez de ficar ao lado dela (accrue -> write interno -> commit/release, interleaved dentro
do `handler.write`). Separar D1-de-contabilidade de R2 exige **mover onde as fases abrem**,
nao acrescentar um nome — ver [B-122], que e o bloqueio.

Duas tentativas de contornar foram medidas e descartadas ANTES de virar codigo. Levar o
handle do ledger no `CasWriteRequest` inverteria uma dependencia de crate (o tipo vive no
`corelink-handler-cas`, o `PhaseLedger` no `corelink-container`, e a seta aponta
container->handler). E abrir um `oaccrue` sob o `Store` externo quebraria a particao: o
`ostore` ja cobre a janela do accrue, entao a fase nova cobriria a mesma janela outra vez e
`Sigma(fases) > total` — sem saida pela borda, porque o Worker deriva o `ohop` por subtracao
das fases nomeadas e nao existe categoria "informativa que se sobrepoe".

**Enquanto [B-122] nao fechar, este item nao tem alvo mensuravel.** "Otimizar o
`ostore`" mira duas coisas ao mesmo tempo por construcao, e qualquer melhora medida seria
inatribuivel entre R2 e contabilidade.

**A cauda entra no item.** Total: mediana 1,409 s, media 1,907, desvio 0,942, max 4,064
(n=10) — cauda de 3x sobre a mediana. Otimizar a mediana e deixar a cauda em 4 s entrega um
produto que parece rapido e trava de vez em quando: a versao de performance do sucesso
silencioso.

```backlog
id: B-107
repo: corelink-server
owner: tl
status: open
verify: manual
verify-means: |
  MANUAL — a alegacao e latencia de producao sob credencial, e o gate roda sem credencial de
  plano de dados.

  Procedimento: PAT `cas:rw` no tenant de dogfood; TRES PUTs de 1 KiB em sequencia dentro de
  60 s (a sequencia e obrigatoria — foi a medicao a frio que produziu a causa errada da
  primeira versao de [B-102]); ler `ostore` do terceiro. Registrar a VERSAO de producao
  medida junto do numero: prod fica atras da `main`, e comparar numero novo com codigo
  diferente nao atribui causa.

  **NAO fecha por o `ostore` cair.** Enquanto [B-122] nao separar D1-de-contabilidade
  de R2, uma queda nao diz qual dos dois melhorou, e um item que aceita melhora
  inatribuivel aceita coincidencia como prova. Fecha quando (1) as duas partes forem
  mensuraveis separadamente E (2) a soma delas cair para a ordem de 30-50 ms.

  A metade da cauda tem criterio proprio: p90 e p99 medidos, nao so mediana. Um p99 de 4 s
  com mediana de 1,4 s continua sendo defeito depois de a mediana melhorar.
last-verified: 2026-08-30
```

### B-108 — `qbatch` custa 116-131 ms em todo PUT e é a única quota que o cache não serve

No perfil quente, os três irmãos de quota zeram: `qtier` cai de 433 ms para 0-4 ms e
`qresid` de 332 ms para 0-4 ms, servidos pelas camadas L1/KV. O `qbatch` **não cai**:
mantém-se em 116-131 ms nos três PUTs consecutivos.

É a única contabilidade que executa de verdade em toda escrita.

**Isto pode ser correto, e o item registra a pergunta antes de registrar o conserto.** Um
contador de lote que precisa ser fresco não pode ser cacheado sem furar a cobrança, e
quota que não é cobrada é receita perdida. Se for esse o caso, o desfecho certo é **fechar
como aceito, com o motivo registrado** — não otimizar.

O que decide: descobrir se o `qbatch` é (a) um contador que exige leitura fresca por
correção de cobrança, ou (b) o mesmo padrão dos irmãos sem o cache que eles ganharam. Em
(a) o item vira recusa registrada; em (b) vira conserto de uma linha.

Nota de composição: 116 ms em toda escrita, disputando a mesma linha de tenant, é um
mecanismo concreto de serialização — e serialização é exatamente o que [B-103] procura sem
ter nomeado. Se o `qbatch` for a causa, os dois fecham juntos.

```backlog
id: B-108
repo: corelink-server
owner: tl
status: open
verify: manual
verify-means: |
  MANUAL, e por uma razão que não é a credencial: **este item pode não ter conserto.** A
  primeira etapa é uma decisão de correção de cobrança, não uma medição de latência, e um
  `verify` que exigisse `qbatch` baixo prejulgaria essa decisão — passaria a exigir a
  remoção de uma checagem que pode ser obrigatória.

  Etapa 1, e é leitura de código, não medição: determinar se o `qbatch` exige leitura
  fresca por correção de cobrança. Se exigir, **fechar como aceito com o motivo registrado**
  e a nota de que 116 ms é o preço da cobrança correta.

  Etapa 2, só se a etapa 1 disser que não exige: cachear como os irmãos e provar pelo
  `Server-Timing` de três PUTs em sequência.

  **Nenhum conserto pode remover a checagem.** O alvo é fazê-la uma vez, em lote, ou fora do
  caminho quente. Trocar latência por furo de cobrança não é otimização.
last-verified: 2026-08-30
```

### B-109 — `oother` custa 139 ms de trabalho de contêiner sem fase nomeada

No perfil quente do PUT, `oother` é 139 ms; no GET, 1 ms. Sobreviveu à correção de cache
frio que derrubou as causas de `auth`, `qtier` e `qresid`, então não é artefato de ordem de
medição.

**`oother` é subtração, não medição.** É o resíduo de `origin` menos as fases nomeadas —
esta casa já registrou que uma fase fora da allowlist de nomes vira "outros", e portanto
trabalho de contêiner real aparece como categoria vazia. Um número que só existe por
diferença não diz o que está fazendo.

Por isso o primeiro passo é **enumerar, não otimizar**. Instrumentar as sub-fases que hoje
caem no resíduo e descobrir o que são. Otimizar uma categoria de subtração é escolher um
alvo sem saber onde ele está.

Composição provável: a mediana de 320 ms de um 404 autenticado ([B-104]) também é trabalho
não enumerado num caminho que deveria ser barato. Pode ser o mesmo. Enumerar decide.

```backlog
id: B-109
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'f=crates/corelink-container/src/routes/cas.rs
  hint=$(grep -rln "oother\|OOTHER" crates/corelink-container/src --include="*.rs" 2>/dev/null | head -1)
  [ -n "$hint" ] || { echo "FALHA: nao encontro a emissao de oother — reavalie o item."; exit 1; }
  fases=$(grep -rhoE "\"o[a-z]+\"" crates/corelink-container/src --include="*.rs" 2>/dev/null | sort -u | wc -l | tr -d " ")
  [ "$fases" -gt 0 ] || { echo "FALHA: nenhuma fase nomeada encontrada — reavalie."; exit 1; }
  echo "aberto: $fases fase(s) de origin nomeadas; oother continua sendo o residuo por subtracao"'
verify-means: |
  open — existe emissão de `oother` no código, ou seja, ainda há uma categoria de resíduo
  por subtração no `Server-Timing` de `origin`.

  Admito o que este comando NÃO decide: ele conta fases nomeadas e confirma que o resíduo
  existe; **não mede quanto tempo cai nele**. Esse número só sai do `Server-Timing` de um
  PUT autenticado contra produção, e o gate não tem credencial.

  É deliberado que este seja o único dos itens de performance com `verify` automático: a
  alegação aqui não é "139 ms é muito", é "**existe trabalho de contêiner que nenhuma fase
  nomeia**". Essa parte é estrutural, vive no repositório, e é decidível sem credencial.

  Vira DRIFTED quando o resíduo deixar de existir — isto é, quando as sub-fases forem
  enumeradas e o `oother` sumir ou virar constante desprezível. Que é exatamente a definição
  de pronto: enumerar, não otimizar.
last-verified: 2026-08-30
```

### B-114 — a imagem `corelink-runner-devenv` não existe e nenhum workflow a constrói

Descoberto em 2026-08-30 ao destravar o deploy de produção, e a cadeia importa mais que o
sintoma:

```
deploy de prod  →  BLOQUEADO: RunnerDevEnvDO não exportado pelo spawn-worker
  deploy do spawn-worker  →  BLOQUEADO: "Latest tags are not allowed"
    corelink-runner-devenv:latest  →  A IMAGEM NUNCA FOI CONSTRUÍDA
      grep -rln "runner-devenv" .github/workflows/  →  VAZIO
```

O `:latest` é o sintoma — a Cloudflare recusa tags móveis. A causa é que **nada produz a
imagem**. Os dois contêineres irmãos da frota são pinados por digest; só o DevEnv não é,
porque não há digest a pinar. O `Dockerfile.runner-devenv` existe no repo da frota e nenhum
workflow o constrói.

**O custo não foi a feature incompleta, foi o bloqueio colateral.** Enquanto durou, nenhum
deploy de produção passava, com 43 commits presos — entre eles o #1410, que conserta a
tabela fantasma que parte o apagamento do **GDPR Art. 17** no meio. Uma feature de
desenvolvimento incompleta segurou uma correção de conformidade.

Destravado em #1447 desacoplando o binding, deliberadamente **sem** construir a imagem às
pressas: ligar feature nova pelo caminho apressado só para desbloquear é exatamente como o
defeito chegou aqui. Este item cobre a dívida que sobrou.

```backlog
id: B-114
repo: corelink-runners
owner: tl
status: open
verify: manual
verify-means: |
  MANUAL — a alegação é sobre o repositório `corelink-runners`, e o `backlog_verify.py`
  roda no `corelink-server`. Um `verify` automático aqui grepearia a árvore errada e
  passaria verde para sempre, medindo a ausência do arquivo no repo onde ele nunca esteve.

  Procedimento, no clone do `corelink-runners`: `grep -rln "runner-devenv"
  .github/workflows/` — hoje retorna **vazio**, e é essa ausência que é a alegação.
  Confirmar também que `Dockerfile.runner-devenv` existe (o defeito é o descasamento entre
  Dockerfile presente e workflow ausente, não a falta do Dockerfile).

  Fecha por qualquer um dos dois desfechos legítimos: existe workflow que constrói e
  publica a imagem por digest, como os dois irmãos da frota; **ou** a feature DevEnv é
  removida e o `Dockerfile.runner-devenv` sai junto. O segundo é decisão de produto e vale
  como recusa registrada.

  NÃO fecha por alguém construir a imagem à mão e pinar o digest. Imagem sem workflow que a
  reproduza é a mesma dívida com outra roupa — o próximo deploy volta a depender de um
  artefato que ninguém sabe reconstruir.
last-verified: 2026-08-30
```

### B-115 — nenhum portão testa se um PR deixa a produção deployável

Este é o item de maior valor dos dois, porque é uma **classe** de defeito sem portão, não
um defeito.

O #1432 mergeou verde e tornou a produção não-deployável. Nada no CI mediu isso, porque
nenhum portão pergunta *"depois deste merge, um deploy ainda passa?"*. A verificação existe
— é o próprio `cf-deploy-prod` — mas roda **depois** do merge e só quando alguém deploya.

O intervalo entre as duas coisas é o defeito. O deploy seguinte pode ser dias depois, e
quando falhar vai carregar junto todos os commits que entraram no meio: foi exatamente o
que aconteceu, com 43 commits presos atrás de um binding quebrado, incluindo uma correção
de GDPR ([B-114]).

Confirmado em 2026-08-30: `grep -rn "wrangler deploy" .github/workflows/ | grep -i dry`
retorna **vazio**. Nenhuma lane faz um deploy de ensaio. O `cf-deploy-prod` tem portões de
secrets e de drift, mas todos pressupõem que a configuração resolve — nenhum verifica que
ela resolve.

O caso é agravado por dependência entre repositórios: o binding quebrado apontava para um
Durable Object exportado por **outro** worker, em **outro** repo. Um portão que valide só
esta árvore não teria pego. O que pegaria é um `wrangler deploy --dry-run` contra a
configuração real de produção, que resolve bindings de verdade.

```backlog
id: B-115
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'n=$(grep -rn "wrangler deploy" .github/workflows/ 2>/dev/null | grep -ci "dry-run" | tr -d " ")
  [ "$n" = 0 ] || { echo "FALHA: $n lane(s) ja fazem deploy de ensaio — feche o item."; exit 1; }
  echo "aberto: nenhuma lane executa wrangler deploy --dry-run; um PR pode ficar verde e tornar a producao nao-deployavel"'
verify-means: |
  open — nenhum workflow executa `wrangler deploy --dry-run`.

  Vira DRIFTED quando alguma lane passar a fazer o ensaio, que é o reparo. Escolhi o
  predicado mais estreito e mais honesto que existe: mede a **presença do ensaio**, não a
  ausência de defeitos de deployabilidade — essa segunda coisa nenhum grep decide.

  O que este comando NÃO decide, e admito: se o ensaio, uma vez existindo, **resolve
  bindings entre repositórios**. O binding que causou [B-114] apontava para um Durable
  Object exportado por outro worker, em outro repo; um dry-run que só valide a sintaxe
  desta árvore ficaria verde e o item fecharia sem entregar a proteção. Quem fechar deve
  provar com o caso concreto: reintroduzir o binding quebrado numa branch descartável e
  confirmar que a lane REPROVA. Portão que nunca foi visto falhando não é portão.

  Nota de escopo: o reparo natural é uma lane `pull_request` com `wrangler deploy
  --dry-run` contra a config de produção. Ela não precisa de credencial de deploy — o
  dry-run resolve e não publica — o que a torna barata e compatível com o mandato de zero
  gasto hosted, rodando na frota `corelink`.
last-verified: 2026-08-30
```

---

### B-116 — a doc publica um endpoint de aceite de DPA que não existe

O `apps/docs` documenta `POST /v1/dpa/accept`, com página própria, entrada no índice da
API, exemplos em quatro linguagens e uma tradução pt-BR. **O código não registra essa
rota.** O que existe é `POST /v1/onboarding/dpa-accept`
(`crates/corelink-container/src/routes/dpa_accept.rs:634`).

Não é sinônimo, é outro caminho, e a divergência tem duas camadas:

1. **O caminho.** Não há reescrita no Worker. `/v1/dpa/accept` não casa com nenhum arm
   dedicado do `matchRoute`, então cai no balde genérico `/v1/*`, é encaminhado ao
   contêiner, e o contêiner não tem a rota — 404 depois de autenticar.
2. **O mecanismo de autenticação.** A doc apresenta o endpoint como o resto da API de PAT.
   A rota real vive sob `/v1/onboarding/`, que o Worker trata num arm próprio,
   **autenticado por Clerk** e com tenant `_anonymous`
   (`worker/src/index.ts:1000-1001`). Um cliente que siga a doc erra o caminho **e** a
   credencial.

Gravidade: o aceite de DPA é o registro de consentimento click-through. Um cliente que
tente registrá-lo pela via publicada não consegue, e o produto fica vendendo um controle
contratual cuja porta documentada não abre.

**Como foi provado, e por que não por HTTP.** Sondar prod não decide: os dois caminhos
devolvem `401`, porque o Worker rejeita sem credencial **antes** de rotear. O instrumento
correto é a tabela de rotas do Worker mais o registro do contêiner — sonda autenticada não
vê o que o cliente vê, e sonda não autenticada não vê o roteamento.

```backlog
id: B-116
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'docs=$(grep -rl -- "/v1/dpa/accept" apps/docs/ 2>/dev/null | wc -l | tr -d " ")
  code=$(grep -rn -- "\"/v1/dpa/accept\"" crates/ worker/src/ 2>/dev/null | wc -l | tr -d " ")
  [ "$docs" -gt 0 ] || { echo "FALHA: a doc nao menciona mais /v1/dpa/accept — ou foi corrigida (feche o item) ou o grep quebrou; conte o sinal antes de fechar."; exit 1; }
  [ "$code" = 0 ] || { echo "FALHA: /v1/dpa/accept agora esta registrado no codigo — a divergencia acabou, feche o item."; exit 1; }
  echo "aberto: $docs arquivo(s) de doc publicam /v1/dpa/accept e 0 sitio de codigo registra essa rota"'
verify-means: |
  open — a doc publica o caminho e o código não o registra.

  Os dois lados são medidos, de propósito. Um predicado que só olhasse a doc fecharia
  sozinho se alguém apagasse a página sem criar a rota, e um que só olhasse o código
  fecharia se a rota nascesse com a doc ainda errada. A divergência é uma relação entre
  duas coisas e só uma medição das duas a decide.

  A primeira falha é deliberadamente ruidosa: `docs=0` pode significar "corrigido" ou
  "meu grep quebrou", e essas duas leituras não podem compartilhar um caminho silencioso.

  O que este comando NÃO decide: se o reparo escolhido é o certo. Há dois — publicar o
  caminho real (`/v1/onboarding/dpa-accept`, dizendo que é autenticado por Clerk) ou
  registrar um alias no caminho publicado. O primeiro é honesto; o segundo preserva
  qualquer integração que já tenha sido escrita contra a doc. Quem fechar escolhe e
  justifica, e em ambos os casos a página tem de dizer a credencial certa — corrigir o
  caminho e deixar o mecanismo de auth errado troca um 404 por um 401.
last-verified: 2026-08-30
```

---

### B-117 — apagar e exportar a própria conta não têm documento nenhum

O contêiner registra `POST /v1/customer/account/delete` e
`GET /v1/customer/account/export`. **Nenhuma das duas aparece em lugar algum do
`apps/docs`** — nem página de referência, nem índice, nem OpenAPI, nem tutorial, nem a
tradução pt-BR.

São as duas rotas que materializam apagamento e portabilidade para o titular. O produto
vende conformidade com esses direitos, o binário os implementa, e o cliente não tem como
descobrir que existem. É a forma inversa do [B-116]: lá a doc promete o que o código não
faz; aqui o código faz o que a doc não conta.

Contexto que evita conclusão apressada: as rotas `/v1/privacy/dsr/*` **são** documentadas,
com página por direito. Então não é o tema que falta — é este par específico. Vale
verificar, ao fechar, se as duas são redundantes com o portal DSR (e então o reparo é
apagá-las ou apontá-las) ou se são a via de autosserviço do titular (e então o reparo é
documentá-las).

```backlog
id: B-117
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'n=0
  for r in /v1/customer/account/delete /v1/customer/account/export; do
    c=$(grep -rn -- "\"$r\"" crates/ 2>/dev/null | wc -l | tr -d " ")
    d=$(grep -rl -- "$r" apps/docs/ 2>/dev/null | wc -l | tr -d " ")
    [ "$c" -gt 0 ] || { echo "FALHA: $r nao esta mais registrada no codigo — o item pressupoe que ela existe; reavalie em vez de fechar."; exit 1; }
    [ "$d" = 0 ] && n=$((n+1))
  done
  [ "$n" -gt 0 ] || { echo "FALHA: as duas rotas agora aparecem na doc — feche o item."; exit 1; }
  echo "aberto: $n de 2 rotas de conta registradas no codigo sem nenhuma mencao em apps/docs"'
verify-means: |
  open — pelo menos uma das duas rotas existe no binário e não existe na doc.

  O comando exige que a rota **esteja registrada** antes de reclamar da ausência de doc.
  Sem isso o item fecharia sozinho no dia em que alguém apagasse a rota — e "sumiu" não é
  "resolvido". Um item que pode fechar pelo desaparecimento do seu próprio objeto não
  mede nada.

  O que este comando NÃO decide: se a documentação que aparecer é **correta**. Ele conta
  menção, não qualidade. Quem fechar tem de operar as duas rotas como cliente, com
  credencial de cliente, e confirmar que a página descreve o que elas de fato fazem —
  inclusive o que é irreversível.
last-verified: 2026-08-30
```

---

### B-118 — a única lane hosted protegida por waiver nunca executou — RESOLVIDO, lane apagada

**Fechado 2026-08-31 (WP-E), saída = APAGAR.** O item pedia que quem fechasse
dissesse **por que** a lane nunca rodou, "com o corpo do workflow na mão".
Aqui está: não foi um motivo, foram **cinco**, e só o primeiro era o gatilho.

`cosign-sign.yml` assinava a imagem OCI do Worker com Cosign keyless e publicava
no Rekor. Era a **única** lane que o owner autorizou manter hospedada na GitHub,
com waiver escrito no cabeçalho (2026-08-11). O argumento de custo do waiver
estava certo e sua razão técnica também — Cosign keyless precisa de `docker` e da
identidade OIDC hosted, e a frota Firecracker não tem daemon docker. Ele apenas
protegia uma lane que **não podia funcionar**.

```
gh run list --workflow=cosign-sign.yml --limit 5   →  zero linhas
gh run list --workflow=nightly.yml     --limit 2   →  2 linhas    (controle)
```

A segunda consulta prova o mecanismo: a mesma chamada devolve linhas quando há
linhas, então o vazio da primeira era ausência real, não consulta quebrada.

**As cinco paredes, todas lidas do arquivo:**

1. **Gatilho faminto.** Disparava em push de tag `v[0-9]+.[0-9]+.[0-9]+`, e não
   existe nenhuma tag `v*` no repo — só `cli-v*`. Medido:
   `git ls-remote --tags origin` devolve **0** para `refs/tags/v[0-9]` e **4**
   para `refs/tags/cli-v`. Esta era a única parede que a análise anterior tinha
   visto, e por isso o item foi classificado como "não é defeito".
2. **Empurrão impossível.** `IMAGE_NAME: HumanGuardrail/corelink-worker`, com
   `password: ${{ secrets.GITHUB_TOKEN }}` de um repo em **HuGR-Labs**. O
   `GITHUB_TOKEN` só tem `packages: write` no namespace do próprio repo. O
   arquivo trazia um `TODO(org-rename): HumanGuardrail→HuGR-Labs pending`
   admitindo a pendência.
3. **Sujeito errado.** `context: .` constrói o `Dockerfile` da raiz, que é o
   **contêiner**, e rotulava o resultado `corelink-worker`. O Worker é um
   Cloudflare Worker; não existe imagem OCI dele para assinar.
4. **Placeholder no caminho de deploy.** O job `trigger-cf-deploy` montava o
   payload com `zone_id: "ZONE_ID_PLACEHOLDER"` literal, e o postava em
   `CF_DEPLOY_VERIFIER_URL`.
5. **Atestação que era um `echo`.** O passo "Attest SLSA provenance" está
   declarado no próprio nome como *stub* e só imprimia texto — nenhuma atestação
   era produzida.

Some-se que o caminho de deploy real deste repo é
`container-build-push-prod → PR de repin → cf-deploy-prod`, então a lane também
**duplicava e contradizia** o mecanismo que de fato entrega produção.

**A promessa ao cliente saiu junto, como manda a regra** — lane apagada leva
consigo toda promessa que sustentava. Nove arquivos da doc pública foram
corrigidos no mesmo PR: `trust/subprocessors.mdx` (que nomeava o arquivo do
workflow), `trust/compliance.mdx` (×2), `trust/iso27001.mdx`, `trust/index.mdx`,
`explanation/security/index.mdx` (×2, incluindo a linha da tabela de criptografia
que anunciava "Ed25519 via Sigstore/cosign | offline HSM-backed key" — não existe
chave), `trust/fedramp-info.mdx`, `pricing/comparison.mdx`,
`explanation/compliance/sbom-access.mdx` (que ensinava um `cosign verify-blob`
contra uma chave `$TBD` que nunca foi publicada) e `explanation/privacy/gdpr.mdx`
(Sigstore listado como destinatário de transferência internacional).

**O que este fechamento NÃO decide.** Se CoreLink *deve* assinar releases — deve,
e a ausência agora está dita em voz alta na doc em vez de prometida. Refazer isso
é trabalho novo, com um desenho que funcione (registro no namespace correto, um
sujeito que exista, e um runner com docker), e não a ressurreição deste arquivo.
Rastreado como parte do [B-091], a cadeia de proveniência de release.

```backlog
id: B-118
repo: corelink-server
owner: tl
status: done
verify: |
  bash -c 'test -e .github/workflows/cosign-sign.yml && { echo "FALHA: cosign-sign.yml voltou a existir — o item foi fechado APAGANDO a lane; se ela foi recriada, refaça a analise das cinco paredes antes de reabrir."; exit 1; }
  ctl=$(ls apps/docs/docs/trust/*.mdx 2>/dev/null | wc -l | tr -d " ")
  [ "${ctl:-0}" -gt 0 ] || { echo "INDETERMINADO: nenhum arquivo em apps/docs/docs/trust/ — o instrumento falhou, nao a arvore."; exit 0; }
  claim=$(grep -rlE "Ed25519 via Sigstore/cosign|Rekor transparency-log entries for every release|carries a Sigstore" apps/docs/docs/ 2>/dev/null | wc -l | tr -d " ")
  cmd=$(find apps/docs/docs -type f \( -name "*.mdx" -o -name "*.md" \) -exec awk "match(\$0,/^[ \t]*\`\`\`/){f=!f;next} f && /cosign verify-blob/{print FILENAME; exit}" {} \; 2>/dev/null | sort -u | wc -l | tr -d " ")
  bad=$((claim + cmd))
  [ "$bad" = 0 ] || { echo "FALHA: $bad arquivo(s) da doc publica prometem assinatura Sigstore/Cosign que nao existe (promessa declarativa: $claim; comando dentro de cerca de codigo: $cmd)."; exit 1; }
  echo "done: cosign-sign.yml apagada e nenhuma promessa de assinatura ressuscitou na doc (controle: $ctl arquivos em trust/)"'
verify-means: |
  done — polaridade INVERTIDA, como exige um item fechado: ele agora falha se a
  lane VOLTAR, não se ela existir.

  Duas cláusulas, porque o fechamento tem duas metades e deixar só a primeira
  seria um portão pela metade. A primeira exige que `cosign-sign.yml` continue
  ausente. A segunda é a que importa mais e é a razão de o item existir: exige
  que as promessas de assinatura **não ressuscitem na doc do cliente**. Foi
  exatamente esse o dano — o waiver marcava a lane como resolvida, a varredura de
  lanes hosted não a procurava do lado protegido da fronteira, e enquanto isso a
  doc pública prometia entradas Sigstore/Rekor que nunca foram emitidas.

  A cláusula de controle (`ls apps/docs/docs/trust/*.mdx`) existe porque um
  `grep -rl` num diretório que sumiu devolve zero — e zero, aqui, significaria
  "tudo limpo". Sem o controle, uma reorganização da doc fecharia este portão
  silenciosamente. Com ele, o item sai como INDETERMINADO e pede investigação.

  O que este comando NÃO decide: se CoreLink voltou a assinar releases de verdade.
  Ele só garante que a doc não promete o que não existe. Assinatura real é
  trabalho novo, rastreado no [B-091].
last-verified: 2026-08-31
```

---

### B-119 — `POST /v1/admin/ops` é documentado, chamado por três clientes, e não existe

A superfície de aprovação dupla é publicada como quatro endpoints — `POST /v1/admin/ops`,
`GET /v1/admin/ops/{op_id}`, `POST /v1/admin/ops/{op_id}/approve`,
`POST /v1/admin/ops/{op_id}/reject` — cada um com página própria em
`apps/docs/docs/reference/api/endpoints/`.

**Nenhum servidor registra nenhum dos quatro.** O contêiner registra `/v1/admin/mutate` e
`/v1/admin/approve`.

O que torna este caso grave não é a ausência, é **quantas coisas já foram construídas em
cima dela**:

- `apps/admin-ui/src/lib/admin-client.ts:139-163` chama os quatro.
- `crates/corelink-wasm/examples/quickstart_team_invite.ts` e
  `quickstart_byok_rotate.ts` — **exemplos de quickstart publicados dos SDKs** — chamam
  `POST /v1/admin/ops` e `POST /v1/admin/ops/{op_id}/approve`.
- `crates/corelink-dual-approval/src/types.rs:8` tem um tipo cujo doc-comment diz
  *"Corresponds to `POST /v1/admin/ops` body after header extraction."*

E o crate que teria a implementação **não está no binário servido**:

```
cargo tree -p corelink-server --edges normal
  corelink-dual-approval        0 ocorrências
  corelink-cas   (controle)     2 ocorrências
  arvore total                  1513 linhas
```

O controle e o total não são enfeite: sem eles, um zero é indistinguível de um comando que
falhou. Ler o crate também não decide — ele compila igual estando ou não ligado ao binário;
só a árvore de dependências do binário responde.

**Por que ninguém percebeu.** O admin-ui tem `src/lib/e2e-mock-fixtures.ts` e um catch-all
em `src/app/api/v1/[...path]/route.ts` que **simulam** essas rotas. O próprio arquivo se
declara mock de teste e devolve 503 em produção. O e2e passa contra o mock, e o mock é a
única coisa que implementa o endpoint — teste verde provando o simulador, não o produto.

```backlog
id: B-119
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'docs=$(ls apps/docs/docs/reference/api/endpoints/ 2>/dev/null | grep -c "admin-ops" | tr -d " ")
  [ "$docs" -gt 0 ] || { echo "FALHA: nenhuma pagina de doc de admin/ops encontrada — ou foram removidas (feche) ou o caminho mudou; investigue antes de fechar."; exit 1; }
  srv=$(grep -rn -- "\"/v1/admin/ops" crates/ worker/src 2>/dev/null | wc -l | tr -d " ")
  [ "$srv" = 0 ] || { echo "FALHA: /v1/admin/ops agora tem $srv literal(is) no servidor — a rota nasceu, feche ou reescreva o item."; exit 1; }
  echo "aberto: $docs pagina(s) publicam /v1/admin/ops e 0 sitio de servidor registra a rota"'
verify-means: |
  open — a doc publica a superfície e nenhum servidor a registra.

  Os dois lados são medidos. Um predicado de um lado só fecharia sozinho pelo lado errado:
  apagar as páginas fecharia o item sem entregar a funcionalidade, e a funcionalidade
  aparecer sem corrigir a doc deixaria a divergência viva ao contrário.

  O que este comando NÃO decide, e é o mais importante: **qual das duas superfícies é a
  verdadeira.** Pode ser que a doc esteja adiantada (a feature nunca foi ligada) ou que ela
  esteja atrasada (a feature virou `/v1/admin/mutate` + `/v1/admin/approve` e a doc não
  acompanhou). O reparo é completamente diferente nos dois casos, e quem fechar tem de
  dizer qual é — com `corelink-dual-approval` e `routes/admin.rs` abertos lado a lado.

  Também não decide os clientes. Se a superfície documentada for abandonada, os dois
  exemplos de quickstart dos SDKs e o `admin-client.ts` passam a chamar rota inexistente e
  precisam ir junto. Fechar este item sem varrer os consumidores só move a mentira de
  lugar.
last-verified: 2026-08-30
```

---

### B-120 — `POST /v1/enterprise/inquire` idem: doc, crate, e nenhuma rota

`apps/docs/docs/reference/api/endpoints/post-v1-enterprise-inquire.mdx` publica o endpoint e
afirma *"Implemented by `corelink-enterprise-inquiry`. Atomic outbox to …"*. O
`openapi-corelink-v1.yaml` repete a atribuição.

O crate existe e é dependência de `corelink-ops` e `corelink-slack-real`. **Não há literal
de caminho `/v1/enterprise/inquire` em `crates/` nem em `worker/src`**, e o crate não está
no binário servido:

```
cargo tree -p corelink-server --edges normal
  corelink-enterprise-inquiry   0 ocorrências
```

Mesma classe do [B-119], e com um agravante comercial: é a porta pela qual um cliente
enterprise pede contato. O owner **vende** enterprise. Um formulário de contato enterprise
que não existe é receita perdida em silêncio, não um defeito de documentação.

```backlog
id: B-120
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'test -f apps/docs/docs/reference/api/endpoints/post-v1-enterprise-inquire.mdx || { echo "FALHA: a pagina do endpoint sumiu — se foi decisao, feche o item registrando o motivo."; exit 1; }
  srv=$(grep -rn -- "\"/v1/enterprise/inquire\"" crates/ worker/src 2>/dev/null | wc -l | tr -d " ")
  dep=$(grep -c "corelink-enterprise-inquiry" crates/corelink-container/Cargo.toml 2>/dev/null | tr -d " ")
  [ "$srv" = 0 ] || { echo "FALHA: /v1/enterprise/inquire aparece $srv vez(es) como literal de caminho no servidor — a rota pode ter nascido; verifique e feche."; exit 1; }
  [ "$dep" = 0 ] || { echo "FALHA: o binario do conteiner agora depende de corelink-enterprise-inquiry — a implementacao pode ter sido ligada; verifique e feche."; exit 1; }
  echo "aberto: a doc publica /v1/enterprise/inquire, nenhum literal de caminho o registra, e o crate nao e dependencia do binario servido"'
verify-means: |
  open — a página existe e o caminho não aparece em nenhum servidor.

  O predicado do lado do servidor é deliberadamente **largo**: qualquer menção ao caminho,
  não só um `.route(...)`. Rota registrada por constante não casaria com o padrão estreito,
  e foi exatamente assim que uma primeira varredura minha reportou 32 endpoints ausentes
  quando o número real era muito menor — o instrumento estava errado, não o repo.
  Predicado largo erra para o lado seguro: pode deixar de acusar, nunca acusa à toa.

  O que este comando NÃO decide: se a inquirição enterprise é atendida por **outro**
  caminho (um formulário no site, um e-mail, o Slack via `corelink-slack-real`). Se for,
  o reparo é apagar a promessa de endpoint e apontar o caminho real — e nesse caso o item
  fecha por decisão registrada, não por o grep mudar de valor.
last-verified: 2026-08-30
```

---

### B-121 — nada compara a superfície documentada com a superfície servida

[B-119] e [B-120] não são dois defeitos, são duas amostras. A varredura que os achou
encontrou mais, e o que falta é o portão, não os consertos.

Comparando os 45 endpoints de `apps/docs/docs/reference/api/endpoints/` com todos os
literais de caminho de `crates/`, `worker/` e `apps/`:

| endpoint documentado | situação no código |
|---|---|
| `POST /v1/admin/ops` (+ `{op_id}`, approve, reject) | não registrado — [B-119] |
| `POST /v1/enterprise/inquire` | não registrado — [B-120] |
| `POST /v1/dpa/accept` | caminho real é `/v1/onboarding/dpa-accept` — [B-116] |
| `GET /v1/audit/export` | caminho real é `/v1/audit/{tenant}/export` — falta o segmento de tenant |
| `GET /v1/admin/audit/events` | não registrado |
| `GET /v1/admin/tenants` (lista) | não registrado; só existem as sub-rotas `/{tenant_id}/…` |
| `GET /v1/data-categories` | não registrado; só `admin-ui/src/lib/dsr-client.ts` o chama |
| `GET`/`POST` `/v1/pats`, `DELETE /v1/pats/{pat_id}` | só existe a variante admin `/v1/admin/tenants/{tenant_id}/pats` |

Duas formas distintas aparecem aqui e o reparo difere: **ausência** (a rota não existe) e
**divergência de caminho** (a rota existe com outro caminho). A segunda é pior para o
cliente, porque a doc parece certa e a chamada falha com 404 depois de autenticar.

**A raiz é a especificação, não as páginas.** `scripts/gen-api-reference.py` **gera** as 45
páginas a partir de `openapi/corelink-v1.yaml`, uma por `paths.<path>.<method>`. Então as
páginas não são a fonte da divergência — são a sua propagação, e regenerá-las não conserta
nada. A fonte é o OpenAPI, que é escrito à mão e **nunca é confrontado com as rotas
servidas**. O comparador certo é `openapi/corelink-v1.yaml` × rotas registradas; a doc
publicada segue de graça. Isso também define quem tem de mudar quando o veredito sair: a
especificação, não o MDX.

**A lição de método vale mais que a lista.** A primeira varredura reportou 32 ausências. Era
o instrumento: `grep '\.route("…'` é por linha, e este repo registra rota com o caminho na
linha seguinte, ou por constante (`TURBO_GET_ROUTE`, `AUDIT_EXPORT_ROUTE`,
`ROUTE_EVENT_COUNT`). Um resultado implausível é sinal contra o próprio instrumento antes
de ser sinal contra o repo — e um portão automatizado tem de nascer com essa lição embutida,
senão vira uma fonte de alarme falso que alguém acaba silenciando.

```backlog
id: B-121
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'n=$(grep -rln "reference/api/endpoints" scripts/ .github/workflows/ 2>/dev/null | grep -v "gen-api-reference" | wc -l | tr -d " ")
  [ "$n" = 0 ] || { echo "FALHA: $n script/lane alem do gerador referencia o diretorio de endpoints — pode ser o comparador nascendo; verifique e feche se for."; exit 1; }
  eps=$(ls apps/docs/docs/reference/api/endpoints/*.mdx 2>/dev/null | wc -l | tr -d " ")
  [ "$eps" -gt 0 ] || { echo "FALHA: nenhuma pagina de endpoint encontrada — o item pressupoe que a superficie documentada existe; reavalie."; exit 1; }
  echo "aberto: $eps endpoints documentados e nenhum script ou lane compara essa lista com as rotas servidas"'
verify-means: |
  open — a superfície documentada existe e nada a compara com a servida.

  O predicado mede a **presença do comparador**, não a ausência de divergências. Escolhi o
  mais estreito e mais honesto: contar divergências dentro de um `verify:` exigiria embutir
  ali a varredura inteira, e uma varredura frágil dentro de um portão é pior que nenhuma —
  ela vira alarme falso, alguém a silencia, e a supressão-com-motivo sobrevive muito mais
  tempo que um build vermelho.

  O que este comando NÃO decide: se o comparador, quando existir, é **correto**. Um que
  extraia rota por `grep '\.route("'` reprova este item ficando verde e não enxerga nada —
  foi o que aconteceu na primeira tentativa desta varredura. Quem fechar precisa provar o
  contrário: rodar o comparador contra as divergências já conhecidas ([B-116], [B-119],
  [B-120] e as quatro da tabela) e mostrar que ele **acusa as oito**. Portão que nunca foi
  visto pegando o defeito conhecido não é portão.

  Nota de escopo: o comparador natural é bidirecional. Uma direção pega doc sem rota
  (o que está aqui); a outra pega rota sem doc, que é [B-117]. Um único instrumento
  fecha as duas famílias.
last-verified: 2026-08-30
```

### B-122 — regiao de fase sob `spawn_blocking` nao registra nada, e o `ostore` nao se separa sem mover as fronteiras

Dois defeitos que so aparecem juntos, e o segundo e a razao pela qual o [B-107] esta
bloqueado.

**Primeiro: existe tempo real que nao aparece em fase nenhuma.** O ledger de fases e um
task-local. `tokio::task::spawn_blocking` troca de task, entao qualquer
`PhaseScope::enter` alcancado dentro de um closure de `spawn_blocking` nao acha ledger
ambiente e **registra zero, em silencio**. O mecanismo correto existe e ja e usado — o
`adapter_pat.rs` captura `current_ledger()` antes da fronteira e usa
`PhaseScope::with_handle` no caminho do Argon2id. O `adapter_cache.rs` **nao**: tem dois
sitios sob `spawn_blocking`, a leitura (`:305`) e a escrita (`:389`), e **nenhum dos dois
captura handle**.

Consequencia, dita sem suavizar: **existe tempo real que nao aparece em fase nenhuma, e
ninguem sabe se e 5 ms ou 300 ms.** Ele nao some do total — cai no residuo `oother`, que e
subtracao — mas some da atribuicao, que e o que decide o que otimizar.

**Segundo: mesmo resolvido o primeiro, o `ostore` continua inseparavel.** O
`Phase::Store` externo (`adapter_cache.rs:351-353`) envolve o `put_untimed` inteiro, e o
accrue acontece dentro do `handler.write` que ele envolve. Uma fase nova para a
contabilidade cobriria a **mesma janela** que o `ostore` ja cobre, e como sao fases
distintas o resultado nao e dobra dentro de um nome: e `Sigma(fases) > total`.

E nao ha saida pela borda. O Worker deriva o `ohop` por subtracao das fases nomeadas
(`ORIGIN_CONTAINER_PHASES`): nome fora da allowlist cai em `ohop` e vira "rede"; nome dentro
e subtraido. **Nao existe terceira categoria** — um campo "informativo que se sobrepoe" nao
cabe no formato, e publicar um quebraria a propriedade que o `Server-Timing` do produto
vende.

A causa e a forma do decorator: o accounting **envolve** a escrita em vez de ficar ao lado
dela. Separar exige mover ONDE as fases abrem.

**As tres pecas sao ACOPLADAS e o item so fecha com as tres.**

1. **Fallback de handle para regiao bloqueante** — de modo que uma fase alcancada sob
   `spawn_blocking` registre.
2. **Controle de profundidade no `PhaseScope`** — OBRIGATORIO no mesmo movimento. Sem ele, o
   `Phase::Store` interno do `R2CasHandler::write` passa a registrar assim que a peca 1
   entrar, a janela do R2 e contada duas vezes, e o `(total - attributed).max(0)` do `oother`
   **grampeia em zero e absorve a diferenca em silencio**.
3. **Remedicao de TODA linha de base existente** — os numeros mudam para todo caminho que
   hoje perde tempo em `spawn_blocking`, inclusive os 654 ms do [B-107] e o perfil de
   [B-102]. Trocar a regua invalida a comparabilidade do que ja foi levantado.

**Criterio de aceitacao, literal: nao fazer isto pela metade.** Fallback sem profundidade
entrega numeros MAIORES que o relogio de parede e um residuo grampeado — pior que a cegueira
atual, porque parece medicao. Um item que pode ser fechado por uma das tres pecas vai ser
fechado por uma das tres pecas.

```backlog
id: B-122
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'a=crates/corelink-container/src/adapter_cache.rs
  [ -f "$a" ] || { echo "FALHA: adapter_cache.rs sumiu — reavalie o item."; exit 1; }
  sb=$(grep -c "spawn_blocking" "$a" | tr -d " ")
  [ "$sb" -gt 0 ] || { echo "FALHA: nao ha mais spawn_blocking no adapter_cache — reavalie."; exit 1; }
  handle=$(grep -c "with_handle\|current_ledger" "$a" | tr -d " ")
  [ "$handle" = 0 ] || { echo "FALHA: o adapter_cache ja captura handle de ledger — feche ou reescreva o item."; exit 1; }
  echo "aberto: $sb sitio(s) de spawn_blocking no adapter_cache e ZERO captura de handle"'
verify-means: |
  open — o `adapter_cache` ainda entrega trabalho a `spawn_blocking` sem capturar handle do
  ledger, logo qualquer fase alcancada la dentro registra zero.

  Vira DRIFTED quando o `adapter_cache` passar a capturar handle, que e a peca 1.

  **O que este comando NAO decide, e admito, porque e a maior parte do item:** as pecas 2 e
  3. Ele nao ve se o `PhaseScope` ganhou controle de profundidade, e nao ve se as linhas de
  base foram remedidas. Um `verify` que fechasse so na peca 1 seria exatamente o portao
  dominado que o criterio de aceitacao proibe — e eu prefiro dizer isso a fabricar um
  comando que parece decidir tres coisas.

  Quem fechar deve provar as tres a mao: (1) uma fase sob `spawn_blocking` aparecendo no
  header; (2) um teste que aninhe a MESMA fase na MESMA task e mostre `Sigma(fases) <=
  total` — hoje `origin_timing::nested_reentry_of_the_same_phase_double_counts` fixa o
  comportamento oposto e teria de ser invertido, o que e o sinal de que a peca 2 entrou;
  (3) linhas de base novas para [B-102] e [B-107], com a versao de producao registrada.

  Nota de ordem: [B-107] esta BLOQUEADO por este item e nao deve receber otimizacao antes
  dele. Enquanto o `ostore` agregar R2 com contabilidade D1, qualquer melhora medida e
  inatribuivel entre os dois.
last-verified: 2026-08-30
```

### B-123 — the OKF gate pays for the wrong fix: a blob anchor is vacuously green

When a branch shifts lines in a cited file, there are two things a person can
do, and the gate cannot tell them apart:

| | cost | gate result |
|---|---|---|
| advance the `source_blobs` anchor | one command | **green immediately** |
| renumber every citation | a script, byte-for-byte content verification, plus a manual sweep of the abbreviated citations no gate can see | green, eventually |

**The atalho is cheap and the gate rewards it.** That is not a discipline
problem to be solved with more care — it is a defect of INCENTIVE in the gate,
and it will keep recurring for exactly as long as that cost ratio holds.

**The root, stated plainly:** with a `source_blobs` anchor present, C5 compares
the tree **with itself** for that file. It is therefore **vacuously true** for
the anchored file — it detects drift introduced AFTER the anchor and hides the
drift the anchor was written on top of.

**Hard evidence, one day, two independent sessions, 100% recurrence:** on
2026-08-30 the anchor shortcut was taken on #1389 and on #1393 — by two
different sessions, **both of which knew the caveat** — and in both cases
`validate_okf` reported **0 stale** while citations pointed at wrong lines: 17
of 18 `main.rs` citations in `planes/container.md` (#1389) and 16 more in
#1393. A sample of two with total recurrence, among people who knew better, is
not anecdote.

**The remedy is NOT to remove the anchor.** Without it the gate red-flags a file
the branch legitimately alters, which is a real need. The point is that the
anchor **does not substitute** for renumbering — the two solve different
problems and collapse into one in the mind of anyone in a hurry, including the
author of the instruction that recommended it.

**[B-059] is the amplifier.** A citation in the abbreviated `` `:N-M` ``
continuation form is invisible to `CITE_RE`, so those are never reported by any
gate, ever. A vacuously-green anchor **plus** a blind surface is how 17 wrong
citations survive two reviews. Seven of the ones corrected on #1389 were in
that form.

What would close this: C5 should refuse to treat an anchored file as fresh
without evidence the citations were re-verified — e.g. require the citation's
line CONTENT to match across the anchor boundary rather than comparing the file
to itself.

Relates to [B-059] (the blind surface), [B-049] (anchors orphaned by
squash/rebase) and [B-124].

```backlog
id: B-123
repo: corelink-server
owner: tl
status: open
verify: |
  grep -q 'source_blobs' scripts/validate_okf.py \
    && ! grep -q 'anchor_content_reverify' scripts/validate_okf.py
verify-means: |
  open — passes while validate_okf still honours a source_blobs anchor without
  any content re-verification of the citations it covers, i.e. while the anchor
  remains a vacuously-green shortcut. Closes when the gate carries a named
  re-verification step. It does NOT prove any specific concept is wrong today —
  only that the cheap wrong fix is still available and still rewarded.
last-verified: 2026-08-30
```

### B-124 — hand-edits and a bulk shifter over the same file double-shift it

A bulk citation shifter computes each offset from the ORIGINAL line numbers, so
it is **not idempotent** against a partially hand-corrected file: a citation
already fixed by hand gets moved a second time.

Reproduced on #1389: `main.rs:48-55` was corrected by hand to `49-56`, then the
programmatic shifter ran over the same concept and produced **`50-57`**, which
points at a blank line. Nothing failed — `validate_okf` was green, because the
checkpoint had been advanced in the same operation.

**The rule, and it is per FILE, not per session:** pick one method and stay with
it. If a hand edit already happened, `git checkout -- <file>` back to the base
and let the script do all of them; if the script cannot handle a case, exclude
that file from the script entirely and do it all by hand.

**The check that catches it either way** is content verification: the line at
the OLD number in the base must be byte-identical to the line at the NEW number
in the branch. A double shift fails that immediately.

Relates to [B-123] and to the citation-token replacement hazard already known
in this repo (replacing `file:N` corrupts a neighbouring `file:N-M`).

```backlog
id: B-124
repo: corelink-server
owner: tl
status: open
verify: |
  ! test -x scripts/okf_shift_citations.py
verify-means: |
  open — passes while there is no repo-owned, idempotent citation shifter, so
  each session writes its own throwaway and re-meets this hazard. Closes when a
  shared tool exists that is safe to run twice. It does NOT prove any concept
  is currently double-shifted; that is what content verification is for.
last-verified: 2026-08-30
```

### B-125 — the audit chain seals 200 rows/hour, so evidence lags the event by hours

Measured against production D1 (`d64742ea`) on 2026-08-30, with a control query
beside every count so a zero means absence and not a broken instrument. Full
method in `reports/go-live/D-1-audit-trail.md`.

| measure | value |
|---|---|
| rows sealed per hour, six consecutive hours | **200, 200, 200, 200, 200, 200** |
| seal latency, last 24 h (n=2692) | min **12 s**, mean **1 h 28 min**, max **5 h 13 min** |
| unsealed backlog | **4 243** of 77 935 |
| oldest unsealed row | **6.8 h** |
| arrivals over the same window | 86, 300, 2, 30, 88, 92 (mean ≈ 100/h) |

Six identical integers is a hard cap, not a load curve. The drain currently
outpaces the mean arrival rate, so the backlog shrinks — but the entire margin is
a factor of two against a bursty process whose **observed peak of 300/h already
exceeds the ceiling**. A sustained burst accumulates and then drains only at
200/h.

**What the customer gets:** an event is not tamper-evident when it happens. It
becomes so ~1.5 h later on average, and in the observed tail **over 5 h** later.
Anything read from `audit_outbox` inside that window is present but outside the
chain.

**Not decided here, deliberately:** where the 200 comes from. It is not a literal
in `routes/audit_drain.rs` — grep returned 0 there against a control of 5 hits
for another string in the same file, so the absence is real; the value is passed
by the caller and the caller was not identified. Guessing it would be the kind of
claim this campaign keeps retracting.

**What IS settled and should stop being re-litigated:** the chain head signing is
correct and live — 365 of 365 heads carry an Ed25519 signature, and the seed is a
**write-only Cloudflare secret** (`ERASURE_ATTESTATION_SEED_HEX`, reused by
design), with **zero** occurrences in `wrangler.toml` against a control of 5. An
earlier claim of a plaintext seed in the repo was mine, made from memory without
measuring, and is retired.

Relates to [B-054] (the per-link hash is un-keyed, so the head signature carries
the whole tamper-evidence guarantee — which makes seal LATENCY the window in
which there is no guarantee at all).

```backlog
id: B-125
repo: corelink-server
owner: tl
status: open
verify: |
  grep -q 'fn resolve_seed' crates/corelink-container/src/routes/audit_drain.rs
verify-means: |
  open — a STRUCTURAL pin, not a measurement: it passes while the drain still
  exists in its current shape. It deliberately does NOT assert a throughput
  number, because a verify that greps for "200" would go green the moment
  someone changed the constant without changing the latency, and a verify that
  queried prod would make the gate depend on the network. The real check is
  re-running the queries in reports/go-live/D-1-audit-trail.md; this line only
  guarantees the item cannot be silently closed while the drain is untouched.
last-verified: 2026-08-30
```

---

### B-126 — 81 arquivos acima de 1000 linhas, e nenhum portão os impede de crescer

**Decisão do owner (2026-08-31): todo arquivo acima de 1000 linhas é refatorado em
arquivos menores.** Este item rastreia a campanha e o portão que a torna irreversível.

Inventário em `reports/refactor/god-files-2026-08-31.tsv`, gerado por comando e não por
memória: **81 arquivos, 143 667 linhas**. Para calibrar — o repo tem 691 622 linhas
rastreadas em `.rs`/`.ts`/`.tsx`/`.py`, então **21% do código vive em arquivos que o owner
já declarou grandes demais**. Contando a partir de 800 linhas seriam 139 arquivos e 28%.

Concentração, que decide a sequência: **32 dos 81 estão em `crates/corelink-container`**,
5 em `corelink-gc`, 4 em `apps/signup-worker`. Os quatro maiores:

| linhas | arquivo |
|---|---|
| 5 738 | `crates/corelink-container/src/storage/r2_s3.rs` |
| 4 788 | `worker/src/index.ts` |
| 4 603 | `crates/corelink-container/src/adapter_pat.rs` |
| 4 535 | `crates/corelink-container/src/customer_d1.rs` |

**Isto não é higiene abstrata — os quatro do topo já custaram trabalho medido hoje.** O
`r2_s3.rs` é onde a investigação de performance foi procurar por que o `ostore` agrega R2
com D1 de contabilidade, e a resposta exigiu ler cinco camadas num arquivo só. O
`index.ts` é onde as citações OKF deslocam a cada mudança — o conserto de citações de um
único PR mexeu em 45 delas hoje, e um branch teve **17 de 18 apontando para linha errada**
com o portão verde.

**O risco da própria campanha, dito antes de começar.** Mover código entre arquivos
invalida **toda** citação OKF que aponte para ele, e o conceito continua verde se a âncora
for de blob — porque a âncora faz o C5 comparar a árvore consigo mesma. Uma refatoração
grande feita com âncora e sem renumeração por conteúdo produz uma wiki inteiramente
plausível e inteiramente errada, sem nenhum portão reclamar. Quem executar tem de
renumerar por conteúdo, arquivo por arquivo, e conferir **depois**.

**Ordem de execução, e a razão de cada degrau:**
1. **O portão primeiro**, em catraca: arquivo novo acima do teto reprova; arquivo já acima
   reprova se **crescer**. Sem ele, os 81 voltam a crescer enquanto a campanha corre, e a
   campanha não converge.
2. **A cauda longa antes da cabeça** — os que estão entre 1000 e 1500 saem com corte
   simples e derrubam a contagem rápido, dando sinal de progresso real.
3. **Os quatro maiores por último e um por vez**, porque cada um deles é superfície citada
   pela wiki e por medições em voo.

```backlog
id: B-126
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'inv=reports/refactor/god-files-2026-08-31.tsv
  test -f "$inv" || { echo "INDETERMINADO: inventario $inv sumiu — o item mede contra ele; restaure antes de concluir."; exit 0; }
  ctl=$(git ls-files "*.rs" | grep -vc "^$" | tr -d " ")
  [ "${ctl:-0}" -gt 100 ] || { echo "INDETERMINADO: git ls-files devolveu $ctl arquivos .rs — o instrumento falhou, nao o repo."; exit 0; }
  n=$(git ls-files "*.rs" "*.ts" "*.tsx" "*.py" | grep -vE "node_modules|/target/" | xargs wc -l 2>/dev/null | grep -v " total$" | awk "\$1>1000" | wc -l | tr -d " ")
  [ "$n" -gt 0 ] || { echo "FALHA: zero arquivos acima de 1000 linhas — a campanha terminou, feche o item."; exit 1; }
  base=$(wc -l < "$inv" | tr -d " ")
  echo "aberto: $n arquivo(s) acima de 1000 linhas (linha de base do inventario: $base)"'
verify-means: |
  open — ainda existe arquivo acima de 1000 linhas.

  O predicado **recontará do zero** a cada execução em vez de confiar no inventário: o
  arquivo `.tsv` é a linha de base histórica, e a contagem viva é o estado. Se os dois
  divergirem, a mensagem mostra os dois números — progresso e regressão ficam visíveis
  na mesma linha, sem que ninguém precise abrir o arquivo.

  A consulta de controle (`git ls-files "*.rs"` > 100) existe porque a contagem real pode
  cair a zero por dois motivos opostos: a campanha terminou, ou o comando parou de
  enxergar arquivos. Sem o controle, os dois são indistinguíveis — e a segunda leitura
  fecharia o item declarando vitória. Essa é a classe de defeito dominante desta campanha
  e não entra aqui pela porta de trás.

  O que este comando NÃO decide, e é o mais importante: **se a refatoração melhorou alguma
  coisa.** Contagem de linhas é métrica de superfície; um arquivo de 4 000 linhas partido
  em cinco de 800 com as mesmas responsabilidades embaralhadas fecha este `verify` e
  piora o código. Quem fechar precisa mostrar, por arquivo dividido, qual responsabilidade
  cada parte passou a ter — e que os testes que cobriam o original continuam cobrindo as
  partes.

  Também não decide o estado da wiki. Mover código invalida citações OKF, e a âncora de
  blob deixa o C5 **vacuamente verde** para o arquivo ancorado — ver [B-123]. Cada PR
  desta campanha tem de renumerar citações por conteúdo e conferir depois, e um `validate_okf`
  verde **não** é prova de que isso foi feito.
last-verified: 2026-08-31
```

### B-127 — 3 670 audit rows have no tenant, so the residency predicate is unevaluable — and a JOIN check reports clean

Measured against production D1 (`d64742ea`) on 2026-08-30, containers running
`cab16a3a-r1` (21 commits behind `main`, 5 of them container-affecting). Full
method in `reports/go-live/D-2-residency.md`.

| measure | value |
|---|---|
| rows in `audit_outbox` | **77 935** |
| rows that join to a `tenant` row | **74 333** |
| rows whose `tenant_id` has NO `tenant` row | **3 670** (175 distinct ids) |
| residency violations among the joinable rows | **0** |
| rows in `weur` | **4** — and **no tenant** declares `weur` |
| do those 4 have a tenant row? | **no, 0 of 4** |

**The finding is not the violation count.** It is that the invariant is
**unevaluable** for 4.7% of the population, and that the obvious way to check it
— join `audit_outbox` to `tenant` and compare `region` to `primary_region` —
**drops exactly those rows without saying so**. "0 violations" was computed over
a population the query had already trimmed.

Unevaluable is a third state, weaker than satisfied and different from violated,
and nothing in the system currently distinguishes it from satisfied.

Four of the orphans sit in `weur`, a region **no tenant is provisioned for**. The
count is trivial; the property is not — a row exists in a region whose residency
predicate has nothing to check against.

Secondary, and it bounds what any green result here means: **76 129 of 77 935
rows (97.7%) are `enam`**. The cross-region rejection path (409
`residency_violation`, plus the migration-0023 `RAISE(ABORT)` triggers) is real
code that production traffic has barely exercised. A control that never fires has
not been shown to fire.

**Cause, separated rather than left open** (control: `dsr_erasure_log` holds
2 044 rows, so the lookup is live): **170 of the 175** orphan tenants — **3 526
of the 3 670 rows** — appear in `dsr_erasure_log`. Those are CORRECT: audit rows
are RETAIN-class evidence under Art. 5(2) and must survive an Art. 17 erasure.
That shrinks the unexplained residual to **5 tenants / 144 rows** (3 in `wnam`,
1 in `weur`, 1 in `apac`) — including the `weur` rows above, which are therefore
NOT explained by erasure.

Recorded that way deliberately: the first measurement supported a much larger
claim, and the second one shrank it. The item states the smaller true number.

What would close this: a residency check that counts unevaluable rows as a
FAILING third bucket rather than dropping them — correct orphans and unexplained
orphans are equally invisible to a `JOIN` — plus an explanation for the residual
5.

Relates to [B-125] (the same audit table, seal latency) and to the erasure path,
which is the most likely legitimate producer of tenant-less rows.

```backlog
id: B-127
repo: corelink-server
owner: tl
status: open
verify: |
  grep -rq "region" migrations/d1/0023_residency_check_constraints.sql
verify-means: |
  open — a STRUCTURAL pin: it passes while the residency constraint migration is
  in the tree, i.e. while the mechanism this item is about still exists. It
  deliberately asserts NO count: a verify that hard-coded 3670 would go green on
  any change to the number rather than on the defect being fixed, and one that
  queried production would make the gate depend on the network and on
  credentials. The real check is re-running the queries in
  reports/go-live/D-2-residency.md. This line only guarantees the item cannot be
  silently closed while the mechanism is untouched.
last-verified: 2026-08-30
```

---

### B-128 — os runners ficam sem disco e o resultado é vermelho falso, não erro de disco

Em um único dia, **três** execuções distintas reprovaram por falta de espaço, e nenhuma
delas se anunciou como problema de infraestrutura:

```
#1393  6 portões vermelhos, logs inexistentes           (Mac, volume em 95 MB livres)
#1450  error: could not create incremental compilation crate directory
       …: No space left on device (os error 28)          (runner Linux, /opt/actions-runner)
       collect2: fatal error: ld terminated with signal 7 [Bus error]
```

**A forma do defeito é o que importa.** Um `ld` morto com `Bus error` e um crate que "não
compila" leem-se como defeito do PR. Quem recebe isso vai investigar o próprio código —
foi o que eu fiz, duas vezes, antes de achar a linha do `os error 28` enterrada no meio do
log. E na primeira ocorrência os logs nem existiam, então não havia nada para ler.

Pior: o vermelho é **atribuído ao PR errado**. Quem estiver na fila quando o disco encher
recebe a falha, e quem encheu o disco não recebe nada.

**As duas máquinas têm causas diferentes e reparos diferentes:**
- **Mac** — é o runner E a máquina do owner E onde as sessões compilam. Encheu porque 30
  worktrees acumularam `target/`; três deles somavam 31 GB. Reparo: higiene (apagar
  `target/` ao fim de cada pacote) mais, idealmente, um teto.
- **Runner Linux** — `/opt/actions-runner/_work/…/target` cresce entre execuções e nada o
  poda. Reparo é no `corelink-runners`: dimensionar o disco da imagem ou limpar `target/`
  no fim do job.

**Por que isto vira item agora e não depois.** A campanha B-126 vai gerar dezenas de PRs de
refatoração, cada um disparando compilação. Se o disco enche no meio, os vermelhos falsos
vão parecer defeitos de divisão de módulo — exatamente a leitura mais cara possível, porque
manda o autor desfazer um trabalho correto.

```backlog
id: B-128
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'ctl=$(ls .github/workflows/*.yml 2>/dev/null | wc -l | tr -d " ")
  [ "${ctl:-0}" -gt 50 ] || { echo "INDETERMINADO: so $ctl workflows encontrados — o instrumento falhou, nao a arvore."; exit 0; }
  guard=$(grep -rln "df -[hH]" .github/workflows/ 2>/dev/null | wc -l | tr -d " ")
  live=$(grep -rnE "No space left on device|ENOSPC" scripts/ .github/workflows/ 2>/dev/null | grep -vE ":[[:space:]]*#|# " | wc -l | tr -d " ")
  [ "$guard" = 0 ] || { echo "FALHA: $guard workflow(s) ja checam espaco com df — pode ser o reparo; verifique e feche."; exit 1; }
  [ "$live" = 0 ] || { echo "FALHA: $live linha(s) NAO-comentario ja tratam ENOSPC — pode ser o reparo; verifique e feche."; exit 1; }
  echo "aberto: nenhum workflow checa espaco com df e nenhuma linha executavel trata ENOSPC; a exaustao chega como vermelho generico"'
verify-means: |
  open — nada no repo distingue "sem espaço" de "seu código quebrou".

  O predicado mede **tratamento executável**, não menção. A primeira versão contava
  qualquer ocorrência da string e nasceu DRIFTED: os dois arquivos que ela achou —
  `pre-merge-gate-check.sh` e `run_tla_suite.sh` — mencionam ENOSPC em **comentário**,
  descrevendo o sintoma, sem detectar nada. Um predicado que confunde comentário com
  código fecharia este item por prosa.

  Ele mede também a **presença de tratamento**, não a ausência de incidentes. Contar
  incidentes exigiria consultar execuções pela rede, e um `verify` que depende de rede
  falha por motivo errado; além disso um dia sem incidente não é conserto.

  O que este comando NÃO decide: **se o tratamento, quando existir, funciona.** Um passo
  que só imprima "disco cheio" no fim do job fecharia este `verify` sem mudar nada — o
  ponto é o vermelho ser **atribuído** corretamente, não ser explicado depois. Quem fechar
  precisa mostrar um job que, com o disco cheio, sai com uma mensagem que o autor do PR
  reconhece como "não é você" **antes** de investigar o próprio código.

  Também não decide o reparo do lado do `corelink-runners` — dimensionar o disco da imagem
  ou podar `target/` no fim do job é trabalho naquele repo, e este item só rastreia o lado
  do servidor. Fechar aqui sem o outro lado deixa a causa viva.
last-verified: 2026-08-31
```

### B-129 — o `Server-Timing` foi desenhado para SOMAR, e somar e compativel com esconder

Item guarda-chuva. **B-107, B-109 e B-105 nao sao tres itens — sao tres sintomas de um
defeito de desenho, e ha um quarto previsivel.**

**O numero que dói.** Numa leitura de cache autenticada medida em producao (GET, total
1001 ms):

| fase | ms | nomeado |
|---|---|---|
| `auth` | 8 | sim |
| `wdb` | 391 | **7** (`qtier` 3 + `qresid` 4) |
| `origin` | 602 | **372** (`ostore` 371 + `oother` 1) |

Sobram **384 ms no residuo do `wdb`** e **230 ms no `ohop`**: **61% de uma leitura de cache
nao tem nome.** Esse e o argumento; o resto e contexto.

**A tese.** Toda vez que esta campanha tentou atribuir custo, esbarrou num residuo por
subtracao que nao e publicado. `oother` (139 ms, [B-109]); `ostore` agregando R2 com ate
cinco idas ao D1 sob um nome que se le como "armazenamento" ([B-107]); `qother` (384 ms,
aqui). O padrao nao e coincidencia: **uma particao que FECHA e condicao necessaria e NAO
suficiente para atribuir custo.** Fechar garante que nada sumiu do total — nao garante que
se saiba onde o tempo esta. O `Server-Timing` foi desenhado para somar, e somar e
compativel com esconder.

**A instrumentacao que resolveria metade existe e esta desligada.** O `qother` — o residuo
do `wdb` — e publicado apenas atras de flag (`worker/src/index.ts:278-279`: o padrao
publica `wdb`/`qtier`/`qbatch`/`qresid`, sem `qdo` e sem `qother`).

Custo de ligar: e um flag de Worker, sem rebuild de contêiner — mais barato que qualquer
das outras instrumentacoes desta campanha. **O que ligar NAO resolve:** o `qother` nomeia o
residuo do `wdb` como UM bloco. Ele diz quanto tempo esta la; nao diz do que e feito. Trocar
"384 ms sem nome" por "384 ms chamados `qother`" e progresso de atribuicao, nao de
diagnostico — e o `ohop` de 230 ms continua sendo subtracao no lado do Worker.

**A relacao com os outros tres, explicita.** Se este item for resolvido, os tres encolhem:
[B-107] deixa de comparar um nome que agrega duas coisas, [B-109] deixa de existir como
categoria, e [B-105] ganha um piso calculavel. Se cada um for atacado sozinho, **o quarto
residuo aparece** — e a campanha gasta um ciclo por sintoma.

**O que este item NAO decide, e e a parte honesta.** Se publicar todos os residuos e a
resposta certa. Pode ser que a granularidade correta seja **menos** fases e nao mais: um
`Server-Timing` com quinze nomes e tao pouco acionavel quanto um com tres, e cada nome novo
e uma chance de quebrar a particao (ver [B-122], onde adicionar UM nome ja colidia com o
`Store` externo). Nao tenho dado para escolher entre "mais nomes" e "fronteiras melhores", e
escolher sem dado e como os `oother` chegaram aqui.

```backlog
id: B-129
repo: corelink-server
owner: tl
status: open
verify: |
  bash -c 'w=worker/src/index.ts
  [ -f "$w" ] || { echo "FALHA: index.ts sumiu — reavalie o item."; exit 1; }
  gated=0; grep -qE "no .qother.|sem .qother.|without .qother" "$w" && gated=1
  grep -q "qother" "$w" || { echo "FALHA: qother nao existe mais no Worker — reavalie o item."; exit 1; }
  residuos=0
  grep -q "oother" crates/corelink-container/src/origin_timing.rs 2>/dev/null && residuos=$((residuos+1))
  grep -q "qother" "$w" && residuos=$((residuos+1))
  grep -q "ohop" "$w" && residuos=$((residuos+1))
  [ "$residuos" -ge 2 ] || { echo "FALHA: restam menos de 2 residuos por subtracao — feche ou reescreva o item."; exit 1; }
  echo "aberto: $residuos residuos por subtracao no caminho quente (oother/qother/ohop); qother gated=$gated"'
verify-means: |
  open — o caminho quente ainda tem dois ou mais residuos calculados por SUBTRACAO
  (`oother` no contêiner, `qother` e `ohop` no Worker).

  Vira DRIFTED quando sobrar menos de dois — o que acontece se os residuos forem eliminados
  por fronteiras melhores, nao por publicacao. Escolhi contar residuos em vez de exigir o
  flag ligado de proposito: **ligar o `qother` nao fecha este item**, so troca "sem nome"
  por "um nome que agrega". A alegacao e sobre atribuicao, nao sobre publicacao.

  **O que este comando NAO decide, e e a maior parte:** quanto tempo cai nos residuos. Isso
  so sai do `Server-Timing` de uma requisicao autenticada contra producao, e o gate nao tem
  credencial. O comando decide a existencia estrutural dos residuos, que vive no
  repositorio; o tamanho deles esta na prosa como instantaneo datado (384 ms + 230 ms de
  1001 ms, medido em 2026-08-30 contra o pin `4f9313e0`).

  Fecha quando uma leitura de cache autenticada tiver **menos de 10% do tempo total** em
  residuo — numero escolhido para que o resto seja atribuivel, nao para ser facil.

  Cross-ref: [B-107] (o `ostore` agrega e por isso nao e otimizavel como uma coisa so),
  [B-109] (o `oother`, ja consertado por divisao de portao — o unico dos tres que fechou),
  [B-105] (o piso de uma leitura nao e calculavel enquanto 61% nao tiver nome), [B-122]
  (adicionar UM nome ja colidiu com a particao, que e a evidencia de que "mais nomes" nao e
  obviamente a resposta).
last-verified: 2026-08-30
```
