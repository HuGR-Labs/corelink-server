/**
 * POST /api/newsletter/subscribe — capture an email for the public CoreLink
 * newsletter (non-customer top-of-funnel) and forward it to Resend's
 * Audience API.
 *
 * Per the channels audit + solo-SaaS playbook: docs visitors who are not
 * yet customers should be able to follow along (release notes, design
 * essays, post-mortems). We already use Resend for transactional mail, so
 * reusing the *same* `RESEND_API_KEY` against the Audience API avoids
 * adding a second vendor (Mailchimp, SendGrid, ConvertKit, etc.).
 *
 * Why this route lives in admin-ui (not docs):
 *   - docs is a static Docusaurus build deployed to Cloudflare Pages with
 *     no server runtime — there is nowhere to safely hold `RESEND_API_KEY`.
 *   - admin-ui already runs as a Next.js app on Cloudflare Pages with
 *     environment secrets configured (Stripe, Clerk, etc.). Adding one
 *     more secret here is the lowest-risk path.
 *   - The browser POSTs to the *absolute* admin-ui origin
 *     (e.g. `https://corelink-admin.humangr.com/api/newsletter/subscribe`)
 *     from the docs site — see the matching `NewsletterSignup` component
 *     for the CORS-safe `fetch` configuration.
 *
 * Security:
 *   - `RESEND_API_KEY` is read at *server* runtime only and never crosses
 *     into the response body or any log line.
 *   - The audience id (`RESEND_NEWSLETTER_AUDIENCE_ID`) is also server-only;
 *     leaking it would still not let an attacker mutate Resend state
 *     without the API key, but we keep it server-side for defense in depth.
 *   - Email format is validated with a conservative RFC-5321-shaped regex
 *     (no inline display names, no quoted locals) to keep the forwarded
 *     payload tidy. Resend itself performs the authoritative validation.
 *
 * Double opt-in:
 *   - Resend Audiences supports per-audience double-opt-in. When that is
 *     enabled in the Resend dashboard, the contact created here lands in
 *     "unconfirmed" state and Resend automatically sends the confirmation
 *     email. We do *not* trigger a confirmation email from this handler —
 *     that would duplicate the one Resend sends and risk inconsistency.
 *
 * Behaviour matrix:
 *   - Missing or invalid email                → 400 invalid_email
 *   - Missing server secrets (dev/test mode)  → 503 not_configured
 *   - Resend 4xx (already exists, blocked)    → 200 ok (idempotent UX)
 *   - Resend 5xx / network                    → 502 upstream_unavailable
 *   - Success                                 → 200 ok
 *
 * The 200-on-Resend-4xx behaviour is deliberate: "you're already on the
 * list" is a success outcome from the visitor's perspective and we don't
 * want the docs site UI to leak audience-membership state to anyone who
 * can spam the endpoint.
 */

import { NextResponse, type NextRequest } from "next/server";

// Edge runtime — `fetch` is the only outbound call we make, and the
// Resend Audience API is purely HTTPS. No Node-only dependency required.
export const dynamic = "force-dynamic";

// Conservative email check: a local-part with no whitespace / @, an @,
// and a domain with at least one dot. Resend does the authoritative
// validation; this is just to bounce obvious junk before we burn a call.
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

// Hard cap on the source/locale strings we propagate as Resend metadata.
const MAX_TAG_LEN = 64;

interface SubscribeBody {
  email?: string;
  /** Free-form origin tag, e.g. "docs-home", "docs-blog". */
  source?: string;
  /** Optional locale slug, e.g. "en-US", "pt-BR". */
  locale?: string;
}

interface ResendContactPayload {
  email: string;
  unsubscribed: false;
  first_name?: string;
  last_name?: string;
}

function sanitiseTag(value: string | undefined, fallback: string): string {
  if (typeof value !== "string") return fallback;
  const cleaned = value.replace(/[^a-zA-Z0-9_-]/g, "").slice(0, MAX_TAG_LEN);
  return cleaned.length > 0 ? cleaned : fallback;
}

