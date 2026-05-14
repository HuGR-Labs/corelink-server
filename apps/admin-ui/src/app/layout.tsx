import type { Metadata } from "next";
import { headers } from "next/headers";
import { NextIntlClientProvider } from "next-intl";
import { getLocale, getMessages } from "next-intl/server";
import { ClerkProvider } from "@clerk/nextjs";
import "./globals.css";

export const metadata: Metadata = {
  title: "CoreLink Admin",
  description: "Shared content-addressable cache for builds, packages, and ML.",
};

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

  const publishableKey = process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"];
  const body = (
    <html lang={locale} suppressHydrationWarning>
      <body data-nonce={nonce ?? ""}>
        <NextIntlClientProvider locale={locale} messages={messages}>
          {children}
        </NextIntlClientProvider>
      </body>
    </html>
  );

  // Wrap with ClerkProvider only when a publishable key is configured. This
  // keeps `pnpm build` and local boot working without Clerk credentials.
  if (!publishableKey) return body;
  return <ClerkProvider>{body}</ClerkProvider>;
}
