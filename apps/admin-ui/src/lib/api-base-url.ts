/**
 * The ONE resolver for the CoreLink API origin that browser-side clients
 * prefix onto a `/v1/...` path.
 *
 * WHY THIS FILE EXISTS
 *   A `/v1/...` path is NOT a route in this Next app — there is no `/v1`
 *   segment under `src/app` and no rewrite in `next.config.ts`. It is a path
 *   on the SEPARATE `corelink-api` origin. Handing a bare `/v1/...` to
 *   `fetch()` from the browser therefore resolves against the page origin,
 *   which is `humangr.com` — and the apex is a DIFFERENT application (the
 *   `hugr-site` marketing Pages project). Measured live 2026-08-03:
 *
 *     GET humangr.com/v1/consent/active           -> 200 text/html (marketing)
 *     GET humangr.com/corelink/v1/consent/active  -> 307
 *     GET corelink-api.humangr.com/v1/consent/... -> 401 (the real origin)
 *
 *   The `200 text/html` is strictly worse than a 404: the fetch RESOLVES, and
 *   the failure only surfaces later as a JSON parse error, so the calling
 *   screen reports a generic "could not load" instead of a routing fault.
 *   Note this is NOT a basePath bug — prefixing `/corelink` does not help,
 *   because `/corelink/v1/...` is not a route in this app either.
 *
 * THE LOGIC is lifted verbatim from `CustomerClient`'s private
 * `resolveBaseUrl()` (`lib/customer-client.ts`), which is the proven-in-prod
 * convention: prefer `NEXT_PUBLIC_CORELINK_API_URL` (the admin-ui prod build
 * sets it to `https://corelink-api.humangr.com`, and `NEXT_PUBLIC_*` is
 * inlined into the client bundle), else fall back to the same-origin `/api`
 * path served by the E2E mock catch-all. It lives here, in its own module, so
 * that a fourth caller does not have to paste a fourth byte-identical copy —
 * `lib/customer-client.ts`, `lib/admin-client.ts` and the customer
 * audit-visualization page each still carry their own and should be migrated
 * onto this helper once the PRs currently touching them have landed.
 *
 * The same-origin fallback carries the app's `/corelink` basePath: a bare
 * `/api` resolves against the apex marketing app, not this one.
 * `withAppBasePath` is a no-op on an absolute `https://` value, so the
 * production path is unaffected.
 */

import { withAppBasePath } from "./route-matcher";

export function resolveApiBaseUrl(): string {
  const explicit =
    typeof process !== "undefined" ? process.env?.NEXT_PUBLIC_CORELINK_API_URL : undefined;
  if (typeof window !== "undefined") return withAppBasePath(explicit ?? "/api");
  if (explicit && !explicit.startsWith("/")) return explicit;
  const port = (typeof process !== "undefined" && process.env?.PORT) || "3000";
  const relPath = withAppBasePath(explicit ?? "/api");
  return `http://127.0.0.1:${port}${relPath}`;
}
