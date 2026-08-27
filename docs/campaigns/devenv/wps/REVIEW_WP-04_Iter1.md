# WP-04 REVIEW — Iteration 1 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)
**Date:** 2026-08-26
**Verdict:** ❌ **FAIL — 9 BLOCKING ISSUES, 11 HIGH SEVERITY ISSUES, 6 MEDIUM SEVERITY ISSUES**

---

## 🔴 BLOCKING ISSUES (Must fix before sign-off)

### B1. **`this.envVars` does not exist in WP-01's finalized class**
- **Location:** Throughout (lines 192, 193, 196, 197, 235, 239, 279, 280, 307, 308, 349)
- **Code:** `this.envVars.PROFILE_NAME`, `this.envVars.WORKSPACE_NAME`, `this.envVars.CLW_TENANT`
- **Problem:** Per WP-01's finalized shape (`WP-01_RunnerDevEnvDO_Skeleton.md:225-274`), the class has NO `this.envVars` field. `STATIC_ENV_VARS` is a `private static readonly` constant; the mutable tenant-scoped envVars are passed to `super.start({ envVars: newEnvVars, ... })` (line 355) — they live on the Container instance, not the DO. The tenant-scoped values ARE persisted via `DevenvState` (line 95-141: `workspaceName`, `profileName`, etc.).
- **Real API:** `this.state.workspaceName` / `this.state.profileName` for names; `this.state.tenant` (or read back from `STATIC_ENV_VARS.CLW_ENDPOINT` + state) for tenant.
- **Fix:** Replace every `this.envVars.X` with `this.state.X` for names; tenant must be added to `DevenvState` (it currently isn't — see B2).

### B2. **`DevEnvState` is missing `clwTenant` — `recordUsage` cannot post to billing**
- **Location:** Section 3.2 (lines 64-69), Section 3.3 (line 349)
- **Code:** `body: JSON.stringify({ tenant_id: this.envVars.CLW_TENANT, ... })`
- **Problem:** WP-01's `DevenvState` (the discriminated union, lines 95-141) does not store `clwTenant`. WP-04 reads `this.envVars.CLW_TENANT` (which doesn't exist — see B1) and posts to the billing ingest. Even after fixing B1, the tenant is not in the state object; WP-07 owns billing and needs to know how to fetch it.
- **Fix:** Add `clwTenant: string` (or `tenantId: string`) to the `DevenvState` discriminated union's `starting`/`running`/`stopping` arms. Update WP-01 in the same pass (or document the cross-WP contract that WP-04 extends the state).

### B3. **`clw hydrate` JSON output has NO `chunks_total` / `chunks_uploaded` fields**
- **Location:** `hydrateViaClw` (lines 102-132)
- **Code:** `chunksTotal: report.chunks_total, chunksUploaded: report.chunks_uploaded`
- **Problem:** Verified against `crates/clw-cli/src/subcmds/hydrate.rs:114-122` — the actual hydrate JSON shape is `{root, files, bytes_total, bytes_from_cache, bytes_downloaded}`. There is NO `chunks_total`/`chunks_uploaded` (hydrate is a downloader, not an uploader — it has no chunk-upload counter). Reading these keys yields `undefined` → the `try/catch` always falls into the empty-metadata fallback, which is silently lossy.
- **Fix:** Either (a) drop the `chunksTotal`/`chunksUploaded` fields from `SnapshotMetadata` for hydrate (they make no sense), or (b) define two distinct shapes: `HydrateMetadata` and `SnapshotMetadata`. The hydrate report MUST use `bytes_downloaded` / `bytes_from_cache` to populate the metadata instead.

