/**
 * Sign-up route — Clerk widget is dynamic-imported with `ssr: false` so the
 * `@clerk/nextjs` module never loads during the edge-runtime SSR pass. Clerk
 * initialises state at module scope, which Cloudflare Workers' edge sandbox
 * rejects ("Disallowed operation called within global scope"). Loading the
 * widget only after hydration sidesteps the constraint while keeping this
 * route on the Edge runtime (required by `@cloudflare/next-on-pages`).
 */
"use client";

import dynamic from "next/dynamic";
import { useEffect } from "react";
import { useTranslations } from "next-intl";
import { track } from "@/lib/analytics";


const SignUp = dynamic(
  () => import("@clerk/nextjs").then((m) => m.SignUp),
  { ssr: false },
);

export default function SignUpPage(): React.ReactElement {
  const t = useTranslations("auth");
  const publishableKey = process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"];

  // Phase 0.G — fire `signup_started` on first mount.
  // PLG §7.1: this is the moment the Clerk widget mounts; the server-side
  // `signup_completed` lands later via the signup-worker Clerk webhook.
  // Captured even before consent (no PII, session-only correlator).
  useEffect(() => {
    track("signup_started", {
      properties: {
        auth_widget: "clerk",
      },
    });
  }, []);

  if (!publishableKey) {
    return (
      <main className="mx-auto max-w-md p-8">
        <h1 className="text-xl font-semibold">{t("signUp")}</h1>
        <p className="mt-4 text-sm text-gray-600">
          Clerk publishable key not configured.
        </p>
      </main>
    );
  }
  return (
    <main className="mx-auto max-w-md p-8">
      {/*
       * forceRedirectUrl: always land on /en/welcome after Clerk completes
       * sign-up, regardless of the Clerk dashboard "redirect URL" setting.
       * fallbackRedirectUrl: safety net if Clerk ignores forceRedirectUrl
       * (e.g. email-verification flows that redirect independently).
       * /en/welcome is used because next-intl requires the locale prefix;
       * the default locale is "en" (src/i18n/request.ts).
       */}
      <SignUp
        forceRedirectUrl="/en/welcome"
        fallbackRedirectUrl="/en/welcome"
      />
    </main>
  );
}
