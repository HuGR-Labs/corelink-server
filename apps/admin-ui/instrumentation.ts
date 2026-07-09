/**
 * Next.js instrumentation hook — Sentry server/edge init (v10 migration).
 *
 * @sentry/nextjs v9 removed the webpack-based auto-loading of
 * `sentry.server.config.ts` / `sentry.edge.config.ts`. They are now loaded
 * explicitly from `register()` here, gated on `NEXT_RUNTIME`. The client init
 * moved to `instrumentation-client.ts` (see that file).
 *
 * `onRequestError` forwards Next.js server request errors (RSC, route
 * handlers, server actions) to Sentry via `captureRequestError`.
 *
 * See: https://docs.sentry.io/platforms/javascript/guides/nextjs/manual-setup/
 */
import * as Sentry from "@sentry/nextjs";
import { initSentryServer } from "./sentry.server.config";
import { initSentryEdge } from "./sentry.edge.config";

// NOTE: STATIC imports (not `await import(...)`) are deliberate. With Next 16's
// standalone output, dynamic imports here make OpenNext's copyTracedFiles fail
// ("File server/instrumentation.js does not exist"). Static imports + callable
// init functions are the OpenNext-recommended pattern.
// https://opennext.js.org/aws/common_issues
export function register() {
  if (process.env.NEXT_RUNTIME === "nodejs") {
    initSentryServer();
  }
  if (process.env.NEXT_RUNTIME === "edge") {
    initSentryEdge();
  }
}

export const onRequestError = Sentry.captureRequestError;