### B4. **`clw` has no `exec` subcommand — `execClw(["exec", "mkdir", ...])` will fail**
- **Location:** `onStart` (line 189)
- **Code:** `await this.execClw(["exec", "mkdir", "-p", "/data/chrome", "/data/workspace"], { captureOutput: false });`
- **Problem:** `clw` is a content-addressed cache tool. The verified CLI subcommands are `init/snapshot/hydrate/status/run/ls/rm/prune/erase/doctor/auth/uninstall/completions` (see `clw-cli/src/main.rs:163-191`). There is NO `clw exec` and never will be — `clw` does not spawn arbitrary subprocesses (that is the `clw run` surface, and only for memoized builds, not shell utilities). The DO must use a real shell path for `mkdir`.
- **Real API:** `clw` runs inside the container as a binary; arbitrary shell ops must go through `bash -c "mkdir -p ..."` via the same `exec()` call (which is what the Container class provides for arbitrary command exec, NOT clw's CLI).
- **Fix:** Either drop the `mkdir` step (entrypoint.sh / supervisord is responsible for directory creation — see WP-03 `validate_env` section), OR replace with `await this.execClw(["/bin/sh", "-c", "mkdir -p /data/chrome /data/workspace"], ...)`. Cleanest: delete the line — WP-03 owns directory provisioning.

### B5. **Hard-coded `--ref-domain runner` violates WP-01's static-env invariant source-of-truth**
- **Location:** `hydrateViaClw` (line 106), `snapshotViaClw` (line 140)
- **Code:** `"--ref-domain", "runner"`
- **Problem:** WP-01 declares `STATIC_ENV_VARS.CLW_REF_DOMAIN = "runner"` (frozen, immutable). The CLW binary already reads `CLW_REF_DOMAIN` from the environment, so passing `--ref-domain runner` is redundant-but-correct. However, the WP hard-codes the literal in TWO places — if the runner ever needed a non-runner domain (it never will, but), there are two magic strings to update.
- **Real API:** The container's `envVars` already include `CLW_REF_DOMAIN=runner`. The binary picks it up; no CLI flag is needed.
- **Fix:** Drop `--ref-domain runner` and `--concurrency 8` from both call sites — `CLW_REF_DOMAIN` and `CLW_CONCURRENCY` are already exported by entrypoint.sh and clw will read them (clw's `--concurrency` is global but the env fallback is `RUST_LOG`-style, not `CLW_CONCURRENCY`; verify in the binary — see H1). At minimum, drop `--ref-domain` (env-only) and either drop `--concurrency` (use clw's default 8) OR thread through a single helper that reads from one source.

### B6. **`hydrateViaClw` treats exit 2 as "not found" — WRONG**
- **Location:** `hydrateViaClw` (line 126-128)
- **Code:** `} else if (result.exitCode === 2) { return null; }`
- **Problem:** Per `clw-cli/src/main.rs:287-291`, `clw` calls `std::process::exit(2)` for **ANY clw-level error** (auth, network, config) — not specifically "not found". `clw hydrate` exit 2 covers: missing tenant, missing token, network 401/404, malformed manifest digest, etc. The first-run case (no existing snapshot) is ALSO exit 2, but for the SAME reason — clw errored and the error IS "workspace not found" (mapped via `map_hydrate_error`'s `ClwError::NotFound` branch to a clear message, then `exit_err` → `exit(2)`). The first-run case is indistinguishable from any other clw error purely from the exit code.
- **Real API:** To detect "not found" specifically, the caller must either (a) parse stderr for the substring "not found" / "is not in the CAS", (b) use `--json` and detect an `error` field, or (c) pre-check `clw ls --name <name> --json` and branch on its exit code (which IS exit 0 for exists, exit 2 for not-found after the SAME error-routing — no, this has the same problem). There is no clean way to discriminate from the exit code alone.
- **Fix:** Drop the `exit 2 → null` special case. Treat any non-zero exit as an error (log it, but for `onStart` MAKE THE HYDRATE OPTIONAL regardless: pre-check with `clw ls --name <name>` and skip hydrate if absent, OR use the WP-03 entrypoint.sh behavior — `trap 'snapshot_all' EXIT` — and let first-run be a no-op because the dest dir is empty). Cleanest: add a separate `clw ls --json --name X` pre-check; if not found, skip hydrate.

### B7. **State-machine bypass — WP-04 mutates `this.state` directly, ignoring WP-01's `transitionState`**
- **Location:** `onStart` (line 205-214), `onStop` (line 226, 245-251), `onError` (line 270), `snapshot` RPC (line 311-316)
- **Code:** `this.state = { ...this.state, status: "running", ... }; this.persistState();`
- **Problem:** WP-01 enforces a discriminated-union state machine with `transitionState()` (lines 291-312). It throws `INVALID_STATE_TRANSITION` on illegal transitions. WP-04 ignores it. Additionally, WP-06 expands the state machine to 7 states (`provisioning`, `starting`, `port_wait`, `running`, `stopping`, `stopped`, `errored`) — WP-04 never enters `port_wait` (the `waitForPorts` step) and skips the 7-state machine entirely.
- **Fix:** Use `this.transitionState({ status: "running", ... })` everywhere. Add a `port_wait` transition before marking `running`. Coordinate with WP-06 so the state machine is shared (move the canonical state machine + transition table into a shared helper, or document which WP owns the canonical version).

### B8. **`snapshot()` RPC payload type uses a different shape than WP-01**
- **Location:** Section 3.2 (line 303)
- **Code:** `async snapshot(payload: { force: boolean }): Promise<SnapshotResponse>`
- **Problem:** WP-01's `snapshot()` is defined at `WP-01_RunnerDevEnvDO_Skeleton.md:404-406` with the throwaway stub. WP-08 (`WP-08_Worker_Ingress_Routes.md:373`) declares the public schema as `SnapshotResponse` with a specific shape. WP-04 invents a different `SnapshotResponse` (`{ok, profileSnapshot: {root, bytesTotal}, workspaceSnapshot: {root, bytesTotal}}` per line 318-322) that is not the same as the worker-routes schema, AND uses an ad-hoc `SnapshotRequest` shape `{force: boolean}` not declared anywhere.
- **Fix:** Import the canonical `SnapshotRequest`/`SnapshotResponse` types from `src/types/devenv.ts` (or wherever WP-01/08 place them). If those types don't exist yet, declare them in a shared module that all WPs import.

### B9. **`acquireSnapshotLock` is racy with multiple DO instances / DOs across the same workspace name**
- **Location:** `acquireSnapshotLock` (lines 166-174)
- **Code:** Mutates in-memory `this.state.snapshotInProgress = true`.
- **Problem:** A DO is single-threaded WITHIN one instance — so the in-memory check is fine for a single DO. BUT WP-04 stores the lock in `this.state` (a field of this DO) and persists it. If the DO is restarted, `snapshotInProgress` could be left as `true` from a prior run that crashed mid-snapshot — the new instance will then refuse to take a lock it can never release. Also, the lock is in-process: nothing prevents the same tenant from running TWO DevEnvs concurrently with the same workspace name and clashing the AC ref key (the create-only AC would 409, but the local `execClw` would still run).
- **Fix:** Use a `try/finally` to guarantee `releaseSnapshotLock` is called; the current code does this. BUT: a crashed snapshot leaves `snapshotInProgress = true` forever. On `initializeState`, if `state.snapshotInProgress === true`, set it to `false` and log a warning (best-effort recovery). Document that the lock is intra-DO only and cross-DO consistency is delegated to the AC's create-only semantics (with `--force`).

---

## 🟠 HIGH SEVERITY ISSUES

### H1. **`--concurrency 8` hard-coded — no configurability**
- **Location:** `hydrateViaClw` (line 107), `snapshotViaClw` (line 141)
- **Problem:** 8 is clw's default and matches entrypoint.sh, but is duplicated. If the binary's default changes, WP-04 silently desyncs.
- **Fix:** Either read from env (`CLW_CONCURRENCY`) or define a `static SNAPSHOT_CONCURRENCY = 8` constant at module top.

### H2. **`containerHandle` is set from `this.ctx.id.toString()` — wrong abstraction**
- **Location:** `onStart` (line 210)
- **Code:** `containerHandle: this.ctx.id.toString()`
- **Problem:** `this.ctx.id` is the **Durable Object** ID, not the container handle. The container's identity is separate (`this.ctx.container.id` per the Container SDK). Using the DO id as the container handle leaks DO identity into the container-id namespace and breaks observability (a `container_id` field that is the same for the lifetime of a DO is technically correct, but naming it `containerHandle` and using the DO id is misleading).
- **Fix:** Use the actual container handle (per `@cloudflare/containers` SDK) or rename the field to `devenvId` / `doId` and document the meaning.

### H3. **`recordUsage` posts to a billing endpoint that is not part of WP-01/WP-07's contract**
- **Location:** `recordUsage` (lines 336-359)
- **Code:** `fetch(\`${this.env.BILLING_INGEST_URL}/v1/usage\`, ...)`
- **Problem:** WP-07 (Billing Metering) owns billing. WP-04 invents the endpoint (`/v1/usage`), headers (`BILLING_INGEST_AUTH_KEY`), and request shape. This is a cross-WP contract that is not documented in WP-07's scope and not shared with any other WP.
- **Fix:** Move `recordUsage` to WP-07's spec (or import WP-07's exported helper). WP-04 should only CALL the helper, not define the request shape. The `BILLING_INGEST_URL` env var must be declared in WP-01's `Env` interface.

### H4. **`recordUsage` swallows all errors — even the vCPU math is wrong**
- **Location:** `recordUsage` (lines 336-359)
- **Code:** `const vcpuSeconds = Math.floor((Date.now() - this.state.startedAt) / 1000) * 4;` then `try { fetch(...) } catch (err) { this.log(...) }`
- **Problem:** Two bugs:
  1. `Math.floor((ms)/1000) * 4` is WRONG ORDER OF OPERATIONS — `Math.floor` runs first, dividing ms-by-1000 to get seconds, THEN multiplying by 4. But "4" is supposed to be the vCPU count, so the right expression is `Math.floor(((Date.now() - this.state.startedAt) / 1000) * 4)`. As written, this is `(seconds) * 4`, not `floor(seconds * 4)`. For a 1-second run it returns `4` (correct), for 1.5s it returns `floor(1.5)*4 = 4` instead of `floor(6) = 6` (under-reporting). And the vCPU count "4" is hard-coded — the actual vCPU count is a property of the container class (it should come from a `this.ctx.container.vcpu` or be a config field in WP-01).
  2. `try/catch` swallows fetch errors silently. WP-04's risk register row says "Fire-and-forget; log error but don't fail snapshot" — but it doesn't even log (only the `err` is in scope, never logged). The catch block has `this.log(\`recordUsage failed: ${err}\`)` — wait, it DOES log. So this is fine for logging, but the catch block makes it impossible to test "did we post usage?" and the post is best-effort without a retry queue. For a billing endpoint that DRIVES INVOICING, silent drops are unacceptable.
- **Fix:** (1) Fix the math: `const elapsedSec = (Date.now() - this.state.startedAt) / 1000; const vcpuSeconds = Math.floor(elapsedSec * vcpuCount);` where `vcpuCount` comes from a `VCPU_COUNT` config constant (default 4, declared in WP-01). (2) Hand off to WP-07's billing-usage helper which owns retry/durability. (3) Add a `pendingUsagePost` flag to state so a failed post is retried on the next start.

### H5. **`vcpuSeconds` math collapses to 0 if `startedAt` is null**
- **Location:** `recordUsage` (lines 337-339)
- **Code:** `if (!this.state.startedAt) return; const vcpuSeconds = Math.floor((Date.now() - this.state.startedAt) / 1000) * 4;`
- **Problem:** `Date.now() - null` is `NaN`, not "0". `Math.floor(NaN) * 4` is `NaN`. The `if (!startedAt) return` guard handles `null` correctly — but the field is `number | null`, and after WP-01's discriminated union, `startedAt` is only present in `starting`/`running`/`stopping` states. If `recordUsage` is called when state is `errored` or `stopped`, `startedAt` is undefined (not even present) — `this.state.startedAt` access would be a type error. Even with the `if (!this.state.startedAt) return` guard, the discriminated union says it should be `if (this.state.status !== "running" && this.state.status !== "stopping")` first.
- **Fix:** Type the guard: `if (this.state.status === "stopped" || this.state.status === "errored") return;` then access `this.state.startedAt`.

### H6. **`onError` does NOT call `transitionState` — bypasses the state machine AND doesn't mark `errored` properly**
- **Location:** `onError` (line 267-297)
- **Code:** `this.state = { ...this.state, status: "errored", lastError: err.message }; this.persistState();`
- **Problem:** Bypasses `transitionState` (see B7). Worse, the WP-01 5-state machine says `errored` requires the special form (line 126-141: `{status: "errored", lastError: string, ...}`). A direct spread mutation produces a state that is BOTH "errored" AND has `startedAt`/`workspaceName`/`profileName` — which is not a valid arm of the discriminated union and breaks the type guarantee.
- **Fix:** Use `this.transitionState({ status: "errored", lastError: err.message, ... })` with the exact arm shape from WP-01.

### H7. **`Promise.allSettled` result handling is buggy**
- **Location:** `onError` (lines 278-288)
- **Code:**
  ```ts
  if (profileMeta.status === "fulfilled") {
      this.state = { ...this.state, lastProfileSnapshot: profileMeta.value };
  }
  ```
- **Problem:** Two bugs:
  1. `profileMeta` is a `PromiseSettledResult<SnapshotMetadata>`, not the metadata itself. `profileMeta.status` is `"fulfilled" | "rejected"`, and `profileMeta.value` is only present when fulfilled. Reading `profileMeta.value` when rejected is `undefined` — but the type check makes that unreachable. However, the BIGGER bug is:
  2. The state is mutated THREE TIMES inside one function: (a) at the top (status=errored), (b) if profileMeta fulfilled (lastProfileSnapshot), (c) if workspaceMeta fulfilled (lastWorkspaceSnapshot). Each call to `this.persistState()` is a sync SQLite write (small but non-zero). Worse, if `recordUsage` then throws (it can't — it has its own catch), state is half-updated. The state should be computed ONCE and persisted ONCE at the end.
- **Fix:** Build a single `nextState = { ...this.state, status: "errored", lastError: err.message, lastProfileSnapshot: profileMeta.status === "fulfilled" ? profileMeta.value : this.state.lastProfileSnapshot, lastWorkspaceSnapshot: workspaceMeta.status === "fulfilled" ? workspaceMeta.value : this.state.lastWorkspaceSnapshot }` then call `this.transitionState(nextState)` once.

### H8. **`waitForPorts` is referenced but never defined**
- **Location:** `onStart` (line 202)
- **Code:** `await this.waitForPorts([6080, 7681, 8080], { timeoutMs: 60_000 });`
- **Problem:** No `waitForPorts` method exists in WP-04. WP-06 owns port readiness verification (line 17 of WP-06: "Port readiness checks with timeout"). WP-04 references it as if it's locally defined.
- **Fix:** Either (a) move the call to WP-06 and have WP-04 call WP-06's helper, (b) define `waitForPorts` here as a private method that uses `fetch(\`http://localhost:\${port}\`)` with a timeout, or (c) add a TODO referencing WP-06. The cleanest contract: WP-06 exports a `waitForPort(port, timeoutMs): Promise<void>` helper that WP-04 imports.

### H9. **`Container.exec()` API misuse — `output()` returns synchronously?**
- **Location:** `execClw` (lines 81-100)
- **Code:** `const output = await execProcess.output();`
- **Problem:** Per `@cloudflare/containers` SDK, `exec()` returns `ExecProcess` (a class), and `output()` is async and resolves to `{stdout: ArrayBuffer, stderr: ArrayBuffer, exitCode: number}`. The signature in WP-04 is correct (`await execProcess.output()`). BUT: `stdout: capture ? "pipe" : "ignore"` is correct; however, when `capture === false`, `output()` may resolve with empty `ArrayBuffer` or `undefined` — and `new TextDecoder().decode(undefined)` throws. The WP-04 code DOES guard with `capture ? ... : ""` (lines 96-97), so this is OK.
- **Verdict:** Mostly OK; leave the `output()` API call but add a JSDoc comment that the Cloudflare Container exec API requires a `cmd: string[]` and a `timeoutMs` (in milliseconds), and the output's `stdout`/`stderr` are `ArrayBuffer | null` when not captured (handle null defensively — `output.stdout ?? new ArrayBuffer(0)`).

### H10. **Concurrent snapshot lock has no per-DO uniqueness test in DoD**
- **Location:** Section 4 (DoD #6)
- **Problem:** "Parallel `snapshot()` RPC calls → second fails with `SNAPSHOT_IN_PROGRESS`" — this is the test claim. But:
  1. The lock check is `if (this.state.snapshotInProgress) throw`. In a single-threaded DO, two parallel RPC calls are SERIALIZED by the DO runtime — the second one only runs AFTER the first completes. So the lock is only meaningful if `onStart`/`onStop`/`onError`/manual `snapshot` all run concurrently — which the DO runtime also serializes. The lock is effectively a NO-OP for the steady state.
  2. The lock DOES add value if `onStop` is in progress (DO is awaiting a long snapshot) and a manual `snapshot` RPC fires — but even then, the second waits for the first, so the second sees `snapshotInProgress=true` and throws correctly.
  3. The DoD claim "parallel calls" is misleading — they are SERIAL, not parallel.
- **Fix:** Reword DoD #6: "Sequential `snapshot()` RPC calls during an active snapshot → second fails with `SNAPSHOT_IN_PROGRESS`". Or: test it as a test of `acquireSnapshotLock` in isolation (unit test the helper directly).

### H11. **`snapshot()` RPC on `force: false` is undefined**
- **Location:** `snapshot` RPC (line 303)
- **Code:** `async snapshot(payload: { force: boolean })` — `payload.force` is NEVER USED.
- **Problem:** The RPC accepts `force: boolean` but always calls `snapshotViaClw(... "--force", ...)`. So `force: false` is silently ignored. Either the RPC is supposed to pass `--force` conditionally (matching clw's `--force` semantics — re-snapshot of identical content is already a no-op without force), or the `force` flag should be removed.
- **Fix:** Thread `payload.force` into `snapshotViaClw(dir, name, { force: payload.force })` and only add `--force` when truthy. The default (`--force=true`) is fine for the `onStop`/`onError` paths, but a manual `snapshot({force: false})` should NOT pass `--force` (so a no-op-skip re-snapshot remains a no-op).

---

## 🟡 MEDIUM SEVERITY ISSUES

### M1. **`SnapshotMetadata.root: ""` fallback is a footgun**
- **Location:** `hydrateViaClw` (line 124), `snapshotViaClw` (line 155)
- **Code:** `return { root: "", bytesTotal: 0, ...timestamp: Date.now() };`
- **Problem:** Empty-string `root` is indistinguishable from a real empty root. Downstream code that hashes `state.lastProfileSnapshot.root` would silently produce a bogus digest.
- **Fix:** Use `root: null` (or make `SnapshotMetadata.root: string | null`). When `root` is null, downstream observability knows the parse failed.

### M2. **`--json` is global, not a snapshot/hydrate flag — placement is fragile**
- **Location:** `hydrateViaClw` (line 108), `snapshotViaClw` (line 142)
- **Code:** `--json` appears after `--concurrency`
- **Problem:** clap's `global = true` accepts it anywhere, but it MUST come AFTER the subcommand (`snapshot` / `hydrate`). Per clw's CLI surface, `clw snapshot <dir> --name X --json --ref-domain runner --concurrency 8` works, but `clw --json snapshot <dir> ...` also works. The WP-04 ordering is fine; the META-ISSUE is that there is no test for the exact arg order — clap is permissive, so it works, but a future clw release that makes `--json` subcommand-scoped would silently break WP-04.
- **Fix:** Add a test that locks the exact arg vector: `assert.deepEqual(args, ["/usr/local/bin/clw", "snapshot", "/data/chrome", "--name", "X", "--force", "--ref-domain", "runner", "--concurrency", "8", "--json"])`.

### M3. **No JSDoc on lifecycle methods — undocumented contract**
- **Location:** All three lifecycle hooks
- **Problem:** `onStart`/`onStop`/`onError` are short comments but no JSDoc specifying pre/post conditions, idempotency, throwing behavior. WP-01 has them as `override async onStart(): Promise<void> { /* NOT_IMPLEMENTED */ }`.
- **Fix:** Add JSDoc explaining: "onStart: invoked by Container runtime after envVars injected. Must complete hydrate before supervisord's exec output is consumed. May throw — Container marks DO as errored."

### M4. **`Promise.all` in `snapshot()` RPC fails fast on first error**
- **Location:** `snapshot` RPC (line 306-309)
- **Code:** `await Promise.all([this.snapshotViaClw("/data/chrome", ...), this.snapshotViaClw("/data/workspace", ...)])`
- **Problem:** Unlike `onError` (which uses `Promise.allSettled`), `snapshot()` uses `Promise.all` — one failure aborts the other. The other half-finished snapshot is left running (clw keeps going server-side), but the RPC errors out, leaving the user with a half-snapshotted state.
- **Fix:** Use `Promise.allSettled` and report a partial-success response: `{ok: true, partialFailure: "profile" | "workspace" | null}`. Or document that `snapshot()` is all-or-nothing and the `onError`-style `allSettled` is intentionally reserved for crash paths.

### M5. **No `await` on `Promise.allSettled` result assignment — code path works but is non-idiomatic**
- **Location:** `onError` (line 278)
- **Problem:** `await Promise.allSettled([...])` is correct. The destructuring is fine. No bug. Just inconsistent style with the manual-snapshot path.

### M6. **`recordUsage` reads `this.env.BILLING_INGEST_URL` — env not declared in WP-01's `Env`**
- **Location:** `recordUsage` (line 342)
- **Problem:** WP-01's `Env` interface is referenced but not exported in WP-04. The new env vars `BILLING_INGEST_URL` and `BILLING_INGEST_AUTH_KEY` are not declared in WP-08's wrangler config either.
- **Fix:** Declare both in WP-01's `Env` (or in a shared `src/env.ts` that WP-01/WP-04/WP-07 all import). Update WP-08's wrangler example to add the bindings.

---

## 📋 DoD Gap Analysis

The DoD has 10 items. Checking each against the actual clw binary + WP-01 shape:

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | onStart calls clw hydrate for /data/chrome and /data/workspace | ❌ **FAIL** | `this.envVars.PROFILE_NAME` does not exist (B1); `--ref-domain` hard-coded twice (B5); clw hydrate JSON has no chunks_total/chunks_uploaded (B3) |
| 2 | onStart handles clw exit code 2 (no snapshot) as non-fatal | ❌ **FAIL** | clw exit 2 is generic-error, not "not found" (B6). The mapping is wrong. |
| 3 | onStop calls clw snapshot --force for both dirs | ⚠️ **PARTIAL** | Compiles only after B1 fix; the `this.envVars` issue makes it not compile against WP-01 |
| 4 | onStop persists snapshot metadata to DO state | ⚠️ **PARTIAL** | Bypasses transitionState (B7); the spread-mutation pattern doesn't satisfy the discriminated-union type |
| 5 | onError attempts emergency snapshot before dying | ⚠️ **PARTIAL** | Promise.allSettled used (good) but result handling buggy (H7); same transitionState issue |
| 6 | Snapshot lock prevents concurrent snapshots | ❌ **FAIL** | DO serializes calls — lock is a NO-OP except in the specific onStop+manual-snapshot case (H10) |
| 7 | recordUsage called on onStop and onError | ❌ **FAIL** | Billing contract is WP-07's (H3); math is wrong (H4); env not declared (M6) |
| 8 | JSON output parsing works for clw --json | ❌ **FAIL** | hydrate JSON shape mismatch (B3); no test |
| 9 | Snapshot timeout (10 min) prevents hanging | ⚠️ **PARTIAL** | timeoutMs=600_000 is set; but exec API may have a lower max timeoutMs (verify Cloudflare Container SDK) |
| 10 | State persists across DO restarts | ⚠️ **PARTIAL** | persistState called, BUT snapshotInProgress lock can be left true after crash (B9) |

**DoD Score: 1/10 PASS, 5/10 PARTIAL, 4/10 FAIL**

---

## 📋 Invariants Verification

| Invariant | Enforced in Code? | Verdict |
|-----------|-------------------|---------|
| I1: snapshotInProgress = true only during active snapshot | ⚠️ In-memory only; no recovery on crash | **PARTIAL** (B9) |
| I2: lastProfileSnapshot / lastWorkspaceSnapshot only on success | ✅ Try/catch around snapshotViaClw | **ENFORCED** |
| I3: onStop and onError both call recordUsage exactly once | ⚠️ recordUsage is best-effort, may not actually fire | **PARTIAL** (H4) |
| I4: acquireSnapshotLock / releaseSnapshotLock always paired (try/finally) | ✅ Both use try/finally | **ENFORCED** |
| I5: clw hydrate exit code 2 never fails container start | ❌ Exit 2 is generic, not "not found" (B6) | **VIOLATED** |
| I6: clw snapshot exit code != 0 always throws | ✅ All non-zero exits throw | **ENFORCED** |
| I7: SnapshotMetadata includes root/bytesTotal/chunksTotal/chunksUploaded/timestamp | ❌ chunksTotal/chunksUploaded don't exist for hydrate (B3) | **VIOLATED** |

**Invariants Enforced: 3/7 (43%)** — INSUFFICIENT

---

## 📋 Quality Standards Verification

| Standard | Met? | Evidence |
|----------|------|----------|
| Error Handling: All clw calls wrapped; exit codes checked; stderr logged on failure | ⚠️ Partial — exit 2 is mishandled (B6) |
| Observability: Structured logging with [RunnerDevEnvDO] ISO8601 prefix | ✅ Pass — log() helper present |
| Concurrency: DO single-threaded + explicit lock = no race conditions | ❌ Lock is mostly a no-op (H10); crash-recovery missing (B9) |
| Idempotency: snapshot() RPC safe to call multiple times (lock prevents overlap) | ⚠️ force flag is ignored (H11) |
| Resource Bounds: 10-min timeout on clw ops | ✅ Pass — timeoutMs=600_000 set |
| Crash Safety: onError attempts snapshot even during crash | ⚠️ Partial — emergency snapshot path exists but transitionState bypassed (B7/H6) |

**Quality Standards: 2/6 MET, 4/6 PARTIAL** — INSUFFICIENT

---

## 📋 Self-Check Points Analysis

### Self-Check 1: clw Contract Compliance
- [ ] hydrate args: `[dir, "--name", name, "--ref-domain", "runner", "--concurrency", "8", "--json"]` — **PARTIAL**: clw JSON has no chunks_total/chunks_uploaded (B3); `--ref-domain` redundant when env set (B5)
- [ ] snapshot args: `[dir, "--name", name, "--force", "--ref-domain", "runner", "--concurrency", "8", "--json"]` — **PARTIAL**: same
- [ ] Exit codes: 0=success, 2=not_found (hydrate only), other=error — **FAIL**: exit 2 is generic (B6)
- [ ] JSON output parsed for root, bytes_total, chunks_total, chunks_uploaded — **FAIL**: hydrate has no chunks_total/chunks_uploaded (B3)

**Verdict: 0/4 PASS, 2/4 PARTIAL, 2/4 FAIL**

### Self-Check 2: Snapshot Lock Correctness
- [ ] snapshotInProgress stored in DO state — ✅
- [ ] acquireSnapshotLock throws if already in progress — ✅
- [ ] releaseSnapshotLock in finally block — ✅
- [ ] Manual snapshot() RPC also acquires lock — ✅
- [ ] onStop and onError both acquire lock — ✅
- [ ] (NEW: lock recovered on crash) — ❌ Missing (B9)

**Verdict: 5/6 PASS, 1/6 FAIL**

### Self-Check 3: Crash Recovery Semantics
- [ ] onError fires on container crash/OOM — ✅ (Container runtime contract)
- [ ] Emergency snapshot attempted for BOTH — ✅
- [ ] Promise.allSettled used — ✅
- [ ] Successful snapshots persisted to state — ⚠️ But the multi-mutation pattern risks inconsistency (H7)
- [ ] recordUsage called even on crash — ⚠️ Best-effort, may silently drop (H4)
- [ ] State shows status: "errored" with lastError — ⚠️ Direct mutation, not transitionState (B7/H6)

**Verdict: 3/6 PASS, 3/6 PARTIAL**

---

## 📊 SCORECARD

| Category | Score | Required | Gap |
|----------|-------|----------|-----|
| Blocking Issues | 9 | 0 | **-9** |
| High Issues | 11 | 0 | **-11** |
| Medium Issues | 6 | 0 | **-6** |
| DoD Pass Rate | 10% | 100% | **-90%** |
| Invariants Enforced | 43% | 100% | **-57%** |
| Quality Standards | 33% | 100% | **-67%** |
| Self-Check Pass | 50% | 100% | **-50%** |

**OVERALL VERDICT: ❌ FAIL — Requires major rework before sign-off**

---

## 🔧 FIX PRIORITY

### Must Fix (Blockers)
1. B1, B2 — Replace `this.envVars.X` with `this.state.X`; add `clwTenant` to state
2. B3 — Hydrate metadata has no chunks_total/chunks_uploaded — fix shape
3. B4 — `clw exec` doesn't exist — drop the mkdir line (WP-03 owns it)
4. B5 — Drop redundant `--ref-domain` (env-only) and `--concurrency` (use clw default 8)
5. B6 — Drop the `exit 2 → null` special case; pre-check with `clw ls` for first-run detection
6. B7 — Use `this.transitionState` everywhere; coordinate with WP-06's 7-state machine
7. B8 — Import canonical SnapshotRequest/SnapshotResponse types
8. B9 — Recover from stale `snapshotInProgress=true` on DO restart

### Should Fix (High)
1. H1 — Single concurrency source (env or constant)
2. H2 — Rename `containerHandle` → `devenvId` (or use real container handle)
3. H3 — Move `recordUsage` to WP-07 or import WP-07's helper
4. H4 — Fix `vcpuSeconds` math (operator precedence); declare vcpu count
5. H5 — Type-narrow the `startedAt` guard via state status
6. H6 — Use `transitionState` in onError with the errored arm shape
7. H7 — Single state mutation in onError (one transitionState call)
8. H8 — waitForPorts is WP-06's; import it
9. H9 — JSDoc the exec API; handle null stdout/stderr defensively
10. H10 — Reword DoD #6 (sequential, not parallel)
11. H11 — Thread `payload.force` into snapshotViaClw

### Nice to Fix (Medium)
1. M1 — Use `root: null` instead of `root: ""` for parse-failure fallback
2. M2 — Lock the arg vector in a test
3. M3 — JSDoc on lifecycle methods
4. M4 — Use `Promise.allSettled` in `snapshot()` RPC for partial-success semantics
5. M5 — Style consistency
6. M6 — Declare billing env vars in `Env` interface (cross-WP)

---

## NEXT STEPS

1. **Apply all BLOCKING fixes** (B1-B9) — non-negotiable
2. **Apply all HIGH fixes** (H1-H11) — required for quality
3. **Apply at least 50% of MEDIUM fixes** (M1, M2, M3 minimum)
4. **Re-verify all 10 DoD items pass** (post-fix)
5. **Re-verify all 7 invariants enforced in code**
6. **Re-verify all 6 quality standards met**
7. **Coordinate cross-WP:**
   - WP-01: extend `DevenvState` with `clwTenant`
   - WP-06: export `waitForPort`, `transitionState` (the 7-state version)
   - WP-07: own `recordUsage`; export `postUsage({tenantId, vcpuSeconds, periodStart, periodEnd})`
   - WP-08: declare `BILLING_INGEST_URL` / `BILLING_INGEST_AUTH_KEY` in `Env`; declare `VCPU_COUNT` in wrangler config
8. **Proceed to Iteration 2 review**

**Do NOT proceed to WP-05 until WP-04 is fixed and passes review.**

---

## CROSS-WP COORDINATION FINDINGS

1. **State machine authority is ambiguous**: WP-01 has a 5-state machine, WP-06 has a 7-state machine, WP-04 has neither. ONE WPs must own the canonical state machine and the others must import it. Recommendation: **WP-06 owns the canonical 7-state machine**; WP-04 imports `transitionState` and `waitForPort` from WP-06's exports.
2. **Billing is owned by WP-07**: WP-04 must NOT define `recordUsage` body — it should call `recordUsageForDevEnv(state)` exported by WP-07. The endpoint, headers, and request shape are WP-07's contract.
3. **`DevenvState` extension is a cross-WP API change**: adding `clwTenant` is required for WP-04 to post billing. Either WP-01 takes the addition as an Iter 5 patch, or the addition is moved to a shared `src/types/devenv.ts` (recommended) that all WPs import.
4. **clw binary `--ref-domain` / `--concurrency` / `--json` are global flags**: they work because clap's `global = true` accepts them anywhere after the subcommand. WP-04's arg ordering is correct. Document this in a shared "clw CLI invocation contract" note for future WPs.

---

**END OF WP-04 ITERATION 1 REVIEW**
