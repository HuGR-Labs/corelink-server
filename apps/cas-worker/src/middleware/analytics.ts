// CAS data-plane analytics middleware — Phase 0.G seed.
//
// Per Phase 0 execution plan §10, the cas-worker is being refactored by
// Wave-33 Stream B (`crates/cas-worker/`). This middleware seeds the emit
// contract so Stream B can adopt it without coordination cost — the function
// signatures below are the contract; the underlying transport is internal.
//
// The three events emitted here are dedup-per-tenant; only the very FIRST
// per tenant ever fires:
//   - first_cli_authed   on first authed /v1/ping  (TTFV milestone 1)
//   - first_cas_write    on first cache PUT        (TTFV milestone 2)
//   - first_cache_hit    on first cache HIT        (§3.2 canonical activation)
//
// Dedup is enforced by a KV `seen:` flag per tenant — see `markSeenOnce`.

interface CasAnalyticsEnv {
    ANALYTICS_ENDPOINT: string;
    ANALYTICS_INGEST_KEY?: string;
    // KV namespace used for the "have we seen this tenant before?" set.
    // Re-uses METADATA_KV if Stream B has not yet provisioned a dedicated one.
    METADATA_KV: KVNamespace;
}

export interface CasEventInput {
    env: CasAnalyticsEnv;
    tenantId: string;
    patId?: string;
    contentHash?: string;
    bytes?: number;
    latencyMs?: number;
    cfColo?: string;
    cliVersion?: string;
    userAgent?: string;
    /** Seconds from `tenant_created` to now — passed by caller (knows the signup ts). */
    secondsSinceSignup?: number;
    /** Seconds from `first_cas_write` to first hit — only meaningful for first_cache_hit. */
    secondsSinceFirstCasWrite?: number;
}

const KV_PREFIX = "corelink-analytics-seen:";

function newEventId(): string {
    return crypto.randomUUID().replace(/-/g, "");
}

/**
 * "Have we emitted `event` for `tenantId` before?" If yes, return true and
 * do nothing. If no, set the KV flag and return false so the caller proceeds
 * with the emit. KV write is best-effort; on KV failure we err on the side of
 * NOT emitting a duplicate (false-positive on `seen`).
 */
async function markSeenOnce(env: CasAnalyticsEnv, event: string, tenantId: string): Promise<boolean> {
    const key = `${KV_PREFIX}${event}:${tenantId}`;
    try {
        const existing = await env.METADATA_KV.get(key);
        if (existing) return true;
        // 0 = never expire — `first_*` events are one-shot for the tenant's lifetime.
        await env.METADATA_KV.put(key, "1");
        return false;
    } catch (err) {
        console.warn(`[cas-analytics] KV failure: ${(err as Error).message}; suppressing emit`);
        return true;
    }
}

async function postEvent(env: CasAnalyticsEnv, body: Record<string, unknown>): Promise<void> {
    try {
        const res = await fetch(env.ANALYTICS_ENDPOINT, {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                ...(env.ANALYTICS_INGEST_KEY
                    ? { "X-Corelink-Ingest-Key": env.ANALYTICS_INGEST_KEY }
                    : {}),
            },
            body: JSON.stringify(body),
        });
        if (!res.ok) {
            console.warn(`[cas-analytics] non-2xx ${res.status}`);
        }
    } catch (err) {
        console.warn(`[cas-analytics] post failed: ${(err as Error).message}`);
    }
}

/** TTFV milestone 1 — first authenticated `/v1/ping` per tenant. */
export async function emitFirstCliAuthed(input: CasEventInput): Promise<void> {
    if (await markSeenOnce(input.env, "first_cli_authed", input.tenantId)) return;
    await postEvent(input.env, {
        id: newEventId(),
        event_name: "first_cli_authed",
        tenant_id: input.tenantId,
        properties: {
            pat_id: input.patId,
            cli_version: input.cliVersion,
            user_agent: input.userAgent,
            seconds_since_signup: input.secondsSinceSignup,
        },
    });
}

/** TTFV milestone 2 — first cache write per tenant. */
export async function emitFirstCasWrite(input: CasEventInput): Promise<void> {
    if (await markSeenOnce(input.env, "first_cas_write", input.tenantId)) return;
    await postEvent(input.env, {
        id: newEventId(),
        event_name: "first_cas_write",
        tenant_id: input.tenantId,
        properties: {
            pat_id: input.patId,
            content_hash: input.contentHash,
            bytes: input.bytes,
            seconds_since_signup: input.secondsSinceSignup,
        },
    });
}

/** §3.2 canonical activation event — first cache HIT per tenant. */
export async function emitCacheHit(input: CasEventInput): Promise<void> {
    if (await markSeenOnce(input.env, "first_cache_hit", input.tenantId)) return;
    await postEvent(input.env, {
        id: newEventId(),
        event_name: "first_cache_hit",
        tenant_id: input.tenantId,
        properties: {
            pat_id: input.patId,
            content_hash: input.contentHash,
            latency_ms: input.latencyMs,
            cf_colo: input.cfColo,
            seconds_since_signup: input.secondsSinceSignup,
            seconds_since_first_cas_write: input.secondsSinceFirstCasWrite,
        },
    });
}
