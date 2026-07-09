/**
 * Sentry server-side init (Next.js server runtime: server actions, RSC, API
 * routes when running on Node / Edge runtime fallbacks).
 *
 * Same hardening posture as `sentry.client.config.ts`. We deliberately read
 * the *non-public* `SENTRY_DSN` here so the server-side DSN can differ from
 * the browser one if Gustavo decides to split projects (recommended at scale
 * but optional pre-PMF — falling back to the public DSN is acceptable).
 *
 * The shared `scrubSentryEvent` redacts PII/secrets from message + exception
 * bodies + extra/contexts VALUES (not just header keys) before any event
 * leaves the process. See `@/lib/sentry-scrub`.
 */

import * as Sentry from "@sentry/nextjs";
import { scrubObject, scrubSentryEvent } from "@/lib/sentry-scrub";

/**
 * Initialise Sentry for the Node server runtime.
 *
 * Exported as a function (rather than running on import) so `instrumentation.ts`
 * can call it via a STATIC import. Dynamic `await import()` here breaks OpenNext's
 * Next 16 standalone tracing (`server/instrumentation.js` is not emitted where the
 * adapter expects it → copyTracedFiles throws). See instrumentation.ts.
 */
export function initSentryServer(): void {
  const DSN = process.env.SENTRY_DSN ?? process.env.NEXT_PUBLIC_SENTRY_DSN;
  if (!DSN) return;
  Sentry.init({
    dsn: DSN,
    environment: process.env.SENTRY_ENVIRONMENT ?? process.env.NODE_ENV,
    release: process.env.SENTRY_RELEASE ?? process.env.NEXT_PUBLIC_SENTRY_RELEASE,

    sendDefaultPii: false,
    tracesSampleRate: 0.1,

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
