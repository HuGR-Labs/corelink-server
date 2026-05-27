import * as React from "react";
import { redirect } from "next/navigation";
import type { Locale } from "@/i18n/messages";

/**
 * Legacy onboarding entry — collapsed per Phase-0 PLG framework §4.
 *
 * Before: `/onboarding` → `/onboarding/tenant` → … 5 more wizard gates.
 * Now:    `/onboarding` → `/welcome` (single post-signup screen).
 *
 * Tenant + region + plan + PAT are provisioned server-side on the Clerk
 * `user.created` webhook (`apps/signup-worker/src/webhooks/clerk.ts`),
 * so the user lands directly on the activation screen.
 *
 * If the signed-in user already has a tenant_id claim, send them straight
 * to the dashboard.
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

  redirect(`/${locale}/welcome`);
}
