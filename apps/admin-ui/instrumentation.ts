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

export async function register() {
  if (process.env.NEXT_RUNTIME === "nodejs") {
    await import("./sentry.server.config");
  }
  if (process.env.NEXT_RUNTIME === "edge") {
    await import("./sentry.edge.config");
  }
}

export const onRequestError = Sentry.captureRequestError;
