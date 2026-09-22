/** Response forwarding, deferred quota, timing, and CORS finalisation. */
import type { ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import { applyCors, applyTimingPad } from "./index_auth.js";
import type { AuthStageResult } from "./index_auth_stage.js";
import type { QuotaStageResult } from "./index_quota_stage.js";
import type { RouteMatch } from "./route_match.js";
import type { RoutingStageResult } from "./index_routing_stage.js";
import { runQuotaBatch, secondsUntilNextMonthStart } from "./lib/quota.js";
import { quotaExceededResponse } from "./index_observability.js";
import { findMissingResponseBody, shadowCompareEdgeFindMissing } from "./lib/edge_find_missing.js";
import { shadowCompareEdgePublicRead } from "./lib/edge_public_read.js";
import { invalidatePatVerifyCache, patRowKvKey } from "./lib/pat_verify_cache.js";
import { getServerNonce } from "./index_common.js";
import { reapiError } from "./index_common.js";
import { originSubPhases, wdbControlPhase } from "./index_observability.js";

export async function finishResponse(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
  requestId: string,
  requestStart: number,
  requestCounter: number,
  route: RouteMatch,
  authStage: AuthStageResult,
  quota: QuotaStageResult,
  routing: RoutingStageResult,
  edgeServed: Response | null,
): Promise<Response> {
  const { auth, resolvedTenantId, stAuthStart, stAuthEnd, stPatSource } = authStage;
  const { stub, augmented, findMissingShadowBody } = routing;
  const { pendingCapVerdict, deferredCap, deferredCapTierResult, stQTierMs, stQDoMs, stQBatchMs } = quota;
  const stQResidMs = routing.stQResidMs;
  let stOriginStart = 0;
  let stOriginEnd = 0;
    let doResponse: Response;
    if (edgeServed) {
      doResponse = edgeServed;
    } else {
      try {
        stOriginStart = Date.now();
        doResponse = await stub.fetch(augmented);
        stOriginEnd = Date.now();
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        // Do NOT include error detail that could leak internal topology
        console.error(`[${requestId}] DO fetch failed: ${message.slice(0, 80)}`);
        // OCI never reaches this PAT-gate DO forward — see the dedicated
        // pass-through branch above (which has its own DO forward + error path).
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
    }

    // ── Enforce the deferred request-cap verdict, before ANY byte is served ──
    // The DO hop ran CONCURRENTLY with the origin fetch above (see
    // `pendingCapVerdict`). This is where it is collected. Nothing has reached
    // the client yet — `doResponse` is a Response object in this isolate — so a
    // 429 here is indistinguishable, from the caller's side, from the 429 the
    // awaited path returned before the fetch. What the caller cannot see, and
    // what is the honest cost of this change, is that an over-cap READ made the
    // origin do its work first.
    //
    // A `null` verdict is a DO fault, and it is fail-OPEN exactly as before: the
    // request is served. The D1 counter fallback that the awaited path ran
    // inline moves to `ctx.waitUntil` here, because by this point the response is
    // already built and blocking it on a D1 write would hand back the latency
    // this whole path removed. `waitUntil` carries no durability guarantee, so a
    // DO outage can now UNDER-count — the direction this counter's fail-open
    // design already tolerates and already produces when the UPSERT itself
    // throws; it can never over-count, which is the failure that would wrongly
    // 429 a paying tenant.
    if (pendingCapVerdict !== null) {
      const verdict = await pendingCapVerdict;
      if (verdict === null) {
        const capDb = env.CONFIG_DB;
        const capTenant = resolvedTenantId;
        const capTier = deferredCapTierResult;
        if (capTier !== null) {
          ctx.waitUntil(
            runQuotaBatch(capDb, capTenant, capTier, { meter: true, isMutating: false }).then(
              () => undefined,
              () => undefined,
            ),
          );
        }
      } else if (!verdict.withinCap) {
        return applyCors(
          quotaExceededResponse(
            `Monthly request quota exceeded: limit is ${deferredCap?.cap ?? 0} (tier: ${
              deferredCap?.tier ?? "unknown"
            })`,
            secondsUntilNextMonthStart(),
            requestId,
          ),
          request,
        );
      }
    }

    // F1 SHADOW (findMissingBlobs): prove the edge answer EQUALS the container's
    // on real traffic before anything flips, and measure the edge's own cost in
    // the same pass. Zero user impact — the container's response is served
    // unchanged and only a CLONE is read here, in the background.
    //
    // Why this route is worth moving: measured from inside the fabric, the
    // container answers at ~13 digests/second, and that ceiling is NOT our
    // fan-out — 16 separate single-digest requests (1.19 s wall) are no faster
    // than one 16-digest request (1.59-1.82 s). See
    // `docs/design/2026-08-25-adr-edge-native-find-missing.md`.
    if (findMissingShadowBody !== null && doResponse.ok) {
      const fmClone = doResponse.clone();
      const fmTenant = resolvedTenantId;
      const fmBody = findMissingShadowBody;
      ctx.waitUntil(
        shadowCompareEdgeFindMissing(env, fmTenant, fmBody, fmClone)
          .then((verdict) =>
            console.log(`[${requestId}] edge_find_missing_shadow ${verdict}`),
          )
          .catch((e) =>
            console.error(
              `[${requestId}] edge_find_missing_shadow error: ${String(e).slice(0, 80)}`,
            ),
          ),
      );
    }

    // F3.3 SHADOW: prove Worker-native `_public` edge-read parity vs the container
    // on real traffic, with ZERO user impact — we serve the container's response
    // unchanged and only compare a CLONE in the background (`ctx.waitUntil`).
    // Reached only on the tenant's home-region leg (non-local regions early-return
    // above), so `env.CAS_BUCKET`/`env.R2_CAS_REGION` are correct for this tenant.
    // Gate on the "shadow" flag; brew/pip GETs only (the `_public` byte surfaces).
    if (
      env.EDGE_PUBLIC_READ === "shadow" &&
      request.method === "GET" &&
      (route.routeKind === "brew" || route.routeKind === "pip") &&
      doResponse.ok
    ) {
      const shadowClone = doResponse.clone();
      const shadowKind = route.routeKind;
      const shadowPath = route.pathSuffix;
      ctx.waitUntil(
        shadowCompareEdgePublicRead(env, shadowKind, shadowPath, shadowClone)
          .then((verdict) =>
            console.log(
              `[${requestId}] edge_public_shadow routeKind=${shadowKind} verdict=${verdict}`,
            ),
          )
          .catch((e) =>
            console.error(
              `[${requestId}] edge_public_shadow error: ${String(e).slice(0, 80)}`,
            ),
          ),
      );
    }

    // Map DO 404 responses through timing-pad (cross-tenant enumeration defence)
    if (doResponse.status === 404) {
      await applyTimingPad(
        requestStart,
        requestId,
        requestCounter,
        getServerNonce(),
      );
    }

    // Attach request-id to outbound response if DO didn't already set it
    const finalHeaders = new Headers(doResponse.headers);
    // Container-owned customer PAT revokes return the non-secret token_id in an
    // internal response header. Evict the Worker-owned positive auth row before
    // exposing the response; KV errors are a TTL-backed availability concern,
    // never a reason to turn an authoritative D1 revoke into a 500. The header
    // is always stripped so this coordination handle is not public API.
    const revokedTokenId =
      route.routeKind === "customer_v1"
        ? finalHeaders.get("x-corelink-pat-cache-invalidate")
        : null;
    finalHeaders.delete("x-corelink-pat-cache-invalidate");
    if (revokedTokenId !== null && revokedTokenId.length > 0) {
      invalidatePatVerifyCache(revokedTokenId);
      const revokeKv = (env as unknown as {
        METADATA_KV?: { delete(key: string): Promise<void> };
      }).METADATA_KV;
      if (revokeKv) {
        ctx.waitUntil(
          revokeKv.delete(patRowKvKey(revokedTokenId)).catch((error: unknown) => {
            console.error(
              `[${requestId}] customer revoke KV eviction failed (TTL backstop): ${error instanceof Error ? error.message.slice(0, 80) : "unknown error"}`,
            );
          }),
        );
      }
    }
    if (!finalHeaders.has("x-request-id")) {
      finalHeaders.set("x-request-id", requestId);
    }
    // Server-Timing (observability, self-serve latency probes): split the authed
    // hot path into `auth` (PAT verify — KV-served ⇒ single-digit ms), `wdb` (the
    // worker-side quota + residency reads to the ENAM primary) and its three
    // serial sub-phases, and `origin` (the DO/container subrequest) and ITS
    // sub-phases — `ohop` (the DO hop, derived here) plus `opat` / `oquota` /
    // `ostore` / `ohandler`, which the container reports on the subresponse and
    // `originSubPhases` merges. This is what lets a client distinguish "auth is
    // slow" from "the downstream D1 reads are slow" from "the container's own
    // per-request D1 read is slow" WITHOUT a log grep. `desc` on `auth` carries
    // the cache tier (`kv`/`l1`/`d1`).
    //
    // EMISSION CONTRACT — a phase that RAN is emitted, INCLUDING at `dur=0`; only
    // a phase that did NOT run is omitted. This matters more than it looks: a
    // Worker's `Date.now()` advances only across I/O, so a KV-served or L1-served
    // phase genuinely measures 0, and suppressing it would make "served from cache,
    // faster than the clock resolves" indistinguishable from "never executed" on
    // the wire. `auth`/`wdb`/`origin` used to be emitted on a strict `>`, which is
    // exactly that bug: the 2026-08-04 production probe reported `auth` on 3 of 30
    // responses and the missing 27 had to be INFERRED to be KV-served rather than
    // skipped. They now gate on "did this phase run?" like the sub-phases do.
    //
    // Durations are coarse by construction (Date.now advances across I/O only) —
    // directional attribution, never a profile.
    {
      const st: string[] = [];
      // `stAuthEnd > 0` ⇔ the auth phase ran (both clocks start at 0 and are only
      // ever set to a real Date.now()). Pre-tenant `signup` never enters it.
      if (stAuthEnd > 0) {
        const d = stAuthEnd - stAuthStart;
        st.push(stPatSource ? `auth;dur=${d};desc="${stPatSource}"` : `auth;dur=${d}`);
      }
      // `wdb` spans auth-end → origin-start, so it ran iff BOTH ends are stamped.
      // The `stAuthEnd > 0` half is load-bearing beyond the ran-check: on a route
      // that skipped auth it would otherwise compute `stOriginStart - 0`, i.e. the
      // epoch, and publish it as a duration.
      if (stAuthEnd > 0 && stOriginStart > 0) {
        st.push(`wdb;dur=${stOriginStart - stAuthEnd}`);
      }
      // The THREE always-on awaits `wdb` is made of, in execution order. `-1`
      // is the did-not-run sentinel (see the declarations); anything >= 0 ran
      // and is reported at its real cost, 0 included. `qbatch` is ONE D1
      // round trip carrying the metering UPSERT and the storage SUM — the two
      // phases that shipped as `qmeter` and `qstor` and were the whole of `wdb`.
      //
      // `qdo` (the awaited `serveViaDO(...)` hop — the 2026-08 prod gap where
      // qtier+qbatch+qresid summed to only 122 ms of a 265 ms `wdb`) and
      // `qcontrol` (wdb's framework work) is diagnostic gated on
      // `SERVER_TIMING_WDB_DETAIL === "on"` — see the flag's doc-comment on
      // `Env`. `qdo`'s presence is a confirmation oracle for whether a
      // client-supplied `x-corelink-fanout-from` matched the internal auth
      // key, so it must NOT ship by default; `qcontrol` only exists to keep the
      // reconciliation invariant honest while `qdo` is on, so it is gated
      // identically. `stQDoMs` itself is still stamped unconditionally above
      // (the clock is free; only the emission is gated) so flipping the flag
      // needs no redeploy of the timing code, only of this gate.
      const wdbDetail = (env as unknown as { SERVER_TIMING_WDB_DETAIL?: string })
        .SERVER_TIMING_WDB_DETAIL === "on";
      const deployment = env as unknown as {
        CORELINK_DEPLOYED_SHA?: string;
        CORELINK_DEPLOYED_REGION?: string;
      };
      if (deployment.CORELINK_DEPLOYED_SHA) {
        finalHeaders.set("X-Corelink-Deployed-Sha", deployment.CORELINK_DEPLOYED_SHA);
      }
      if (deployment.CORELINK_DEPLOYED_REGION) {
        finalHeaders.set("X-Corelink-Deployed-Region", deployment.CORELINK_DEPLOYED_REGION);
      }
      if (wdbDetail) {
        finalHeaders.set("X-Corelink-Server-Timing-Wdb-Detail", "on");
      }
      for (const [name, ms] of [
        ["qtier", stQTierMs],
        ...(wdbDetail ? ([["qdo", stQDoMs]] as const) : ([] as const)),
        ["qbatch", stQBatchMs],
        ["qresid", stQResidMs],
      ] as const) {
        if (ms >= 0) st.push(`${name};dur=${ms}`);
      }
      // `qcontrol`: framework work left from `wdb` after the q-phases above (`qdo`
      // included when the flag is on, excluded when it is off — either way
      // the invariant below holds for exactly the phases actually emitted).
      // Mirrors `ohop`'s reconciliation discipline exactly (see
      // `originSubPhases`): only emitted when `wdb` itself was (same
      // `stAuthEnd > 0 && stOriginStart > 0` guard) AND the flag is on, the
      // sum excludes any phase still at `-1` (did-not-run, never counted as
      // 0), and a sum that overshoots `wdb` is dropped whole as
      // `qcontrol;dur=<wdb>;desc="unreconciled"` rather than silently
      // redistributed.
      //
      // Invariant (flag on): qtier + qdo + qbatch + qresid + qcontrol === wdb,
      // exactly, always. (flag off): the header is byte-identical to before
      // this change — no `qdo`, no `qcontrol`.
      if (wdbDetail && stAuthEnd > 0 && stOriginStart > 0) {
        st.push(
          wdbControlPhase(stOriginStart - stAuthEnd, [stQTierMs, stQDoMs, stQBatchMs, stQResidMs]),
        );
      }
      // `origin` and, immediately after it, the sub-phases it decomposes into —
      // the same parent-then-children order `wdb` and its `q*` phases use. The
      // container reports its own share on the SUBRESPONSE's `Server-Timing`
      // (see `crate::origin_timing` in the Rust container); the Worker owns the
      // `ohop` residue because only the Worker can see both ends of the DO hop.
      // The container's raw header never reaches the client: `finalHeaders` is a
      // copy of the DO response's headers and the `set` below overwrites it.
      if (stOriginEnd > 0) {
        const originMs = stOriginEnd - stOriginStart;
        st.push(`origin;dur=${originMs}`);
        for (const sub of originSubPhases(originMs, doResponse.headers.get("server-timing"))) {
          st.push(sub);
        }
      }
      st.push(`total;dur=${Date.now() - requestStart}`);
      if (st.length > 0) finalHeaders.set("Server-Timing", st.join(", "));
    }
    const finalResponse = new Response(doResponse.body, {
      status: doResponse.status,
      statusText: doResponse.statusText,
      headers: finalHeaders,
    });

    return applyCors(finalResponse, request);
}
