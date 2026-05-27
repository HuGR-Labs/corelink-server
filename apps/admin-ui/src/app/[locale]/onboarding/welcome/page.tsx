import * as React from "react";
import { redirect } from "next/navigation";
import type { Locale } from "@/i18n/messages";

/**
 * Thin redirect kept under `/onboarding/welcome` for compatibility with any
 * external link that still points at the legacy wizard's terminal step.
 * The canonical post-signup screen lives at `/{locale}/welcome`.
 */
export default async function OnboardingWelcomeRedirect(props: {
  params: Promise<{ locale: Locale }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;
  redirect(`/${locale}/welcome`);
}
