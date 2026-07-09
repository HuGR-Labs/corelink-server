/**
 * Sentry client-side init (browser bundle).
 *
 * Loaded automatically by `@sentry/nextjs` when the application boots in the
 * browser. We keep this surface deliberately conservative for solo-founder
 * pre-PMF operation:
 *
 *   - `sendDefaultPii: false`     — never auto-attach IP, cookies, user data.
 *   - Authorization scrub         — strip auth headers from breadcrumbs/events.
 *   - `tracesSampleRate: 0.1`     — 10 % traces to stay well under the 5 k
 *                                   errors/mo Sentry Developer Free tier.
 *   - `replaysSession`/`replays`  — disabled by default (would blow the free
 *                                   tier instantly and capture PII).
 *
 * DSN sourcing:
 *   `NEXT_PUBLIC_SENTRY_DSN` is the canonical env var. When unset we skip
 *   `Sentry.init` entirely so local dev / CI / preview builds do not noisily
 *   POST to an unconfigured ingest endpoint. Gustavo wires the real DSN as a
 *   Cloudflare Pages secret per `specs/_audits/2026-05-27-sentry-setup-runbook.md`.
 *
 * Release tagging:
 *   `NEXT_PUBLIC_SENTRY_RELEASE` is populated at build time from the git SHA
 *   (Cloudflare Pages exports `CF_PAGES_COMMIT_SHA`; Vercel exports
 *   `VERCEL_GIT_COMMIT_SHA`). When neither is present we omit the tag and
 *   Sentry falls back to its own release inference.
 */

import * as Sentry from "@sentry/nextjs";
import { scrubObject, scrubSentryEvent } from "@/lib/sentry-scrub";

const DSN = process.env.NEXT_PUBLIC_SENTRY_DSN;

if (DSN) {
  Sentry.init({
    dsn: DSN,
    environment: process.env.NEXT_PUBLIC_SENTRY_ENVIRONMENT ?? process.env.NODE_ENV,
    release: process.env.NEXT_PUBLIC_SENTRY_RELEASE,

    // PII hardening — see file header.
    sendDefaultPii: false,

    // Sampling — see file header.
    tracesSampleRate: 0.1,

    // Replay disabled pre-PMF (free-tier eater + PII surface).
    replaysSessionSampleRate: 0,
    replaysOnErrorSampleRate: 0,

    // Scrub PII/secrets from message + exception bodies + extra/contexts
    // VALUES (not just header keys) from every event before transmit.
    beforeSend(event) {
      return scrubSentryEvent(event);
    },
    beforeSendTransaction(event) {
      return scrubSentryEvent(event);
    },
    beforeBreadcrumb(breadcrumb) {
      if (breadcrumb.data && typeof breadcrumb.data === "object") {
        breadcrumb.data = scrubObject(breadcrumb.data as Record<string, unknown>);
      }
      return breadcrumb;
    },
  });
}
