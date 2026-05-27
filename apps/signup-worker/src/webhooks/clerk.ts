// Clerk webhook handler.
//
// Phase 0.G scope: fire `signup_completed`, `tenant_created`, `pat_issued`
// analytics events per PLG §7.1 taxonomy. Auto-provision logic itself
// (tenant creation, region assignment, first PAT issuance) lands in
// Phase 0.F (ONBOARDING-WIZARD-2STEP) — this handler exposes the call sites.
//
// Svix signature verification is intentionally a TODO with a hard-fail
// default: if `CLERK_WEBHOOK_SECRET` is configured, the handler MUST verify
// or reject; if not configured, it returns 503 to prevent accidental open
// ingest. Phase 0.F replaces this stub with proper svix verification.

import type { AnalyticsEmitEnv } from "../lib/analytics-server";
import { emit, newEventId } from "../lib/analytics-server";

export interface ClerkWebhookEnv extends AnalyticsEmitEnv {
    CLERK_WEBHOOK_SECRET?: string;
}

interface ClerkUserCreated {
    type: "user.created";
    data: {
        id: string;
        email_addresses?: Array<{ email_address?: string }>;
        external_accounts?: Array<{ provider?: string }>;
        created_at?: number;
    };
}

// Subset of `WebhookEvent` we care about today. Add more as Phase 0.F lands.
type ClerkEvent = ClerkUserCreated | { type: string; data: unknown };

/** Extract email domain only — never persist the full email per PLG §7.3. */
function emailDomain(email: string | undefined): string | null {
    if (!email || !email.includes("@")) return null;
    const at = email.lastIndexOf("@");
    return email.slice(at + 1).toLowerCase();
}

export async function handleClerkWebhook(
    request: Request,
    env: ClerkWebhookEnv,
    ctx: ExecutionContext,
): Promise<Response> {
    // Phase 0.F replaces this with proper Svix signature verification. Until
    // then, refuse to process if no secret is configured so we never accept
    // unsigned webhooks in any environment.
    if (!env.CLERK_WEBHOOK_SECRET) {
        return new Response("clerk webhook secret not configured", { status: 503 });
    }

    let event: ClerkEvent;
    try {
        event = (await request.json()) as ClerkEvent;
    } catch {
        return new Response("invalid_json", { status: 400 });
    }

    if (event.type === "user.created") {
        const userEvt = event as ClerkUserCreated;
        const userId = userEvt.data.id;
        const domain = emailDomain(userEvt.data.email_addresses?.[0]?.email_address);
        const provider = userEvt.data.external_accounts?.[0]?.provider ?? "email";

        // Auto-provision (stub) — Phase 0.F replaces with real tenant + PAT issuance.
        // For now we emit the analytics events with synthetic identifiers so the
        // funnel measurement is wired end-to-end and Phase 0.F just has to
        // replace the synthetic ids with real ones.
        const tenantId = `t_${userId}`;
        const patId = `pat_${userId}_0`;

        ctx.waitUntil(emit(env, [
            {
                id: newEventId(),
                event_name: "signup_completed",
                user_id: userId,
                properties: { auth_provider: provider, email_domain: domain },
            },
            {
                id: newEventId(),
                event_name: "tenant_created",
                tenant_id: tenantId,
                user_id: userId,
                properties: { plan: "free", region: "auto" /* Geo-IP per Phase 0.F */ },
            },
            {
                id: newEventId(),
                event_name: "pat_issued",
                tenant_id: tenantId,
                user_id: userId,
                properties: { pat_id: patId, scope: "default", is_first: true },
            },
        ]));
    }

    // Always 200 to Clerk so it doesn't retry an event we have already
    // observed (idempotency is handled by the analytics-worker's
    // INSERT OR IGNORE on event_id).
    return new Response("ok", { status: 200 });
}
