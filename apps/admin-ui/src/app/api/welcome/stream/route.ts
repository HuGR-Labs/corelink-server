/**
 * GET /api/welcome/stream — Server-Sent Events stream of activation events
 * for the currently authenticated tenant.
 *
 * Per Phase-0 PLG framework §4 step 3 + execution plan §2.F.
 *
 * Stream contract:
 *   - text/event-stream with `:keepalive\n\n` every 15 s.
 *   - One `data: {…json…}\n\n` frame per row pulled from `analytics_events`
 *     where `tenant_id = <session-resolved>` AND `event_name IN
 *     ('welcome_view', 'first_cli_authed', 'first_cas_write',
 *      'first_cache_hit')` AND `created_at > <since>`.
 *   - The `since` cursor advances on each successful flush.
 *
 * Storage backend:
 *   The D1 binding `ANALYTICS_DB` points at `corelink-analytics-prod`, whose
 *   schema + migrations are owned by `apps/analytics-worker/`. It is declared
 *   for this Worker in `apps/admin-ui/wrangler.toml`. If the binding is ever
 *   absent (local `next dev` without a Workers context, a stripped preview
 *   env) this handler degrades to a heartbeat-only stream — that keeps the
 *   browser's `EventSource` open so the UI badge stays in the "waiting" state
 *   without spurious reconnects, instead of 500-ing.
 *
 * Edge runtime is mandatory: ReadableStream + setInterval map onto the
 * Workers `ctx.waitUntil` lifecycle correctly there. Node.js runtime would
 * block the request thread.
 */

import { NextResponse } from "next/server";
import { getCloudflareContext } from "@opennextjs/cloudflare";

export const dynamic = "force-dynamic";

const POLL_INTERVAL_MS = 1500;
const KEEPALIVE_INTERVAL_MS = 15_000;

type SessionClaims = {
  tenant_id?: string;
};

type AnalyticsRow = {
  id: string;
  event_name: string;
  tenant_id: string;
  created_at: string;
  properties?: string;
};

interface D1PreparedStatement {
  bind(...values: unknown[]): D1PreparedStatement;
  all<T = unknown>(): Promise<{ results?: T[] }>;
}
interface D1Database {
  prepare(query: string): D1PreparedStatement;
}
interface RouteEnv {
  ANALYTICS_DB?: D1Database;
}

async function resolveTenantId(): Promise<string | null> {
  const mod = await import("@clerk/nextjs/server").catch(() => null);
  if (!mod) return null;
  const session = await (
    mod as { auth: () => Promise<{ sessionClaims?: SessionClaims }> }
  ).auth();
  return session.sessionClaims?.tenant_id ?? null;
}

async function readEnv(): Promise<RouteEnv> {
  // admin-ui runs on `@opennextjs/cloudflare` (wrangler.toml
  // `main = ".open-next/worker.js"`), which publishes the Workers bindings on
  // the Cloudflare context — read via `getCloudflareContext()`. It does NOT
  // populate `globalThis.__env__`; that is the `next-on-pages` convention this
  // app migrated away from, and reading it here always yielded `undefined`.
  //
  // `async: true` is used deliberately: in the deployed Worker both overloads
  // read the same global (so it costs nothing), but only the async overload can
  // also resolve the context under the `next dev` Node runtime.
  //
  // Defensive by design — any failure (no Cloudflare context, `next dev`
  // without `initOpenNextCloudflareForDev`) degrades to `{}` so `pollEvents`
  // reports "no events yet" rather than throwing a 500 at the client.
  try {
    const { env } = await getCloudflareContext({ async: true });
    // The generated `CloudflareEnv` does not declare this app-specific binding;
    // narrow structurally at the boundary.
    return (env ?? {}) as unknown as RouteEnv;
  } catch {
    return {};
  }
}

async function pollEvents(
  db: D1Database | undefined,
  tenantId: string,
  sinceIso: string,
): Promise<AnalyticsRow[]> {
  if (!db) return [];
  const stmt = db
    .prepare(
      "SELECT id, event_name, tenant_id, created_at, properties " +
        "FROM analytics_events WHERE tenant_id = ?1 AND created_at > ?2 " +
        "AND event_name IN ('welcome_view', 'first_cli_authed', 'first_cas_write', 'first_cache_hit') " +
        "ORDER BY created_at ASC LIMIT 50",
    )
    .bind(tenantId, sinceIso);
  const { results } = await stmt.all<AnalyticsRow>();
  return results ?? [];
}

export async function GET(): Promise<Response> {
  const tenantId = await resolveTenantId();
  if (!tenantId) {
    return NextResponse.json(
      { error: "no_active_session" },
      { status: 401 },
    );
  }
  const env = await readEnv();

  const encoder = new TextEncoder();
  let cursor = new Date(0).toISOString();
  let closed = false;

  const stream = new ReadableStream<Uint8Array>({
    async start(controller) {
      function send(line: string): void {
        if (closed) return;
        controller.enqueue(encoder.encode(line));
      }

      send(`: connected ${new Date().toISOString()}\n\n`);

      const keepalive = setInterval(() => {
        send(`: keepalive ${Date.now()}\n\n`);
      }, KEEPALIVE_INTERVAL_MS);

      const poll = setInterval(async () => {
        if (closed) return;
        try {
          const rows = await pollEvents(env.ANALYTICS_DB, tenantId, cursor);
          for (const row of rows) {
            const payload = JSON.stringify({
              id: row.id,
              event_name: row.event_name,
              created_at: row.created_at,
            });
            send(`event: ${row.event_name}\n`);
            send(`data: ${payload}\n\n`);
            cursor = row.created_at;
          }
        } catch {
          // Don't crash the stream on a transient D1 error — surface as a
          // comment frame so the client side stays connected.
          send(`: poll-error ${Date.now()}\n\n`);
        }
      }, POLL_INTERVAL_MS);

      const close = (): void => {
        if (closed) return;
        closed = true;
        clearInterval(keepalive);
        clearInterval(poll);
        try {
          controller.close();
        } catch {
          /* already closed */
        }
      };

      // Client disconnect is signalled via stream cancellation, handled in
      // `cancel()` below. Belt-and-braces unref for runtime timers.
      void close;
    },
    cancel() {
      closed = true;
    },
  });

  return new Response(stream, {
    status: 200,
    headers: {
      "content-type": "text/event-stream; charset=utf-8",
      "cache-control": "no-store, no-transform",
      connection: "keep-alive",
      "x-accel-buffering": "no",
    },
  });
}
