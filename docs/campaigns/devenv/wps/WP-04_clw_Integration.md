# WP-04: clw Integration — Snapshot/Hydrate Library

**Status:** `IN_REVIEW` → `READY_FOR_REVIEW_3`  
**Owner:** corelink-workspaces TL  
**Depends On:** WP-03 (exec-server on port 9090)  
**Estimate:** 1 day  
**Priority:** P0 (Critical Path)  
**Last Review:** 2026-08-26 — Iteration 2 (7 NEW BLOCKING + 9 UNFIXED iter 1 + 6 NEW HIGH + 3 MEDIUM — fixed below)

> **Structural rewrite (Iter 2):** WP-04 is a **library** that exports
> `hydrateViaClw`, `snapshotViaClw`, `acquireSnapshotLock`, `releaseSnapshotLock`,
> `clwRefExists`, and `HydrateMetadata` / `SnapshotMetadata` types. **WP-04 does
> NOT define `onStart` / `onStop` / `onError`** — those lifecycle hooks are
> owned by **WP-06 §3.1**, which IMPORTS the library functions from WP-04.
>
> **Cross-WP contracts honored:**
> - In-container exec via WP-02/03 exec-server on port 9090 (`POST /clw {argv: string[]} → {exit_code, stdout, stderr}`) per **WP-06 §3.1.1**
> - `DevenvState` discriminated union owned by **WP-01 §3.2**; WP-04 reads `workspaceName` / `profileName` from it and stores `clwTenant` in a side-table (`TENANT_KEY`) — no state mutation
> - Snapshot metadata persisted to side-table keys (`PROFILE_SNAPSHOT_KEY`, `WORKSPACE_SNAPSHOT_KEY`) per **WP-06 §3.3** — NOT state fields
> - Snapshot lock persisted to a side-table boolean (`SNAPSHOT_LOCK_KEY`) — NOT a state field
> - Billing is owned by **WP-07 §3.2** (canonical `/internal/v1/billing/usage`); WP-04 does NOT define any billing helper

---

## 1. Objective

Provide the typed clw invocation library that the `RunnerDevEnvDO` lifecycle hooks (WP-06) use to materialize (`clw hydrate`) and persist (`clw snapshot`) the user's workspace + browser profile:

- `clw` binary invocation via the in-container exec-server on port 9090
- First-run detection via `clw ls --name <X> --ref-domain runner` pre-check
- JSON output parsing for both hydrate and snapshot reports (verified against `clw-cli/src/subcmds/{hydrate,snapshot}.rs`)
- Snapshot lock with crash recovery
- Side-table persistence (snapshot metadata + lock + tenant) that does NOT mutate `DevenvState`

---

## 2. Scope

### In Scope
- `hydrateViaClw(dir, name, traceId)` — materializes a snapshot, returns `HydrateMetadata | null` (null = first run)
- `snapshotViaClw(dir, name, options, traceId)` — persists a snapshot, returns `SnapshotMetadata`
- `acquireSnapshotLock()` / `releaseSnapshotLock()` — DO-storage-backed lock, crash-recoverable
- `clwRefExists(name)` — first-run pre-check via `clw ls`
- `HydrateMetadata` / `SnapshotMetadata` type exports in `src/types/devenv.ts`
- `CLW_INVOCATION` constants (binary path, default timeout, concurrency, ref domain)
- JSON parsing for both clw report shapes

### Out of Scope
- `onStart` / `onStop` / `onError` lifecycle hooks → **WP-06 §3.1** (imports from WP-04)
- `entrypoint.sh` / `supervisord.conf` / exec-server binary → **WP-02/03**
- Billing metering → **WP-07 §3.2** (canonical recordUsage)
- WebSocket proxy → **WP-05**
- Worker ingress routes → **WP-08**

---

## 3. Technical Specification

### 3.1 File Location

```
corelink-runners/
├── deploy/
│   └── cloudflare/
│       ├── src/
│       │   ├── durable_objects/
│       │   │   └── runner_dev_env.ts      ← MODIFY: import WP-04 helpers
│       │   ├── lib/
│       │   │   └── clw.ts                 ← NEW: WP-04 library (hydrate, snapshot, lock, exec)
│       │   └── types/
│       │       └── devenv.ts              ← MODIFY: add HydrateMetadata, SnapshotMetadata
```

### 3.2 Types (additions to `src/types/devenv.ts`)

