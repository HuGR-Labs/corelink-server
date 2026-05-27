/**
 * Sign-in route. Clerk SDK renders the SignIn component if configured;
 * otherwise we fall back to a static notice so the page never 500s in dev.
 *
 * Option B: override `runtime = "nodejs"` to prevent the
 * "Disallowed operation called within global scope" crash on Cloudflare
 * Workers edge runtime. @clerk/nextjs initialises internal state at module
 * scope which is not permitted in the CF Workers sandbox. Node.js runtime
 * (enabled via CF Pages `nodejs_compat` flag) is the correct choice here:
 * the auth UI pages are pure client-rendered React and have no edge-only
 * requirements. See matching fix in apps/admin-ui/src/app/sign-up/.
 */
"use client";

// Route-segment runtime override: must be nodejs so that @clerk/nextjs module-scope
// initialisation does not fire inside the CF Workers edge sandbox.
export const runtime = "nodejs";

import { SignIn } from "@clerk/nextjs";
import { useTranslations } from "next-intl";

export default function SignInPage(): React.ReactElement {
  const t = useTranslations("auth");
  const publishableKey = process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"];
  if (!publishableKey) {
    return (
      <main className="mx-auto max-w-md p-8">
        <h1 className="text-xl font-semibold">{t("signIn")}</h1>
        <p className="mt-4 text-sm text-gray-600">
          Clerk publishable key not configured. Set{" "}
          <code className="rounded bg-gray-100 px-1">
            NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY
          </code>{" "}
          to enable sign-in.
        </p>
      </main>
    );
  }
  return (
    <main className="mx-auto max-w-md p-8">
      <SignIn />
    </main>
  );
}
