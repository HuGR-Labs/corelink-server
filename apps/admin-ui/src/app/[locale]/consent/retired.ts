// ⛔ THE CONSENT UI IS RETIRED UNTIL A BACKEND EXISTS.
//
// WHY
//   The four screens under `[locale]/consent/*` call five endpoints that
//   nothing in this repo serves — `/v1/consent/active`, `/v1/consent/grant`,
//   `/v1/consent/{id}/withdraw`, `/v1/consent/history`, `/v1/subprocessors`.
//   The 55 `.route("…")` mounts across `crates/` contain zero consent paths,
//   `worker/src/index.ts` has no consent branch, and `corelink-privacy` — the
//   crate that owns the consent ledger — is not a dependency of
//   `corelink-container` or of any Worker, so it is not compiled into anything
//   deployed. See `@/lib/consent-api` for the full measurement.
//
//   The severe case was `/[locale]/consent/new`: it was on the middleware
//   PUBLIC allowlist (`lib/route-matcher.ts`), so an ANONYMOUS visitor got a
//   200 "Grant consent — CoreLink" page that walked them through legal basis,
//   data categories, retention period, sub-processors and withdrawal method,
//   stamped `Captured at: <ISO timestamp>`, and persisted NOTHING. On top of
//   that the notice text it asked people to consent to is a hardcoded STUB
//   (`STUB_NOTICE_TEXT` in `new/page.tsx`). Soliciting a GDPR/LGPD consent
//   record that is never stored is worse than having no consent surface at
//   all: it manufactures a false compliance artifact for the visitor and a
//   false audit trail for us.
//
// MECHANISM — `notFound()`, not deletion and not an auth gate.
//   * Deletion would destroy real work; the backend is a planned WP.
//   * An auth gate only hides the defect from anonymous visitors; a signed-in
//     customer would still be invited to grant a consent that goes nowhere.
//     "Gated but broken" is not materially better than "public but broken"
//     once the user is INSIDE — the lie is the same, the audience is smaller.
//   * `notFound()` makes all four routes indistinguishable from routes that do
//     not exist, uniformly, for anonymous and authenticated visitors alike.
//
//   The middleware allowlist entry for `/consent/new` is deliberately LEFT IN
//   PLACE (`lib/route-matcher.ts`): retirement lives in exactly ONE place, so
//   the reversal is one line and cannot half-apply. With the entry intact an
//   anonymous request is not sent on a pointless Clerk round-trip — it is
//   answered directly with 404.
//
// ⚠️ WHOEVER BUILDS THE BACKEND: the client and the crate DISAGREE on the
//   endpoint shape. See the `CONSENT_*` block in `tools/openapi/src/lib.rs`
//   for the side-by-side. Reconcile there BEFORE writing handlers, or you will
//   implement a surface neither side calls.
//
// ONE-LINE REVERSAL
//   Flip the constant below to `false`. Nothing else changes; every component,
//   page, route, test and translation string is preserved untouched.

import { notFound } from "next/navigation";

/**
 * Master kill-switch for the `[locale]/consent/*` surface.
 *
 * `true`  → all four consent routes answer 404 (current state: no backend).
 * `false` → the flow is live again (set this the day the handlers land).
 */
export const CONSENT_UI_RETIRED = true;

/**
 * Guard called first in every consent page component.
 *
 * When the surface is retired this calls Next's {@link notFound}, which throws
 * the `NEXT_HTTP_ERROR_FALLBACK;404` control-flow error — so the page body
 * never runs, no client island mounts, and the response is a 404. That also
 * means the consent screens can never re-trigger the `useAuth()`-outside-
 * `<ClerkProvider>` throw: they sit outside `[locale]/(authenticated)`, the
 * only place a provider is mounted, and now they do not render at all.
 */
export function assertConsentUiEnabled(): void {
  if (CONSENT_UI_RETIRED) {
    notFound();
  }
}
