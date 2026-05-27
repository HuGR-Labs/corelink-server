/**
 * Option B: override `runtime = "nodejs"` on this route to prevent the
 * "Disallowed operation called within global scope" crash on Cloudflare
 * Workers edge runtime. @clerk/nextjs initialises internal state at module
 * scope (e.g. `createClerkClient` config reads) which is not legal in the
 * CF Workers / Next.js edge runtime. Switching this route to Node.js runtime
 * (supported via CF Pages `nodejs_compat`) is the correct fix because the
 * auth UI routes carry no edge-only requirements; they render a pure React
 * component tree. Option A (lazy import) was considered but would require
 * wrapping every Clerk component in a dynamic() call and loses RSC benefits
 * without any upside for a purely client-rendered page.
 *
 * Override the `runtime = "edge"` exported by apps/admin-ui/src/app/layout.tsx.
 * Cloudflare Pages supports Node.js compat via the `nodejs_compat` compatibility flag.
 */
"use client";

// Route-segment runtime override: must be nodejs so that @clerk/nextjs module-scope
// initialisation does not fire inside the CF Workers edge sandbox.
export const runtime = "nodejs";

import { SignUp } from "@clerk/nextjs";
import { useEffect } from "react";
import { useTranslations } from "next-intl";
import { track } from "@/lib/analytics";

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
      <SignUp />
    </main>
  );
}
