// POST /v1/event handler.
//
// Contract:
//   - Body: single EventPayload OR { events: EventPayload[] } for batch.
//   - Auth: either (a) Origin in ALLOWED_ORIGINS (browser, CORS-checked),
//           or (b) X-Corelink-Ingest-Key === INGEST_KEY (trusted server).
//   - Idempotency: INSERT OR IGNORE on PRIMARY KEY id (ULID). Duplicate
//                  ids are silently coalesced — same 200 response.
//   - Response: { accepted: number, rejected: number, errors?: [] }.
//
// Failure posture: a single bad event in a batch does NOT poison the batch.
// We never throw out of the handler — analytics errors must never break the
// caller's hot path. Errors are counted + returned in the body for debugging.

import type { Env, EventName, EventPayload } from "./types";

const ALLOWED_EVENT_NAMES: ReadonlySet<EventName> = new Set<EventName>([
    "landing_view",
    "landing_cta_click",
    "signup_started",
    "signup_completed",
    "tenant_created",
    "pat_issued",
    "welcome_view",
    "first_cli_authed",
    "first_cas_write",
    "first_cache_hit",
    "cache_hits_10",
    "cache_hits_100",
    "cache_hits_1k",
    "team_member_invited",
    "pricing_view",
    "checkout_started",
    "paid_subscription_started",
    "plan_downgraded",
    "subscription_canceled",
]);

interface IngestResult {
    accepted: number;
    rejected: number;
    errors: Array<{ id?: string; reason: string }>;
}

/**
 * CORS preflight + actual-request CORS validation. Returns the matching origin
 * if allowed, or `null` if rejected. The allow-list is comma-separated in
 * `ALLOWED_ORIGINS` (see `wrangler.toml`); browsers without an `Origin` header
 * (i.e. same-origin or non-browser) are not subject to this check and rely on
 * the `INGEST_KEY` header instead.
 */
function allowedOrigin(request: Request, env: Env): string | null {
    const origin = request.headers.get("Origin");
    if (!origin) return null;
    const allowList = env.ALLOWED_ORIGINS.split(",").map(s => s.trim());
    return allowList.includes(origin) ? origin : null;
}

/** Build a CORS response header bag for an allowed origin. */
function corsHeaders(origin: string): Record<string, string> {
    return {
        "Access-Control-Allow-Origin": origin,
        "Access-Control-Allow-Methods": "POST, OPTIONS",
        "Access-Control-Allow-Headers": "Content-Type, X-Corelink-Ingest-Key",
        "Access-Control-Max-Age": "86400",
        "Vary": "Origin",
    };
}

/** Constant-time string compare so secret-key mismatch can't be timing-probed. */
function constantTimeEqual(a: string, b: string): boolean {
    if (a.length !== b.length) return false;
    let diff = 0;
    for (let i = 0; i < a.length; i++) {
        diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
    }
    return diff === 0;
}

/**
 * Validate a single event. Returns `null` on success, or a string reason on
 * failure. Keeps the rules narrow + explicit so privacy rules (no email, no IP)
 * are enforced at the gate rather than relying on every caller to be careful.
 */
function validate(evt: unknown): string | null {
    if (!evt || typeof evt !== "object") return "not_an_object";
    const e = evt as Record<string, unknown>;
    if (typeof e["id"] !== "string" || e["id"].length === 0 || e["id"].length > 64) {
        return "invalid_id";
    }
    if (typeof e["event_name"] !== "string" || !ALLOWED_EVENT_NAMES.has(e["event_name"] as EventName)) {
        return "unknown_event_name";
    }
    for (const field of ["tenant_id", "user_id", "session_id"] as const) {
        const v = e[field];
        if (v !== undefined && v !== null && (typeof v !== "string" || v.length > 128)) {
            return `invalid_${field}`;
        }
    }
    if (e["properties"] !== undefined && (typeof e["properties"] !== "object" || e["properties"] === null)) {
        return "invalid_properties";
    }
    // Hard privacy gate: forbid email / IP in properties (PLG §7.3).
    const props = (e["properties"] as Record<string, unknown> | undefined) ?? {};
    for (const forbidden of ["email", "ip", "ip_address", "remote_addr"]) {
        if (forbidden in props) return `forbidden_field:${forbidden}`;
    }
    if (e["created_at"] !== undefined && typeof e["created_at"] !== "string") {
        return "invalid_created_at";
    }
    return null;
}

