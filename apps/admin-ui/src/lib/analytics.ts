// Client-side analytics ingest — admin-ui edition.
//
// One-shot fire-and-forget POST to the analytics-worker. Errors are swallowed:
// analytics MUST NOT crash the caller's hot path.
//
// Privacy:
//   - Never include email, IP, or any PII in `properties`. The worker enforces
//     this server-side (apps/analytics-worker/src/ingest.ts), but doing the
//     check at the call-site too is cheaper than a round-trip.

const ENDPOINT =
    process.env["NEXT_PUBLIC_ANALYTICS_ENDPOINT"]
    ?? "https://corelink-analytics.humangr.com/v1/event";

const SESSION_KEY = "corelink_session_id";

/** Generate a ULID-ish identifier (sortable, 26 chars). Crypto-safe. */
export function newId(): string {
    if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
        return crypto.randomUUID().replace(/-/g, "");
    }
    return Math.random().toString(36).slice(2) + Date.now().toString(36);
}

/** Read or create a session id (sessionStorage, not a cookie — not subject to consent). */
export function sessionId(): string {
    if (typeof window === "undefined") return "ssr";
    try {
        const existing = window.sessionStorage.getItem(SESSION_KEY);
        if (existing) return existing;
        const fresh = newId();
        window.sessionStorage.setItem(SESSION_KEY, fresh);
        return fresh;
    } catch {
        return "no-storage";
    }
}

export interface TrackOptions {
    tenantId?: string;
    userId?: string;
    properties?: Record<string, unknown>;
}

/**
 * Fire an analytics event. Returns immediately; the POST is best-effort.
 * Uses `fetch(..., { keepalive: true })` so the request survives a page
 * navigation (important for `signup_started` immediately followed by
 * the Clerk widget mounting a redirect).
 */
export function track(eventName: string, opts: TrackOptions = {}): void {
    if (typeof fetch === "undefined") return;
    const payload = {
        id: newId(),
        event_name: eventName,
        tenant_id: opts.tenantId ?? null,
        user_id: opts.userId ?? null,
        session_id: sessionId(),
        properties: opts.properties ?? {},
    };
    try {
        void fetch(ENDPOINT, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload),
            keepalive: true,
            // Browser sends Origin header — analytics-worker CORS check
            // gates on ALLOWED_ORIGINS.
            mode: "cors",
            credentials: "omit",
        }).catch(() => {
            // Swallow — never break the calling page.
        });
    } catch {
        // Swallow.
    }
}
