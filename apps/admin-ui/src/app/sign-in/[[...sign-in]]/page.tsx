/**
 * Sign-in route — Clerk widget is dynamic-imported with `ssr: false` so the
 * `@clerk/nextjs` module never loads during the edge-runtime SSR pass. Clerk
 * initialises state at module scope, which Cloudflare Workers' edge sandbox
 * rejects ("Disallowed operation called within global scope"). Loading the
 * widget only after hydration sidesteps the constraint while keeping this
 * route on the Edge runtime (required by `@cloudflare/next-on-pages`).
 */
"use client";

import dynamic from "next/dynamic";
import { useTranslations } from "next-intl";


const SignIn = dynamic(
  () => import("@clerk/nextjs").then((m) => m.SignIn),
  { ssr: false },
);

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
