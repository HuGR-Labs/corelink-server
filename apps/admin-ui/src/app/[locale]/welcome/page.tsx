import * as React from "react";
import { redirect } from "next/navigation";
import type { Locale } from "@/i18n/messages";
import { InstallOneLiner } from "@/components/InstallOneLiner";
import { WelcomeStream } from "./WelcomeStream";

/**
 * /welcome — the single post-signup screen (PLG framework §4 step 3).
 *
 * Server component — reads `tenant_id`, `region`, and the freshly-issued
 * PAT plaintext from the Clerk session payload populated by the
 * `signup-worker` `user.created` handler. The PAT is shown **once** here
 * and never persisted client-side beyond the lifetime of this render
 * (CTRL-CRED-001).
 *
 * If session claims are missing (e.g. webhook still running), redirect
 * back to `/sign-up` so the user re-authenticates and the webhook gets
 * a second chance to finish provisioning.
 */
type WelcomeClaims = {
  tenant_id?: string;
  region?: string;
  /** One-time PAT plaintext set by the signup-worker. Cleared on next session. */
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
      mod as { auth: () => Promise<{ sessionClaims?: WelcomeClaims }> }
    ).auth();
    claims = session.sessionClaims ?? {};
  }

  // If auto-provision has not finished, the session won't carry the
  // tenant claim yet. The signup-worker webhook is the authoritative
  // writer of these fields, so we bounce the user back to /sign-up to
  // poll the session again on next page render. p95 webhook latency
  // target ≤ 2s (acceptance §2).
  if (!claims.tenant_id) {
    redirect(`/${locale}/sign-up`);
  }

  const region = claims.region ?? "auto";
  const token = claims.pat_plaintext ?? "ct_pending";

  return (
    <main className="mx-auto max-w-2xl p-8" data-testid="welcome-root">
      <h1 className="text-2xl font-semibold">Your CoreLink cache is ready</h1>
      <p className="mt-2 text-sm text-gray-600">
        One copy-paste below installs the CLI, points it at your nearest region
        (<code>{region}</code>), and runs <code>corelink ping</code>. The status
        below updates live as the system sees your first cache call.
      </p>

      <section className="mt-6">
        <h2 className="text-lg font-medium">1. Install + authenticate</h2>
        <InstallOneLiner token={token} region={region} />
        <p className="mt-2 text-xs text-gray-500">
          This token is shown once. Store it securely; we cannot retrieve it
          again.
        </p>
      </section>

      <section className="mt-8">
        <h2 className="text-lg font-medium">2. Activation status</h2>
        <WelcomeStream />
        <p className="mt-2 text-xs text-gray-500">
          After the CLI authenticates, run <code>corelink bazel-init</code> in
          your repo, then <code>bazel build //...</code> twice — the second
          build should report cache hits.
        </p>
      </section>
    </main>
  );
}
