/** Worker route fan-out and Server-Timing attribution helpers. */

export function isDsrEraseFanoutPath(pathSuffix: string): boolean {
  return pathSuffix === "/_internal/dsr/erase" || pathSuffix === "/_internal/dsr/verify";
}

/**
 * The `origin` sub-phases the CONTAINER reports, in emission order.
 *
 * They are produced by `crates/corelink-container/src/origin_timing.rs` on the
 * subresponse's own `Server-Timing`, and they partition the time the container
 * held the request:
 *
 *   - `opat`   — the container's per-request D1 `pat` row read (#1022 kept it
 *                so a revocation takes effect immediately)
 *   - `oquota` — the per-tenant monthly `$`-ceiling check/accrue (ADR-0068)
 *   - `ostore` — the moat CAS/R2 blob work and the native CAS/AC plane's R2
 *                object calls
 *   - `oaccounting` — bounded D1 byte-accounting and adapter URL-map windows;
 *                    this is separate from `ostore`, never an overlapping
 *                    annotation of it
 *   - `oargon`/`opermit`/`ortier`/`oaudit` — the detail phases the container
 *                emits only when `CORELINK_ORIGIN_TIMING_DETAIL=on`: Argon2id
 *                verification, its permit wait, the D1 tier resolution, and the
 *                blocking durable-audit D1 write
 *   - `oratelimit` — the bounded synchronous token-bucket admission decision
 *                    (tenant and credential-free OCI branches)
 *   - `ohandler` — explicitly accounted request-framework work (routing,
 *                  rate-limit layer, HMAC, response assembly)
 *
 * A name NOT on this list is dropped, and a dropped phase's milliseconds land in
 * `ohop` — i.e. real container work is reported as network. That is exactly what
 * happened when the detail phases shipped: with the flag armed, `ohop` measured
 * 114-125 ms against 17-20 ms unarmed, on the same tenant and colo, because
 * ~100 ms of `oaudit` was being discarded here. Any phase the container can emit
 * MUST be on this list.
 *
 * `ohandler` is the explicit request-framework phase the container computes
 * against its OWN whole-request clock, so these always sum to its total. During
 * the mixed rollout, `oother` is accepted as a compatibility alias and is
 * normalized to `ohandler` before reconciliation. (Hoisted; used in `fetch`.)
 */
const ORIGIN_CONTAINER_PHASES = [
  "opat",
  "oquota",
  "ostore",
  "oaccounting",
  "oargon",
  "opermit",
  "ortier",
  "oaudit",
  "oratelimit",
  "ohandler",
] as const;
const LEGACY_ORIGIN_CONTAINER_PHASES = ["oother"] as const;

function canonicalOriginPhase(name: string): string | undefined {
  if ((ORIGIN_CONTAINER_PHASES as readonly string[]).includes(name)) return name;
  if ((LEGACY_ORIGIN_CONTAINER_PHASES as readonly string[]).includes(name)) return "ohandler";
  return undefined;
}

/**
 * Whether this request may take the DO meter hop CONCURRENTLY with the origin
 * fetch instead of awaiting it first.
 *
 * The hop still happens — the DO is the counter, and skipping it would
 * under-count — but nothing in the origin fetch depends on its ANSWER, so on a
 * READ it can be collected just before the response is handed back. The cap is
 * still enforced before a single byte reaches the client; the honest cost is
 * that an over-cap READ makes the origin do its work first, which is the same
 * trade rt-nuclear #24 already accepted for the storage SUM.
 *
 * Every condition here is load-bearing:
 *
 *   - **GET/HEAD only.** A mutation that commits and is THEN 429'd is a
 *     correctness bug, not an optimization. `isStorageMutating` is checked too,
 *     so a verb-shaped read that still grows storage cannot slip through.
 *   - **`EDGE_ASYNC_METER === "on"`.** Deferring is only useful when the storage
 *     verdict already comes from cache; otherwise the request is about to make a
 *     synchronous D1 round trip anyway and nothing is saved.
 *   - **`meter`.** A fan-out sub-request is never metered, so there is no verdict
 *     to defer and no count to protect.
 *   - **tier CONFIRMED.** With `d1Error` we do not know the cap, so the request
 *     takes the exact path with its verb-aware fail posture, untouched.
 */
