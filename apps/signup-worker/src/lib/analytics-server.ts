// Server-side analytics emit — used by signup-worker webhook handlers.
//
// Fire-and-forget POST to the analytics-worker. Authenticated via the shared
// INGEST_KEY (set on both workers as `wrangler secret put`). Never throws —
// analytics MUST NOT break a webhook handler.

export interface AnalyticsEmitEnv {
    ANALYTICS_ENDPOINT: string;
    ANALYTICS_INGEST_KEY?: string;
}

export interface ServerEvent {
    id: string;
    event_name: string;
    tenant_id?: string | null;
    user_id?: string | null;
    session_id?: string | null;
    properties?: Record<string, unknown>;
    created_at?: string;
}

/** Generate a fresh event id. Crypto-safe; usable in Workers runtime. */
export function newEventId(): string {
    return crypto.randomUUID().replace(/-/g, "");
}

/**
 * Fire one or more events. Returns immediately even on error so the calling
 * webhook handler can always 200 to Clerk/Stripe (which retries on non-2xx).
 *
 * Caller is expected to wrap in `ctx.waitUntil` so the worker stays alive
 * until the POST resolves; otherwise CF may kill the isolate at handler return.
 */
export async function emit(env: AnalyticsEmitEnv, events: ServerEvent | ServerEvent[]): Promise<void> {
    const batch = Array.isArray(events) ? events : [events];
    try {
        const res = await fetch(env.ANALYTICS_ENDPOINT, {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                ...(env.ANALYTICS_INGEST_KEY
                    ? { "X-Corelink-Ingest-Key": env.ANALYTICS_INGEST_KEY }
                    : {}),
            },
            body: JSON.stringify({ events: batch }),
        });
        if (!res.ok) {
            console.warn(`[analytics] non-2xx ${res.status}: ${await res.text()}`);
        }
    } catch (err) {
        console.warn(`[analytics] fetch failed: ${(err as Error).message}`);
    }
}