export async function handleIngest(request: Request, env: Env): Promise<Response> {
    // 1. Preflight.
    if (request.method === "OPTIONS") {
        const origin = allowedOrigin(request, env);
        return new Response(null, {
            status: 204,
            headers: origin ? corsHeaders(origin) : {},
        });
    }
    if (request.method !== "POST") {
        return new Response("Method Not Allowed", { status: 405 });
    }

    // 2. Authn: either CORS origin allow-list OR shared ingest key.
    const origin = allowedOrigin(request, env);
    const providedKey = request.headers.get("X-Corelink-Ingest-Key") ?? "";
    const keyOk = env.INGEST_KEY ? constantTimeEqual(providedKey, env.INGEST_KEY) : false;
    if (!origin && !keyOk) {
        return new Response(JSON.stringify({ error: "forbidden" }), {
            status: 403,
            headers: { "Content-Type": "application/json" },
        });
    }

    // 3. Parse body.
    let body: unknown;
    try {
        body = await request.json();
    } catch {
        return new Response(JSON.stringify({ error: "invalid_json" }), {
            status: 400,
            headers: {
                "Content-Type": "application/json",
                ...(origin ? corsHeaders(origin) : {}),
            },
        });
    }

    const events: EventPayload[] = Array.isArray((body as { events?: unknown })?.events)
        ? ((body as { events: EventPayload[] }).events)
        : [body as EventPayload];

    if (events.length === 0 || events.length > 100) {
        return new Response(JSON.stringify({ error: "invalid_batch_size" }), {
            status: 400,
            headers: {
                "Content-Type": "application/json",
                ...(origin ? corsHeaders(origin) : {}),
            },
        });
    }

    // 4. Validate + insert. Build a single batched D1 transaction so 100
    // events cost 1 RTT instead of 100. Bad rows are dropped, not retried.
    const result: IngestResult = { accepted: 0, rejected: 0, errors: [] };
    const statements: D1PreparedStatement[] = [];
    const stmt = env.ANALYTICS_DB.prepare(
        `INSERT OR IGNORE INTO analytics_events
            (id, event_name, tenant_id, user_id, session_id, properties, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, COALESCE(?7, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))`
    );

    for (const evt of events) {
        const reason = validate(evt);
        if (reason) {
            result.rejected++;
            const id = (evt as { id?: unknown })?.id;
            result.errors.push({
                id: typeof id === "string" ? id : undefined,
                reason,
            });
            continue;
        }
        statements.push(
            stmt.bind(
                evt.id,
                evt.event_name,
                evt.tenant_id ?? null,
                evt.user_id ?? null,
                evt.session_id ?? null,
                JSON.stringify(evt.properties ?? {}),
                evt.created_at ?? null,
            )
        );
        result.accepted++;
    }

    if (statements.length > 0) {
        try {
            await env.ANALYTICS_DB.batch(statements);
        } catch (err) {
            // D1 outage / quota — count as rejected so caller can retry,
            // but do NOT throw (callers must never fail because analytics did).
            result.rejected += result.accepted;
            result.accepted = 0;
            result.errors.push({ reason: `d1_error:${(err as Error).message}` });
        }
    }

    return new Response(JSON.stringify(result), {
        status: 200,
        headers: {
            "Content-Type": "application/json",
            ...(origin ? corsHeaders(origin) : {}),
        },
    });
}