export function deferCapVerdictFor(input: {
  readonly method: string;
  readonly isStorageMutating: boolean;
  readonly asyncMeterMode: string | undefined;
  readonly meter: boolean;
  readonly tierD1Error: boolean;
}): boolean {
  const { method, isStorageMutating, asyncMeterMode, meter, tierD1Error } = input;
  if (method !== "GET" && method !== "HEAD") {
    return false;
  }
  return !isStorageMutating && asyncMeterMode === "on" && meter && !tierD1Error;
}

/**
 * The shared 429 quota-exceeded response body/headers.
 *
 * Extracted so the deferred request-cap verdict — enforced after the origin
 * fetch, on the concurrent-hop path — returns a response byte-identical to the
 * one the inline gates return, rather than a second, drifting copy of the same
 * shape. CORS is applied by the caller, matching both use sites.
 */
export function quotaExceededResponse(
  reason: string,
  retryAfterSec: number,
  requestId: string,
): Response {
  return new Response(
    JSON.stringify({ error: "QUOTA_EXCEEDED", message: reason, request_id: requestId }),
    {
      status: 429,
      headers: {
        "Content-Type": "application/json",
        "Retry-After": String(retryAfterSec),
        "X-Request-Id": requestId,
      },
    },
  );
}

/**
 * Which quota path a request takes at the edge — the composition rule for the
 * two independent optimizations that own DIFFERENT caps.
 *
 * `EDGE_DO_METER="serve"` makes the edge-local DO authoritative for the request
 * COUNT. `EDGE_ASYNC_METER="on"` (WP-B) takes the synchronous D1 round trip off
 * a warm READ by answering the STORAGE verdict from the ≤60 s-fresh B1 KV byte
 * count. They are orthogonal, but the arming condition used to read
 * `!serveHandled && mode === "on" && eligible`, so turning the DO serve path on
 * — which is the state of every prod region — disabled the async path for every
 * capped tier, and every warm read paid a D1 storage SUM it did not need
 * (`qbatch` 35-51 ms measured from colo=IAD, 115-124 ms from São Paulo).
 *
 * The verdicts:
 *
 *   - `"serve-fast"` — the DO metered this request AND the async rule arms:
 *     enforce storage from cache, then the DO's own cap verdict, skip D1.
 *   - `"fast"`       — no DO verdict, async rule arms: today's WP-B fast path.
 *   - `"shadow"`     — canary: decide-and-log only; the exact path still serves.
 *   - `"exact"`      — the full `runQuotaBatch` path, byte-identical to before.
 *
 * A request is `eligible` only when it is metered (never a fan-out), NOT
 * storage-mutating (a read cannot grow storage) and the tier is CONFIRMED — so
 * no branch here can trade a verdict it is not allowed to trade.
 */
export function quotaPathFor(input: {
  readonly serveHandled: boolean;
  readonly asyncMeterMode: string | undefined;
  readonly asyncMeterEligible: boolean;
}): "serve-fast" | "fast" | "shadow" | "exact" {
  const { serveHandled, asyncMeterMode, asyncMeterEligible } = input;
  if (!asyncMeterEligible) {
    return "exact";
  }
  if (asyncMeterMode === "on") {
    return serveHandled ? "serve-fast" : "fast";
  }
  if (asyncMeterMode === "shadow") {
    return "shadow";
  }
  return "exact";
}

/**
 * Compute the `qcontrol` phase — the explicitly accounted request-framework
 * remainder of `wdb` after its
 * four named sub-phases (`qtier`, `qdo`, `qbatch`, `qresid`).
 *
 * Same reconciliation discipline as `ohop` above, one level simpler because
 * there is nothing to parse: the four inputs are the Worker's own clocks, not
 * a remote report.
 *
 *   `qcontrol` = `wdb` − Σ(phases that ran)
 *
 * A phase still at its `-1` did-not-run sentinel is EXCLUDED from the sum,
 * never treated as a 0-cost phase — the same sentinel semantics the caller's
 * declarations document. If the phases that did run sum to MORE than `wdb`
 * (clock skew across the awaits), the split is dropped whole rather than
 * published wrong: `qcontrol;dur=<wdbMs>;desc="unreconciled"`, mirroring
 * the fail-closed split contract.
 *
 * Invariant: qtier + qdo + qbatch + qresid + qcontrol === wdb, exactly, always.
 */
export function wdbControlPhase(wdbMs: number, phases: readonly number[]): string {
  let sum = 0;
  for (const v of phases) {
    if (v >= 0) sum += v;
  }
  if (sum > wdbMs) {
    return `qcontrol;dur=${wdbMs};desc="unreconciled"`;
  }
  return `qcontrol;dur=${wdbMs - sum}`;
}

