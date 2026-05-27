/**
 * Sentry server-side init (Next.js server runtime: server actions, RSC, API
 * routes when running on Node / Edge runtime fallbacks).
 *
 * Same hardening posture as `sentry.client.config.ts`. We deliberately read
 * the *non-public* `SENTRY_DSN` here so the server-side DSN can differ from
 * the browser one if Gustavo decides to split projects (recommended at scale
 * but optional pre-PMF — falling back to the public DSN is acceptable).
 */

import * as Sentry from "@sentry/nextjs";

const DSN = process.env.SENTRY_DSN ?? process.env.NEXT_PUBLIC_SENTRY_DSN;

if (DSN) {
  Sentry.init({
    dsn: DSN,
    environment: process.env.SENTRY_ENVIRONMENT ?? process.env.NODE_ENV,
    release: process.env.SENTRY_RELEASE ?? process.env.NEXT_PUBLIC_SENTRY_RELEASE,

    sendDefaultPii: false,
    tracesSampleRate: 0.1,

    beforeSend(event) {
      return scrubAuthorization(event);
    },
    beforeBreadcrumb(breadcrumb) {
      if (breadcrumb.data && typeof breadcrumb.data === "object") {
        breadcrumb.data = scrubObject(breadcrumb.data as Record<string, unknown>);
      }
      return breadcrumb;
    },
  });
}

type SentryEvent = Parameters<NonNullable<Parameters<typeof Sentry.init>[0]["beforeSend"]>>[0];

const SENSITIVE_HEADER_PATTERN = /^(authorization|cookie|set-cookie|x-api-key|proxy-authorization)$/i;

function scrubObject(obj: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(obj)) {
    if (SENSITIVE_HEADER_PATTERN.test(k)) {
      out[k] = "[Filtered]";
      continue;
    }
    if (v && typeof v === "object" && !Array.isArray(v)) {
      out[k] = scrubObject(v as Record<string, unknown>);
    } else {
      out[k] = v;
    }
  }
  return out;
}

function scrubAuthorization(event: SentryEvent): SentryEvent {
  if (event.request?.headers) {
    event.request.headers = scrubObject(
      event.request.headers as unknown as Record<string, unknown>,
    ) as typeof event.request.headers;
  }
  if (event.contexts) {
    event.contexts = scrubObject(event.contexts as Record<string, unknown>) as typeof event.contexts;
  }
  return event;
}