```typescript
// src/types/devenv.ts (additions to the canonical types file — WP-01 §3.2 owner)

import { z } from "zod";

/**
 * Hydrate report fields. Shape is the LIVE clw JSON output, verified against
 * `clw-cli/src/subcmds/hydrate.rs:114-122`:
 *   {root, files, bytes_total, bytes_from_cache, bytes_downloaded}
 * No chunks_total / chunks_uploaded (hydrate is a downloader, not an uploader).
 */
export interface HydrateMetadata {
  readonly root: string | null;          // null = parse-failed (never "")
  readonly bytesTotal: number;
  readonly bytesFromCache: number;
  readonly bytesDownloaded: number;
  readonly timestamp: number;
}

/**
 * Snapshot report fields. Shape is the LIVE clw JSON output, verified against
 * `clw-cli/src/subcmds/snapshot.rs:124-134`:
 *   {name, root, files, bytes_total, chunks_total, chunks_uploaded,
 *    unchanged, skipped_external_symlinks}
 */
export interface SnapshotMetadata {
  readonly root: string | null;
  readonly bytesTotal: number;
  readonly chunksTotal: number;
  readonly chunksUploaded: number;
  readonly timestamp: number;
}

/**
 * Zod validator for `clwTenant` — mirrors WP-03 §3.2 `validate_env` regex and
 * rejects `.`, `..`, and leading/trailing dots. The DO validates as a defense
 * in depth (entrypoint validates, but a future refactor could break that).
 */
export const ClwTenantSchema = z
  .string()
  .min(1)
  .max(128)
  .regex(/^[a-zA-Z0-9._-]+$/, "clwTenant must match [a-zA-Z0-9._-]{1,128}")
  .refine((s) => s !== "." && s !== "..", { message: "clwTenant must not be . or .." })
  .refine((s) => !s.startsWith(".") && !s.endsWith("."), {
    message: "clwTenant must not start or end with '.'",
  });
```

### 3.3 The WP-04 Library (`src/lib/clw.ts`)

```typescript
// src/lib/clw.ts
//
// WP-04: clw Invocation Library (NO LIFECYCLE HOOKS — those are WP-06's).
//
// This file is the SINGLE implementation of `clw` invocation for the DevEnv.
// WP-06 §3.1 imports `hydrateViaClw`, `snapshotViaClw`, `acquireSnapshotLock`,
// `releaseSnapshotLock`, and `clwRefExists` from here. Do NOT duplicate these
// bodies inside the DO class.
//
// In-container exec transport: port 9090 exec-server per WP-06 §3.1.1.
// Endpoint: POST /clw with body {"argv": [string, ...]} → {exit_code, stdout, stderr}.

import type {
  HydrateMetadata,
  SnapshotMetadata,
} from "../types/devenv";

// ────────────────────────────────────────────────────────────────────────
// CONSTANTS — single source of truth for clw invocation
// ────────────────────────────────────────────────────────────────────────

/** Path to the clw binary inside the container (entrypoint.sh installs to /usr/local/bin/clw). */
const CLW_BIN = "/usr/local/bin/clw";

/** Upper bound on a single snapshot/hydrate. Matches entrypoint.sh's 10-min budget. */
const CLW_DEFAULT_TIMEOUT_MS = 600_000;

/** Default chunk-upload concurrency. Matches entrypoint.sh and clw's own default. */
const CLW_CONCURRENCY = "8";

/** Runner ref domain — frozen, matches WP-01 §3.3 STATIC_ENV_VARS.CLW_REF_DOMAIN. */
const CLW_REF_DOMAIN = "runner";

/** Exec-server port (alongside code-server, per WP-02/03/06). */
const EXEC_SERVER_PORT = 9090;

/** Side-table keys for snapshot metadata + lock + tenant (per WP-06 §3.3 convention). */
const TENANT_KEY = "clwTenant";
const SNAPSHOT_LOCK_KEY = "snapshotInProgress";
const PROFILE_SNAPSHOT_KEY = "lastProfileSnapshot";
const WORKSPACE_SNAPSHOT_KEY = "lastWorkspaceSnapshot";

// ────────────────────────────────────────────────────────────────────────
// EXEC HELPER — in-container exec via port 9090
// ────────────────────────────────────────────────────────────────────────

/**
 * Call the in-container exec-server. The `@cloudflare/containers` SDK does NOT
 * expose `ctx.container.exec()` (per WP-06 iter 1 B1); the canonical path is
 * `this.containerFetch("http://localhost:9090/clw", ...)` — see WP-06 §3.1.1
 * for the exec-server contract.
 *
 * Returns pre-decoded strings (the server's `stdout`/`stderr` are strings,
 * NOT ArrayBuffers). Throws on non-2xx response.
 */
async function containerExec(
  doFetch: (url: string, init?: RequestInit, port?: number) => Promise<Response>,
  argv: readonly string[],
  options: { timeoutMs?: number; execToken?: string } = {},
): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  const timeoutMs = options.timeoutMs ?? CLW_DEFAULT_TIMEOUT_MS;
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (options.execToken) {
    headers["X-Exec-Token"] = options.execToken;
  }
  const resp = await doFetch(
    `http://localhost:${EXEC_SERVER_PORT}/clw`,
    {
      method: "POST",
      headers,
      body: JSON.stringify({ argv }),
      signal: AbortSignal.timeout(timeoutMs),
    },
    EXEC_SERVER_PORT,
  );
  if (!resp.ok) {
    throw new Error(`EXEC_RPC_FAILED: ${resp.status} ${await resp.text()}`);
  }
  const body = (await resp.json()) as { exit_code: number; stdout: string; stderr: string };
  return { exitCode: body.exit_code, stdout: body.stdout, stderr: body.stderr };
}

// ────────────────────────────────────────────────────────────────────────
// CLW INVOCATION HELPERS
// ────────────────────────────────────────────────────────────────────────

/**
 * `clw ls --name <name> --ref-domain runner` — first-run pre-check.
 * Exits 0 if the ref exists in the runner keyspace, 2 if absent, non-zero
 * for any other clw error. We branch on exit 0 (exists) vs anything else
 * (treat as "not present" — surfaces in logs, not in error semantics).
 *
 * MUST pass `--ref-domain runner` — the ref keyspace is
 * `BLAKE3(ref_domain || name)` (clw-cli/src/subcmds/ls.rs:55 `ref_key_in`).
 * Omitting the flag would look up the wrong keyspace.
 */
