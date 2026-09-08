import { isLocale, type Locale } from "@/i18n";
import { DsrStatusListClient } from "./DsrStatusListClient";

interface PageProps {
  params: Promise<{ locale: string }>;
}

/**
 * Resolve the Clerk session token at the server boundary (lazy-import so
 * tests / non-Clerk environments don't crash). `/dsr/status` is a PROTECTED
 * path (see `lib/route-matcher.ts`), so `clerkMiddleware` runs for it and the
 * server-side `auth()` probe resolves here — the same pattern as
 * `[locale]/upgrade/page.tsx` + `app/api/checkout/session/route.ts`. The token
 * is handed to the client component so `dsr-client`'s fetches carry
 * `Authorization: Bearer <token>` (without it the list was permanently empty).
 */
async function getSessionToken(): Promise<string | undefined> {
  // The legacy Playwright lane uses a synthetic cookie rather than Clerk FAPI.
  // Provide only a test-mode token so the status screen can exercise its real
  // list/detail fetch; the production branch remains Clerk-only and therefore
  // cannot be unlocked by an env var in a production process.
  if (
    process.env["NEXT_PUBLIC_E2E_TEST_MODE"] === "1" &&
    process.env.NODE_ENV !== "production"
  ) {
    return "e2e-dsr-session";
  }
  const mod = await import("@clerk/nextjs/server").catch(() => null);
  if (!mod) return undefined;
  try {
    const session = await (
      mod as { auth: () => Promise<{ getToken: () => Promise<string | null> }> }
    ).auth();
    return (await session.getToken()) ?? undefined;
  } catch {
    return undefined;
  }
}

export default async function DsrStatusListPage({ params }: PageProps) {
  const { locale: rawLocale } = await params;
  const locale: Locale = isLocale(rawLocale) ? rawLocale : "en";
  const token = await getSessionToken();
  return <DsrStatusListClient locale={locale} token={token} />;
}
