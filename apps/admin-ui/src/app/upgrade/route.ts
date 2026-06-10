/**
 * GET /upgrade — locale-less forwarder for the public pricing CTAs (#49).
 *
 * The docs pricing page links every paid SKU at
 * `https://corelink-app.humangr.com/upgrade?plan=<tier>` (see
 * `apps/docs/src/pages/pricing.tsx` `ctaForTier`) — with NO locale
 * prefix, because the docs site is locale-unaware. next-intl pages live
 * under `/[locale]/…`, so this handler 307s to the canonical
 * `/<DEFAULT_LOCALE>/upgrade?plan=<tier>` page. Hardcoding the default
 * locale is the established convention for locale-less public entry
 * points (the `/` landing page links `/en/pricing`; `/sign-up` force-
 * redirects to `/en/welcome` — both per `DEFAULT_LOCALE = "en"` in
 * src/i18n/request.ts).
 *
 * The `plan` value is normalized HERE as well as in the page
 * (solo/starter/team/pro/max, invalid/missing → `pro`) so the redirect
 * target never carries junk query values. 307 (temporary) keeps the
 * door open for cookie/Accept-Language locale resolution later without
 * browsers having cached a permanent answer.
 *
 * Auth is NOT checked here — the `/[locale]/upgrade` page owns the
 * signed-out → `/sign-in?redirect_url=…` round-trip so the post-sign-in
 * return lands on the locale-prefixed page directly.
 */

import { NextResponse, type NextRequest } from "next/server";
import { DEFAULT_LOCALE } from "@/i18n/request";
import { normalizeCheckoutTier } from "@/lib/pricing";

export const dynamic = "force-dynamic";

export function GET(req: NextRequest): NextResponse {
  const tier = normalizeCheckoutTier(
    new URL(req.url).searchParams.getAll("plan"),
  );
  return NextResponse.redirect(
    new URL(`/${DEFAULT_LOCALE}/upgrade?plan=${tier}`, req.url),
    { status: 307 },
  );
}
