import * as React from "react";
import { redirect } from "next/navigation";
import type { Locale } from "@/i18n/messages";

/**
 * Onboarding entry point.
 *
 * If the signed-in user already has a tenant_id in their Clerk membership
 * claim, redirect to /dashboard. Otherwise start the wizard at /tenant.
 */
export default async function OnboardingEntryPage(props: {
  params: Promise<{ locale: Locale }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;

  // Lazy import so tests do not require Clerk to be configured.
  const mod = await import("@clerk/nextjs/server").catch(() => null);
  if (mod) {
    const session = await (
      mod as { auth: () => Promise<{ sessionClaims?: { tenant_id?: string } }> }
    ).auth();
    if (session.sessionClaims?.tenant_id) {
      redirect(`/${locale}/dashboard`);
    }
  }

  redirect(`/${locale}/onboarding/tenant`);
}
