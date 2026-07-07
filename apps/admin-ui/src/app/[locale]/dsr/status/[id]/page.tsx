import { isLocale, type Locale } from "@/i18n";
import { DsrStatusDetailClient } from "./DsrStatusDetailClient";

interface PageProps {
  params: Promise<{ locale: string; id: string }>;
}

/**
 * Resolve the Clerk session token at the server boundary (lazy-import so
 * tests / non-Clerk environments don't crash). `/dsr/status/[id]` is a
 * PROTECTED path (see `lib/route-matcher.ts`), so `clerkMiddleware` runs and
 * the server-side `auth()` probe resolves — same pattern as
 * `[locale]/upgrade/page.tsx`. Without the token the detail was stuck on a
 * Skeleton forever because `getDsrStatus` never ran.
 */
async function getSessionToken(): Promise<string | undefined> {
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

export default async function DsrStatusDetailPage({ params }: PageProps) {
  const { locale: rawLocale, id } = await params;
  const locale: Locale = isLocale(rawLocale) ? rawLocale : "en";
  const token = await getSessionToken();
  return (
    <DsrStatusDetailClient locale={locale} requestId={id} token={token} />
  );
}
