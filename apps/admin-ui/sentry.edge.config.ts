/**
 * Sentry edge init (Next.js Edge runtime: middleware + edge API routes).
 *
 * Edge runtime is a constrained subset — Sentry's edge bundle avoids any
 * Node-specific APIs. We keep the same scrub-and-sample profile as the
 * browser/server configs: the shared `scrubSentryEvent` redacts PII/secrets
 * from message + exception bodies + extra/contexts VALUES (not just header
 * keys) before any event leaves the process. See `@/lib/sentry-scrub`.
 */

import * as Sentry from "@sentry/nextjs";
import { scrubObject, scrubSentryEvent } from "@/lib/sentry-scrub";

const DSN = process.env.SENTRY_DSN ?? process.env.NEXT_PUBLIC_SENTRY_DSN;

if (DSN) {
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