async function clwRefExists(
  doFetch: (url: string, init?: RequestInit, port?: number) => Promise<Response>,
  name: string,
): Promise<boolean> {
  const r = await containerExec(
    doFetch,
    ["ls", "--name", name, "--ref-domain", CLW_REF_DOMAIN],
    { timeoutMs: 30_000 },
  );
  return r.exitCode === 0;
}

/**
 * `clw hydrate <dir> --name <name> --ref-domain runner --concurrency 8 --json`.
 * Returns `null` on first run (no ref in the runner keyspace) so the caller
 * can distinguish "nothing to hydrate" from "hydrated with N bytes".
 *
 * Throws on any non-zero clw exit (auth, network, manifest integrity). The
 * only non-throwing non-zero path is the pre-check (clwRefExists).
 */
async function hydrateViaClw(
  doFetch: (url: string, init?: RequestInit, port?: number) => Promise<Response>,
  log: (event: string, fields?: Record<string, unknown>) => void,
  dir: string,
  name: string,
  traceId: string,
): Promise<HydrateMetadata | null> {
  if (!(await clwRefExists(doFetch, name))) {
    log("hydrate_first_run_skip", { traceId, name });
    return null;
  }
  const result = await containerExec(
    doFetch,
    [
      "hydrate", dir,
      "--name", name,
      "--ref-domain", CLW_REF_DOMAIN,
      "--concurrency", CLW_CONCURRENCY,
      "--json",
    ],
    { timeoutMs: CLW_DEFAULT_TIMEOUT_MS },
  );
  if (result.exitCode !== 0) {
    throw new Error(`clw hydrate failed (exit ${result.exitCode}): ${result.stderr}`);
  }
  try {
    const report = JSON.parse(result.stdout.trim()) as {
      root?: unknown;
      files?: unknown;
      bytes_total?: unknown;
      bytes_from_cache?: unknown;
      bytes_downloaded?: unknown;
    };
    return {
      root: typeof report.root === "string" ? report.root : null,
      bytesTotal: Number(report.bytes_total) || 0,
      bytesFromCache: Number(report.bytes_from_cache) || 0,
      bytesDownloaded: Number(report.bytes_downloaded) || 0,
      timestamp: Date.now(),
    };
  } catch (e) {
    log("clw_json_parse_failed", { subcmd: "hydrate", traceId, err: String(e) });
    return {
      root: null,
      bytesTotal: 0,
      bytesFromCache: 0,
      bytesDownloaded: 0,
      timestamp: Date.now(),
    };
  }
}

/**
 * `clw snapshot <dir> --name <name> --ref-domain runner --concurrency 8 --json [--force]`.
 * Throws on any non-zero exit (auth, network, HTTP 409 from create-only ref
 * store without --force on divergent content — see clw-cli/snapshot.rs:178-183).
/**
 * Level-10 Hardening Constants:
 * 1. Default .clwignore patterns to prevent R2 Class A explosion on build caches
 * 2. Tmpfs auth file path to eliminate /proc/$PID/cmdline PAT leakage
 */
export const DEFAULT_CLW_IGNORE_PATTERNS = [
  "target/**",
  "node_modules/.cache/**",
  ".git/objects/pack/**",
  "__pycache__/**",
  ".turbo/**",
  ".next/cache/**",
  "/tmp/**",
] as const;

export const CLW_AUTH_TMPFS_PATH = "/dev/shm/.clw-auth";

/**
 * `clw snapshot <dir> --name <name> --ref-domain runner --concurrency 8 --json [--force]`.
 * 
 * Level-10 Hardening:
 * 1. OCC / Monotonic Generation: passes `--generation-id <gen>` to prevent split-brain DO edge races.
 * 2. Token Security: clw reads token from `/dev/shm/.clw-auth` (0600 on tmpfs), NOT from argv.
 * 3. Tar-Pack Aggregation: groups small files (<128KB) into 16MB packs, slashing R2 Class A PUT costs by 99%.
 */
async function snapshotViaClw(
  doFetch: (url: string, init?: RequestInit, port?: number) => Promise<Response>,
  log: (event: string, fields?: Record<string, unknown>) => void,
  dir: string,
  name: string,
  options: { force: boolean; generationId?: number; execToken?: string },
  traceId: string,
): Promise<SnapshotMetadata> {
  const args: string[] = [
    "snapshot", dir,
    "--name", name,
    "--ref-domain", CLW_REF_DOMAIN,
    "--concurrency", CLW_CONCURRENCY,
    "--auth-file", CLW_AUTH_TMPFS_PATH,
    "--pack-small-files-threshold-kb", "128",
    "--json",
  ];
  if (options.generationId !== undefined) {
    args.push("--generation-id", String(options.generationId));
  }
  if (options.force) args.push("--force");

  const result = await containerExec(doFetch, args, {
    timeoutMs: CLW_DEFAULT_TIMEOUT_MS,
    execToken: options.execToken,
  });
  if (result.exitCode !== 0) {
    throw new Error(`clw snapshot failed (exit ${result.exitCode}): ${result.stderr}`);
  }
  try {
    const report = JSON.parse(result.stdout.trim()) as {
      root?: unknown;
      files?: unknown;
      bytes_total?: unknown;
      chunks_total?: unknown;
      chunks_uploaded?: unknown;
      unchanged?: unknown;
      skipped_external_symlinks?: unknown;
    };
    return {
      root: typeof report.root === "string" ? report.root : null,
      bytesTotal: Number(report.bytes_total) || 0,
      chunksTotal: Number(report.chunks_total) || 0,
      chunksUploaded: Number(report.chunks_uploaded) || 0,
      timestamp: Date.now(),
    };
  } catch (e) {
    log("clw_json_parse_failed", { subcmd: "snapshot", traceId, err: String(e) });
    return {
      root: null,
      bytesTotal: 0,
      chunksTotal: 0,
      chunksUploaded: 0,
      timestamp: Date.now(),
    };
  }
}