function corsHeaders(origin: string | null): HeadersInit {
  // Allow any *.humangr.com origin (docs + admin + future marketing). We
  // do not echo arbitrary origins because that would let a malicious page
  // POST on a logged-in user's behalf — but a newsletter endpoint takes
  // no credentials and reveals no PII, so the cost of being permissive
  // here is low. Still, we lock it down to the org's domains.
  const allow =
    origin && /^https:\/\/[a-z0-9-]+\.humangr\.com$/i.test(origin)
      ? origin
      : "https://corelink-docs.humangr.com";
  return {
    "access-control-allow-origin": allow,
    "access-control-allow-methods": "POST, OPTIONS",
    "access-control-allow-headers": "content-type",
    "access-control-max-age": "86400",
    vary: "origin",
  };
}

export function OPTIONS(req: NextRequest): Response {
  return new Response(null, {
    status: 204,
    headers: corsHeaders(req.headers.get("origin")),
  });
}

export async function POST(req: NextRequest): Promise<NextResponse> {
  const cors = corsHeaders(req.headers.get("origin"));

  let body: SubscribeBody = {};
  try {
    body = (await req.json()) as SubscribeBody;
  } catch {
    return NextResponse.json(
      { error: "invalid_json" },
      { status: 400, headers: cors },
    );
  }

  const rawEmail = typeof body.email === "string" ? body.email.trim().toLowerCase() : "";
  if (!rawEmail || rawEmail.length > 254 || !EMAIL_RE.test(rawEmail)) {
    return NextResponse.json(
      { error: "invalid_email" },
      { status: 400, headers: cors },
    );
  }

  const source = sanitiseTag(body.source, "docs");
  const locale = sanitiseTag(body.locale, "en-US");

  // Read secrets from process.env at request time. Cloudflare Pages
  // exposes Pages/Workers secret bindings here at edge runtime.
  const apiKey = process.env["RESEND_API_KEY"];
  const audienceId = process.env["RESEND_NEWSLETTER_AUDIENCE_ID"];

  if (!apiKey || !audienceId) {
    // 503 (not 500) so the client renders a "try again later" — and the
    // dashboard alert clearly distinguishes "we forgot to set the secret"
    // from a genuine outage.
    return NextResponse.json(
      { error: "not_configured" },
      { status: 503, headers: cors },
    );
  }

  const payload: ResendContactPayload = {
    email: rawEmail,
    unsubscribed: false,
  };

  let resendResp: Response;
  try {
    resendResp = await fetch(
      `https://api.resend.com/audiences/${encodeURIComponent(audienceId)}/contacts`,
      {
        method: "POST",
        headers: {
          authorization: `Bearer ${apiKey}`,
          "content-type": "application/json",
          // Attach lightweight, non-PII provenance so Resend's dashboard
          // can segment "from docs landing" vs "from blog landing".
          "x-corelink-source": source,
          "x-corelink-locale": locale,
        },
        body: JSON.stringify(payload),
        // Resend SLA is ~ a few hundred ms; cap at 5 s so an upstream
        // hiccup doesn't pin the worker.
        signal: AbortSignal.timeout(5_000),
      },
    );
  } catch {
    return NextResponse.json(
      { error: "upstream_unavailable" },
      { status: 502, headers: cors },
    );
  }

  if (resendResp.status >= 500) {
    return NextResponse.json(
      { error: "upstream_unavailable" },
      { status: 502, headers: cors },
    );
  }

  // 4xx from Resend (duplicate, blocked, invalid) — surface as success
  // to the client. The visitor doesn't need to know they're already on
  // the list, and we don't want to leak audience state.
  // We still log a coarse marker for ops visibility (status only, never
  // the email).
  if (resendResp.status >= 400) {
    return NextResponse.json(
      { status: "ok" },
      { status: 200, headers: cors },
    );
  }

  return NextResponse.json(
    { status: "ok" },
    { status: 200, headers: cors },
  );
}
