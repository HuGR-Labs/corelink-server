import * as React from "react";
import { redirect } from "next/navigation";
import type { Locale } from "@/i18n/messages";
import { InstallOneLiner } from "@/components/InstallOneLiner";
import { WelcomeStream } from "./WelcomeStream";
import { CopyPatButton } from "./CopyPatButton";

/**
 * /welcome — the single post-signup screen (PLG framework §4 step 3).
 *
 * Server component — reads `tenant_id`, `region`, and the one-time
 * `pat_plaintext` from Clerk publicMetadata (populated by the signup-worker
 * `user.created` webhook, CTRL-CRED-001).
 *
 * Three rendering branches:
 *  1. `pat_plaintext` present: one-time reveal panel (PAT + install one-liner
 *     + CopyPatButton which triggers clearPatPlaintext server action on save)
 *  2. `tenant_id` present but no `pat_plaintext`: "already retrieved" panel
 *     with link to /customer/keys for rotation
 *  3. Neither present: redirect to /sign-up (webhook still running or session
 *     expired — user must re-authenticate)
 *
 * CTRL-CRED-001: PAT plaintext is NEVER logged, NEVER passed to client-side
 * state beyond this render, NEVER stored in localStorage/sessionStorage/cookie.
 * It is cleared from Clerk publicMetadata by `clearPatPlaintext()` in
 * `./actions.ts` when the user confirms they have saved the token.
 */
type WelcomeClaims = {
  tenant_id?: string;
  region?: string;
  /** One-time PAT plaintext set by the signup-worker. Cleared after first view. */
  pat_plaintext?: string;
};

export default async function WelcomePage(props: {
  params: Promise<{ locale: Locale }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;

  const mod = await import("@clerk/nextjs/server").catch(() => null);
  let claims: WelcomeClaims = {};
  if (mod) {
    const session = await (
      mod as {
        auth: () => Promise<{
          sessionClaims?: {
            publicMetadata?: WelcomeClaims;
          };
        }>;
      }
    ).auth();
    // Clerk v6: publicMetadata is nested under sessionClaims.publicMetadata
    claims = session.sessionClaims?.publicMetadata ?? {};
  }

  // Branch 3: no tenant provisioned yet — webhook still running or session
  // expired. Redirect to /sign-up so the user re-authenticates.
  // p95 webhook latency target ≤ 2s (acceptance §2); rare edge case.
  if (!claims.tenant_id) {
    redirect(`/${locale}/sign-up`);
  }

  const region = claims.region ?? "auto";

  // Branch 2: tenant provisioned, but PAT already retrieved (pat_plaintext
  // was cleared after first visit by clearPatPlaintext server action).
  if (!claims.pat_plaintext) {
    return (
      <main
        className="mx-auto max-w-2xl p-8"
        data-testid="welcome-already-retrieved"
      >
        <h1 className="text-2xl font-semibold">Welcome to CoreLink</h1>
        <p className="mt-2 text-sm text-gray-600">
          Your tenant: <code>{claims.tenant_id}</code> | region:{" "}
          <code>{region}</code>
        </p>
        <div
          className="mt-6 rounded border border-yellow-300 bg-yellow-50 p-4"
          role="status"
          data-testid="already-retrieved-notice"
        >
          <p className="text-sm">
            Your token was already retrieved. Rotate it from{" "}
            <a
              href="/customer/keys"
              className="underline"
              data-testid="rotate-keys-link"
            >
              /customer/keys
            </a>{" "}
            if you need a new one.
          </p>
        </div>
        <div className="mt-4">
          <a
            href="/customer"
            className="text-sm underline"
            data-testid="go-to-customer-link"
          >
            Go to dashboard
          </a>
        </div>
      </main>
    );
  }

  // Branch 1: first visit — pat_plaintext present, render one-time reveal panel.
  const pat = claims.pat_plaintext;

  return (
    <main className="mx-auto max-w-2xl p-8" data-testid="welcome-root">
      <h1 className="text-2xl font-semibold">Welcome to CoreLink</h1>
      <p className="mt-2 text-sm text-gray-600">
        Your tenant: <code>{claims.tenant_id}</code> | region:{" "}
        <code>{region}</code>
      </p>

      {/* One-time PAT reveal */}
      <section className="mt-6" data-testid="pat-reveal-section">
        <div
          className="rounded border border-amber-300 bg-amber-50 p-3 text-sm"
          role="alert"
          data-testid="pat-warning"
        >
          This is your personal access token. It is shown{" "}
          <strong>once</strong>. Store it in a secret manager.
        </div>

        <div className="mt-4">
          <CopyPatButton pat={pat} />
        </div>
      </section>

      {/* Install one-liner */}
      <section className="mt-8" data-testid="install-section">
        <h2 className="text-lg font-medium">Install the CLI</h2>
        <div className="mt-2">
          <InstallOneLiner token={pat} region={region} />
        </div>
      </section>

      {/* Verify step */}
      <section className="mt-8" data-testid="verify-section">
        <h2 className="text-lg font-medium">Then verify</h2>
        <pre className="mt-2 rounded bg-gray-100 p-3 text-sm">
          <code>$ corelink whoami</code>
        </pre>
      </section>

      {/* Activation status stream */}
      <section className="mt-8" data-testid="activation-section">
        <h2 className="text-lg font-medium">Activation status</h2>
        <WelcomeStream />
        <p className="mt-2 text-xs text-gray-500">
          After the CLI authenticates, run{" "}
          <code>corelink bazel-init</code> in your repo, then{" "}
          <code>bazel build //...</code> twice — the second build should
          report cache hits.
        </p>
      </section>
    </main>
  );
}
