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
 *   The D1 binding `ANALYTICS_DB` is owned by Phase-0 agent G
 *   (`apps/analytics-worker/`). Until that binding ships in the Pages env
 *   this handler short-circuits to a heartbeat-only stream — keeps the
 *   browser's `EventSource` open so the UI badge stays in the "waiting"
 *   state without spurious reconnects.
 *
 * Edge runtime is mandatory: ReadableStream + setInterval map onto the
 * Workers `ctx.waitUntil` lifecycle correctly there. Node.js runtime would
 * block the request thread.
 */

import { NextResponse } from "next/server";

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

function readEnv(): RouteEnv {
  // `next-on-pages` exposes Workers env via `process.env` at edge runtime
  // for primitive secrets, but bindings (D1, KV) flow through
  // `(globalThis as any).__env__` or `getRequestContext().env`. We read
  // defensively so the handler is testable without a Workers shim.
  const ctx = (
    globalThis as {
      __env__?: RouteEnv;
    }
  ).__env__;
  return ctx ?? {};
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
  const env = readEnv();

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
