import type { Metadata } from "next";
import { headers } from "next/headers";
import { NextIntlClientProvider } from "next-intl";
import { getLocale, getMessages } from "next-intl/server";
import { PlausibleScript } from "@/components/analytics/PlausibleScript";
import "./globals.css";

// NOTE: `ClerkProvider` is intentionally NOT imported at module scope.
// On Cloudflare Workers edge runtime, `@clerk/nextjs` runs initialisation
// in module-scope code that is rejected by the sandbox
// ("Disallowed operation called within global scope"). Pages that need
// the Clerk React context (useUser/useAuth/etc) wrap themselves in a
// client-side `ClerkProvider` from their own layout in
// `src/app/(authenticated)/`. The SignIn/SignUp widgets used by the
// /sign-in and /sign-up routes do not require an outer ClerkProvider —
// they use the publishable key directly.

export const metadata: Metadata = {
  title: "CoreLink Admin",
  description: "Shared content-addressable cache for builds, packages, and ML.",
};

// Cloudflare Pages Edge Runtime — required by @cloudflare/next-on-pages.
// All child routes inherit this unless they explicitly opt out.

// next-intl uses `headers()` for locale detection which is dynamic. Static
// rendering optimization (via `setRequestLocale` + per-locale segments) is
// scoped to WI-S16-006 (privacy/sub-processors pages + locale switcher).
export const dynamic = "force-dynamic";

export default async function RootLayout({
  children,
}: {
  children: React.ReactNode;
}): Promise<React.ReactElement> {
  const locale = await getLocale();
  const messages = await getMessages();
  // Read the per-request nonce injected by middleware. Available for any
  // <Script> child that needs it.
  const h = await headers();
  const nonce = h.get("x-nonce") ?? undefined;

  return (
    <html lang={locale} suppressHydrationWarning>
      <body data-nonce={nonce ?? ""}>
        <NextIntlClientProvider locale={locale} messages={messages}>
          {children}
          {/* Phase 0.G — Plausible install (gated by analytics consent cookie). */}
          <PlausibleScript />
        </NextIntlClientProvider>
      </body>
    </html>
  );
}