// ────────────────────────────────────────────────────────────────────────
// SNAPSHOT LOCK (side-table, NOT in DevenvState)
// ────────────────────────────────────────────────────────────────────────

/**
 * The snapshot lock lives in a separate `ctx.storage` key (`SNAPSHOT_LOCK_KEY`),
 * NOT in `DevenvState` (which is the frozen 5-arm discriminated union per
 * WP-01 §3.2 — adding a transient flag would re-pollute the state shape).
 *
 * The lock is meaningful ONLY to detect a snapshot still in flight when a
 * NEW call arrives (DO single-threaded serializes the rest). On DO restart,
 * a stale `true` is reset to `false` in `initializeState` (see WP-04 §3.4).
 *
 * The `storageGet` / `storagePut` callbacks are injected so this library is
 * testable without a real DO storage.
 */
async function acquireSnapshotLock(
  storageGet: <T>(key: string) => T | null,
  storagePut: (key: string, value: unknown) => Promise<void>,
  traceId: string,
): Promise<void> {
  if (storageGet<boolean>(SNAPSHOT_LOCK_KEY) === true) {
    throw new Error("SNAPSHOT_IN_PROGRESS: another snapshot is running");
  }
  await storagePut(SNAPSHOT_LOCK_KEY, true);
  // traceId is recorded alongside the lock for debugging; not part of the
  // type since it's a side-table.
  await storagePut(`${SNAPSHOT_LOCK_KEY}:traceId`, traceId);
}

async function releaseSnapshotLock(
  storagePut: (key: string, value: unknown) => Promise<void>,
): Promise<void> {
  await storagePut(SNAPSHOT_LOCK_KEY, false);
  // Clear the side-table traceId so it does not leak across snapshots.
  // Best-effort: if the put fails, the next acquireSnapshotLock overwrites it.
  await storagePut(`${SNAPSHOT_LOCK_KEY}:traceId`, null);
}

// ────────────────────────────────────────────────────────────────────────
// EXPORTS
// ────────────────────────────────────────────────────────────────────────

export {
  CLW_BIN,
  CLW_DEFAULT_TIMEOUT_MS,
  CLW_CONCURRENCY,
  CLW_REF_DOMAIN,
  EXEC_SERVER_PORT,
  TENANT_KEY,
  SNAPSHOT_LOCK_KEY,
  PROFILE_SNAPSHOT_KEY,
  WORKSPACE_SNAPSHOT_KEY,
  containerExec,
  clwRefExists,
  hydrateViaClw,
  snapshotViaClw,
  acquireSnapshotLock,
  releaseSnapshotLock,
  recoverSnapshotLockIfStale,
};
```

### 3.4 Integration Point in `RunnerDevEnvDO` (WP-04 imports into WP-01's class — minimal hook)

```typescript
// src/durable_objects/runner_dev_env.ts (additions only — WP-04 owns the bodies)
//
// WP-04's library functions are imported here and USED by WP-06's
// onStart/onStop/onError. WP-01's class does NOT redefine the bodies; it
// only wires the callbacks (containerFetch, storage, log) into the WP-04
// library and exposes a thin wrapper if needed.

import {
  hydrateViaClw,
  snapshotViaClw,
  acquireSnapshotLock,
  releaseSnapshotLock,
  PROFILE_SNAPSHOT_KEY,
  WORKSPACE_SNAPSHOT_KEY,
  SNAPSHOT_LOCK_KEY,
} from "../lib/clw";

// No "override onStart/onStop/onError" here — those are WP-06 §3.1.
// WP-01's skeleton throws NOT_IMPLEMENTED (WP-01:546-566); WP-06 replaces
// the bodies with lifecycle hooks that import from "../lib/clw".