/**
 * Decompose `origin` into `ohop` + the container's own phases.
 *
 * # Why the residue is the Worker's job
 *
 * `origin` is the Worker's clock around `stub.fetch()`. The container can time
 * everything it does, but it cannot see the DO hop that brackets it, so the
 * Worker derives that by SUBTRACTION and publishes it as an explicit phase:
 *
 *   `ohop` = `origin` − Σ(container phases)
 *
 * `ohop` covers the Worker→DO dispatch + placement RPC, the DO's own prologue
 * (tenant-id lifecycle bind, `ensureContainerRunning`, the `getAlarm()` re-arm
 * read), the DO→container HTTP wire, and the response travelling back. Those
 * four are NOT separated: splitting the DO's prologue out needs the DO to
 * rewrite the subresponse headers on the hot path, which is a behaviour change
 * this instrumentation deliberately does not make. If `ohop` turns out to
 * dominate, that is the next split — the decomposition, not a guess, decides.
 *
 * # The reconciliation rule
 *
 * `ohop` + Σ(container phases) === `origin`, exactly, always. Two cases would
 * break that and both are handled by refusing to publish a split rather than by
 * publishing one that does not add up:
 *
 *   - **no container report** (an older container image, or a response the DO
 *     synthesized itself — a 503 `CONTAINER_UNAVAILABLE` never reached the
 *     container): return nothing. `origin` stands alone exactly as before, which
 *     is what lets this Worker deploy ahead of the container repin.
 *   - **a report that does not fit** (Σ > `origin`, or a known phase name with
 *     an unparseable duration): return a single
 *     `ohop;dur=<origin>;desc="unreconciled"`. The split is dropped whole, and
 *     the `desc` says so on the wire. A split that silently redistributes a
 *     phase it could not read is worse than no split — it invites a confident
 *     wrong conclusion, which is the entire failure mode this instrumentation
 *     exists to prevent.
 *
 * ⚠️ The two sides do NOT share a clock resolution. The Worker's `Date.now()`
 * advances only across I/O, so a Worker phase at `dur=0` means "no I/O"; the
 * container measures a real monotonic `Instant`, so a container phase at
 * `dur=0` means "under a millisecond of wall time". `ohop` is a difference of
 * the two and carries ±1 ms of truncation either way — directional attribution,
 * never a profile.
 */
export function originSubPhases(originMs: number, containerTiming: string | null): string[] {
  if (containerTiming === null) return [];
  const reported = new Map<string, number>();
  let malformed = false;
  for (const part of containerTiming.split(",")) {
    // `name;dur=<int>` with optional trailing parameters (e.g. a `desc`).
    const m = /^\s*([A-Za-z0-9_-]+)\s*;\s*dur=([0-9]+)\s*(?:;.*)?$/.exec(part);
    const name = m === null ? part.trim().split(";")[0]?.trim() : m[1];
    const canonicalName = name === undefined ? undefined : canonicalOriginPhase(name);
    if (canonicalName === undefined) {
      // Not one of ours — a future container phase, or a stray metric. Ignore
      // it rather than treating the whole report as broken.
      continue;
    }
    if (m === null) {
      // A name we DO consume, carrying a duration we cannot read. Its
      // milliseconds would silently land in `ohop` and be attributed to the
      // network. Refuse the split instead.
      malformed = true;
      continue;
    }
    const value = Number(m[2]);
    const previous = reported.get(canonicalName);
    if (previous !== undefined && previous !== value) {
      // A dual-rollout report must carry the exact same duration under both
      // names. Conflicting aliases are stale/corrupt and must not leak either
      // value into `ohop`.
      malformed = true;
      continue;
    }
    reported.set(canonicalName, value);
  }
  if (reported.size === 0) return [];
  let sum = 0;
  for (const v of reported.values()) sum += v;
  if (malformed || sum > originMs) {
    return [`ohop;dur=${originMs};desc="unreconciled"`];
  }
  const out = [`ohop;dur=${originMs - sum}`];
  for (const name of ORIGIN_CONTAINER_PHASES) {
    const v = reported.get(name);
    // Emission contract, identical to the `wdb` sub-phases: a phase that RAN is
    // emitted INCLUDING at `dur=0`; only a phase the container did not run at
    // all is absent.
    if (v !== undefined) out.push(`${name};dur=${v}`);
  }
  return out;
}