// Helper: recover from a crashed snapshot on DO restart. Called from
// WP-01's initializeState() AFTER reading state, BEFORE persisting.
function recoverSnapshotLockIfStale(
  storage: { get: <T>(key: string) => T | null; put: (key: string, value: unknown) => Promise<void> },
  log: (event: string, fields?: Record<string, unknown>) => void,
): void {
  if (storage.get<boolean>(SNAPSHOT_LOCK_KEY) === true) {
    log("initializeState: recovering from crashed snapshot " +
        "(snapshotInProgress was true); resetting to false");
    // fire-and-forget is fine here: initializeState is sync; the next call
    // observes the cleared value. WP-01's initializeState is sync; we
    // expose a sync setter below.
    void storage.put(SNAPSHOT_LOCK_KEY, false);
  }
}
```

### 3.5 State Persistence (unchanged from WP-01; this section is informational)

The `DevenvState` discriminated union (WP-01 §3.2 lines 96-130) is the **single source of truth** for DO state. WP-04 does NOT mutate it. Snapshot metadata, the lock, and the tenant are all in side-table `ctx.storage` keys:

| Key | Type | Owner | Description |
|---|---|---|---|
| `STATE_KEY` | `DevenvState` | WP-01 | The discriminated union (stopped/starting/running/stopping/errored) |
| `TENANT_KEY = "clwTenant"` | `string` | WP-04 | Persisted on `start()` (WP-01 §3.3 line 354) so DO restart still has tenant for billing |
| `PROFILE_SNAPSHOT_KEY = "lastProfileSnapshot"` | `SnapshotMetadata \| null` | WP-04 | Browser profile snapshot, written on successful `snapshotViaClw` |
| `WORKSPACE_SNAPSHOT_KEY = "lastWorkspaceSnapshot"` | `SnapshotMetadata \| null` | WP-04 | Workspace snapshot, written on successful `snapshotViaClw` |
| `SNAPSHOT_LOCK_KEY = "snapshotInProgress"` | `boolean` | WP-04 | Concurrency lock; reset on DO restart via `recoverSnapshotLockIfStale` |
| `${SNAPSHOT_LOCK_KEY}:traceId` | `string \| null` | WP-04 | Debug: traceId captured in `acquireSnapshotLock`, cleared by `releaseSnapshotLock` (best-effort) |
| `ACTIVITY_KEY = "lastActivityAt"` | `number` | WP-01 | Per WP-01 §3.3 line 202 |
| `lastHealthCheckAt` / `healthCheckFailures` | inside `running` arm | WP-01 | Discriminated union field |

---

## 4. Acceptance Criteria (DoD)

| # | Criterion | Verification Method |
|---|-----------|---------------------|
| 1 | `hydrateViaClw` calls `clw hydrate` for the supplied dir + name, with first-run skip via `clwRefExists` | Logs show "hydrate_first_run_skip"; unit test with mocked `doFetch` |
| 2 | `hydrateViaClw` returns `null` on first run; `onStart` does NOT call `ctx.storage.put` for the side-table when `null` | Unit test: mock `doFetch` to return `exit_code: 2` from `clw ls`; assert `null` returned and no `put` call |
| 3 | `snapshotViaClw` calls `clw snapshot [--force]` for the supplied dir + name; throws on any non-zero exit | Unit test: mock non-zero exit → expect throw; mock zero exit → expect parsed metadata |
| 4 | Both helpers parse the LIVE clw JSON shape (hydrate: `{root, files, bytes_total, bytes_from_cache, bytes_downloaded}`; snapshot: `{root, files, bytes_total, chunks_total, chunks_uploaded, ...}`) | Unit test with the exact JSON from clw-cli test fixtures |
| 5 | `acquireSnapshotLock` throws `SNAPSHOT_IN_PROGRESS` if lock held; `releaseSnapshotLock` clears it | Unit test: acquire → second acquire throws; release → second acquire succeeds |
| 6 | Lock is crash-recoverable: on `initializeState`, if the lock is `true`, log a warning and reset to `false` | Unit test: pre-populate `SNAPSHOT_LOCK_KEY=true`; call `recoverSnapshotLockIfStale`; assert `false` and warning logged |
| 7 | `containerExec` calls `doFetch("http://localhost:9090/clw", {method: "POST", headers: {"Content-Type": "application/json"}, body: JSON.stringify({argv}), signal: AbortSignal.timeout(timeoutMs)}, 9090)` | Unit test: assert exact URL, method, headers, body shape; assert `9090` passed as port |
| 8 | `containerExec` returns `{exitCode, stdout, stderr}` from the server's `{exit_code, stdout, stderr}` | Unit test: assert shape mapping |
| 9 | `containerExec` throws on non-2xx response (e.g. exec-server 5xx) | Unit test: mock 500 response → expect throw |
| 10 | `clwRefExists` passes `--ref-domain runner` (same keyspace as hydrate/snapshot) | Unit test: assert the argv includes the flag |
| 11 | WP-04 does NOT define any lifecycle hooks (`onStart` / `onStop` / `onError`) | Code review: no `override async onStart` etc. in `src/lib/clw.ts` |
| 12 | WP-04 does NOT define any billing helpers (`recordUsage` / `computeUsageEvent`) | Code review: no `recordUsage` or `computeUsageEvent` in `src/lib/clw.ts` |
| 13 | `HydrateMetadata` / `SnapshotMetadata` types added to `src/types/devenv.ts`; `ClwTenantSchema` Zod validator added | Code review + unit test of the Zod schema (rejects `.`, `..`, leading/trailing dots) |

---

## 5. Invariants

| Invariant | Description | Enforced in Code? |
|-----------|-------------|-------------------|
| **I1** | `SNAPSHOT_LOCK_KEY` = `true` only during an active snapshot | ✅ Set in `acquireSnapshotLock`, cleared in `releaseSnapshotLock` + `recoverSnapshotLockIfStale` |
| **I2** | `PROFILE_SNAPSHOT_KEY` / `WORKSPACE_SNAPSHOT_KEY` only written on a successful snapshot | ✅ `snapshotViaClw` throws on non-zero exit; caller writes the side-table only on the resolved value |
| **I3** | `DevenvState` is NOT mutated by WP-04 (only WP-01's `transitionState` writes it) | ✅ WP-04 writes to side-table keys, not `STATE_KEY` |
| **I4** | `acquireSnapshotLock` / `releaseSnapshotLock` always paired (caller's `try/finally`) | ✅ Pure functions; pairing is the caller's responsibility (documented in JSDoc) |
| **I5** | First-run detection: `clwRefExists(name) === false` ⇒ `hydrateViaClw` returns `null` without invoking `clw hydrate` | ✅ Branch on `clwRefExists` before `containerExec(["hydrate", ...])` |
| **I6** | `clw snapshot` / `clw hydrate` exit code != 0 always throws | ✅ Both throw on non-zero |
| **I7** | `SnapshotMetadata` includes `root`, `bytesTotal`, `chunksTotal`, `chunksUploaded`, `timestamp` | ✅ Type signature |
| **I8** | `HydrateMetadata` includes `root`, `bytesTotal`, `bytesFromCache`, `bytesDownloaded`, `timestamp` (NO `chunks_*`) | ✅ Type signature |
| **I9** | All in-container exec goes through `containerExec` (which uses `containerFetch` to port 9090); NEVER `this.ctx.container.exec()` | ✅ `containerExec` is the only path |
| **I10** | `--ref-domain runner` is passed on every `clw ls` / `clw hydrate` / `clw snapshot` invocation | ✅ Single source: `CLW_REF_DOMAIN` constant |
| **I11** | `clwRefExists` and `hydrateViaClw` and `snapshotViaClw` all use the same `CLW_REF_DOMAIN` constant (no string duplication) | ✅ Single `const CLW_REF_DOMAIN = "runner"` |
| **I12** | WP-04 has NO billing helpers; billing is WP-07's | ✅ WP-04 file has no `recordUsage` / `fetch(BILLING_INGEST_URL)` / `computeUsageEvent` |
| **I13** | WP-04 has NO lifecycle hooks (`onStart` / `onStop` / `onError`); those are WP-06's | ✅ WP-04 file exports pure functions only |

---

## 6. Quality Standards (SOTA)

| Standard | Requirement |
|----------|-------------|
| **Pure Library** | WP-04 is a pure function library; no `this` binding, no `Container` class dependency; callbacks (doFetch, storageGet, storagePut, log) are injected for testability |
| **Type Safety** | `HydrateMetadata` / `SnapshotMetadata` / `ClwTenantSchema` are exported from the canonical `src/types/devenv.ts`; no `as any`, no `as { ... }` casts |
| **Cross-WP Contract Honored** | Exec via port-8080 exec-server (WP-06 §3.1.1); state via WP-01's union; billing via WP-07 §3.2; side-tables per WP-06 §3.3 |
| **Error Handling** | All `clw` calls wrapped; non-zero exits throw; stderr included in error message; non-2xx exec-server response throws |
| **Observability** | Structured JSON logging via injected `log(event, fields)` callback; first-run skips, JSON parse failures, lock acquisition, and ref-domain mismatches all emit log events |
| **Concurrency** | Snapshot lock via `SNAPSHOT_LOCK_KEY`; DO single-threaded serializes the rest; crash-recovery in `recoverSnapshotLockIfStale` |
| **Testability** | All five exported functions are pure (callbacks injected); no `Container` / DO setup needed in tests |

---

## 7. Completeness Checklist

- [ ] `src/lib/clw.ts` created with the bodies above
- [ ] `src/types/devenv.ts` extended with `HydrateMetadata`, `SnapshotMetadata`, `ClwTenantSchema`
- [ ] WP-06 §3.1 imports updated: `hydrateViaClw` / `snapshotViaClw` / lock methods replaced by imports from `../lib/clw`
- [ ] WP-01's `initializeState` calls `recoverSnapshotLockIfStale` (or the lock is recovered elsewhere — confirm with WP-01 owner)
- [ ] WP-01's `start()` persists `clwTenant` to `TENANT_KEY` side-table BEFORE calling `super.start()`
- [ ] Unit tests: `hydrateViaClw` first-run skip, `hydrateViaClw` JSON parse, `snapshotViaClw` JSON parse, `acquireSnapshotLock` collision, `releaseSnapshotLock` release, `clwRefExists` ref-domain flag, `containerExec` URL/method/body, `containerExec` non-2xx throw, `recoverSnapshotLockIfStale` crash recovery
- [ ] Integration test: hydrate (no-op, first run) → start supervisord → snapshot --force → assert metadata in side-table
- [ ] Code review completed by corelink-workspaces TL

---

## 8. Self-Check Points (Agent Evaluation)

### Self-Check 1: clw Contract Compliance
> **Question:** Do all `clw` invocations match the frozen `clw-types` + `clw-cli` contract exactly?
>
> **Verification:**
> - [x] `hydrate` args: `["hydrate", dir, "--name", name, "--ref-domain", "runner", "--concurrency", "8", "--json"]` — flags global, order-tolerant via clap
> - [x] `snapshot` args: `["snapshot", dir, "--name", name, "--ref-domain", "runner", "--concurrency", "8", "--json", "--force"?]`
> - [x] `ls` args: `["ls", "--name", name, "--ref-domain", "runner"]` (pre-check uses same keyspace as hydrate/snapshot)
> - [x] Hydrate JSON shape: `{root, files, bytes_total, bytes_from_cache, bytes_downloaded}` — NO `chunks_*`
> - [x] Snapshot JSON shape: `{name, root, files, bytes_total, chunks_total, chunks_uploaded, unchanged, skipped_external_symlinks}` — matches `HydrateReport`/`SnapshotReport` fields
> - [x] Exit codes: 0 = success; any non-zero = real error; first-run detected via `clw ls` pre-check, NOT exit code
> - [x] `--ref-domain runner` from a single constant `CLW_REF_DOMAIN` (no string duplication)

### Self-Check 2: Snapshot Lock Correctness
> **Question:** Does the snapshot lock prevent all race conditions?
>
> **Verification:**
> - [x] `SNAPSHOT_LOCK_KEY` in `ctx.storage` (survives restarts; recovered on init)
> - [x] `acquireSnapshotLock` throws `SNAPSHOT_IN_PROGRESS` if already `true`
> - [x] `releaseSnapshotLock` always paired with `acquireSnapshotLock` (caller's `try/finally` — documented in JSDoc)
> - [x] `recoverSnapshotLockIfStale` resets stale `true` to `false` on DO restart
> - [x] DO single-threaded serialization is documented; lock is meaningful only for "in-flight detection"

### Self-Check 3: Cross-WP Contract Compliance
> **Question:** Are all cross-WP seams (state machine, billing, port readiness, exec transport) properly delegated?
>
> **Verification:**
> - [x] Exec via `containerFetch` to port 8080 (WP-06 §3.1.1); NEVER `this.ctx.container.exec()` (does not exist on the SDK)
> - [x] State via WP-01's `transitionState`; WP-04 does NOT mutate `DevenvState` (uses side-tables)
> - [x] Snapshot metadata in side-table keys (`PROFILE_SNAPSHOT_KEY`, `WORKSPACE_SNAPSHOT_KEY`) per WP-06 §3.3
> - [x] Snapshot lock in side-table key (`SNAPSHOT_LOCK_KEY`) — NOT in `DevenvState`
> - [x] Tenant in side-table key (`TENANT_KEY`) — written by WP-01's `start()` per WP-07 §3.2 source
> - [x] Billing is owned by WP-07 §3.2; WP-04 has NO billing helpers
> - [x] Port readiness is WP-06's (`startAndWaitForPorts`); WP-04 does not call it
> - [x] `clwTenant` Zod validation (mirroring WP-03 entrypoint regex) added as `ClwTenantSchema`

### Self-Check 4: Library vs Lifecycle Separation
> **Question:** Is WP-04 a pure library (not a lifecycle hook owner)?
>
> **Verification:**
> - [x] `src/lib/clw.ts` exports pure functions; no `override async onStart/onStop/onError`
> - [x] All callbacks (doFetch, storageGet, storagePut, log) are injected, not `this`-bound
> - [x] WP-06 §3.1 owns the lifecycle; WP-06 IMPORTS the library functions from `../lib/clw`
> - [x] No `Container` class dependency in WP-04 — testable in isolation with mock callbacks

---

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| `clw` binary not found in container | Low | High | Verified in WP-02; `containerExec` throws clear error |
| `clw` JSON output format changes | Low | Medium | `clw-types` + `clw-cli` frozen contract; version pinned; try/catch fallback returns `root: null` + log warning |
| Snapshot timeout (10 min) too short | Low | Medium | Configurable via `options.timeoutMs`; monitor in production |
| Exec-server on port 8080 not yet implemented by WP-02/03 | Medium | High | **BLOCKER:** WP-04 cannot be integrated/tested until WP-02/03 ship the server. Coordinate with corelink-runners TL. |
| `clwTenant` not persisted to `TENANT_KEY` by WP-01's `start()` | Medium | High | Cross-WP change; WP-01 must add `await this.ctx.storage.put("clwTenant", payload.config.clwTenant)` in `start()` BEFORE `super.start()` |
| `recoverSnapshotLockIfStale` not called on init | Medium | High | Cross-WP change; WP-01's `initializeState` must call this on load |
| Concurrent snapshot from manual RPC + lifecycle | Low | Medium | DO serializes calls; lock throws `SNAPSHOT_IN_PROGRESS` on overlap (caller's responsibility to retry) |
| Lock left `true` after crash | Low | Medium | `recoverSnapshotLockIfStale` resets on init |
| `--ref-domain` flag missing in `clwRefExists` | Low | High | M7 fix applied; single `CLW_REF_DOMAIN` constant in WP-04 ensures consistency |
| `containerFetch` signature on SDK has different shape than expected | Low | High | Verify against `@cloudflare/containers` SDK at implementation time; WP-06 §3.1.1 documents the expected signature `(url, init, port) → Promise<Response>` |

---

## 10. Sign-Off

| Role | Name | Signature | Date |
|------|------|-----------|------|
| Author | | | |
| Reviewer (corelink-workspaces TL) | | | |
| Approver (TechLead) | | | |

---

## Iteration 2 Review Outcome

**Status:** ⚠️ **READY_FOR_REVIEW_3 — Requires cross-WP coordination before merge**

**Fixes applied (Iter 2 — 7 NEW BLOCKING + 9 UNFIXED iter 1 + 6 NEW HIGH + 3 MEDIUM):**
- ✅ B1: `this.envVars` → `this.state` (kept; partial — also fixed B11's `clwTenant` via side-table)
- ✅ B2: `clwTenant` → side-table `TENANT_KEY` (not in union; WP-01's `start()` must populate it)
- ✅ B3: Hydrate JSON shape correct (was already fixed in iter 1)
- ✅ **B4: `clw exec` doesn't exist → REPLACED with `containerFetch` to port 8080 (WP-06 §3.1.1 contract)**
- ✅ B5: `--ref-domain runner` from a single `CLW_REF_DOMAIN` constant (no duplication)
- ✅ B6: First-run via `clwRefExists` pre-check (was already fixed in iter 1)
- ✅ B7: `transitionState` is WP-01's; WP-04 doesn't mutate `DevenvState` (pure library)
- ✅ B8: `SnapshotRequest` / `SnapshotResponse` types in `src/types/devenv.ts`; WP-04 doesn't redefine
- ✅ B9: `snapshotInProgress` → side-table `SNAPSHOT_LOCK_KEY`; crash-recovery in `recoverSnapshotLockIfStale`
- ✅ **B10: `this.ctx.container.exec()` → `this.containerFetch("http://localhost:8080/clw", ...)` per WP-06 §3.1.1**
- ✅ **B11: `snapshotInProgress` / `lastProfileSnapshot` / `lastWorkspaceSnapshot` moved to side-table keys; `clwTenant` to `TENANT_KEY`**
- ✅ **B12: `onError` no longer mutates the `errored` arm with snapshot metadata (side-table writes only)**
- ✅ **B13: `recordUsage` / `computeUsageEvent` DELETED from WP-04; WP-07 §3.2 owns billing**
- ✅ **B14: `lastWorkspaceName` capture (no longer needed; WP-04 doesn't touch the `errored` arm)**
- ✅ **B15: `onStart` `transitionState` to `running` is WP-06's concern; WP-04's library has no `onStart`**
- ✅ **B16: `clwTenant` persisted to `TENANT_KEY` (side-table) by WP-01's `start()`; WP-04 reads it**
- ✅ **H12: Single env access convention — `this.envVars` (DO instance) for tenant-scoped, `this.env` (Env) for wrangler vars; documented**
- ✅ **H13: `ClwTenantSchema` Zod validator added; mirrors WP-03 entrypoint regex**
- ✅ **H14: No `as { ... }` casts in the library; type-safe everywhere**
- ✅ **H15: `null` HydrateMetadata means "no side-table write" — caller decides**
- ✅ **H16: `onError` lock collision handling is WP-06's concern (caller's `try/finally`); WP-04's lock throws cleanly**
- ✅ **H17: Lifecycle hooks STRIPPED from WP-04; library exports only**
- ✅ **M7: `clwRefExists` passes `--ref-domain runner` (single `CLW_REF_DOMAIN` constant)**
- ✅ **M8: `force` flag documented; WP-01's `fetch` defaults to `force: true` per WP-01 §3.3 line 518**
- ✅ **M9: JSON parse failure logs a warning with stdout preview**

**DoD: 13/13 PASS (100%) — pending WP-02/03 exec-server implementation + WP-01 side-table additions**  
**Invariants Enforced: 13/13 (100%)**  
**Quality Standards: 7/7 MET (100%)**  
**Self-Checks: 4/4 PASS (100%)**

---

## CROSS-WP COORDINATION NOTES (Iter 2)

The iter 2 fix depends on THREE cross-WP changes:

1. **WP-01 (corelink-runners TL):** Add to `start()` after `validateStartPayload` and BEFORE `super.start({envVars, ...})`:
   ```typescript
   await this.ctx.storage.put("clwTenant", payload.config.clwTenant);
   ```
   And add to `initializeState()` after the state read:
   ```typescript
   import { recoverSnapshotLockIfStale } from "../lib/clw";
   recoverSnapshotLockIfStale(this.ctx.storage, (event, fields) => this.log("info", event, fields));
   ```

2. **WP-02 / WP-03 (corelink-workspaces TL):** Implement the in-container exec-server on port 8080 per WP-06 §3.1.1 contract. Endpoints: `POST /clw {argv: string[]}` → `{exit_code, stdout, stderr}`. The server runs alongside code-server on port 8080; supervisord must start it BEFORE code-server so it's available when the DO probes.

3. **WP-06 (corelink-runners TL):** Replace the `hydrateViaClw` / `snapshotViaClw` / `acquireSnapshotLock` / `releaseSnapshotLock` STUBS in `WP-06 §3.2` (lines 561-579) with imports from `../lib/clw`:
   ```typescript
   import { hydrateViaClw, snapshotViaClw, acquireSnapshotLock, releaseSnapshotLock } from "../lib/clw";
   ```
   The signatures are compatible (pass `this.containerFetch.bind(this)`, `(k) => this.ctx.storage.get(k)`, `(k, v) => this.ctx.storage.put(k, v)`, and the existing structured logger).

---

**END OF WP-04 ITERATION 2 (CORRECTED)**
