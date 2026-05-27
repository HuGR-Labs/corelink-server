import * as React from "react";
import type { Locale } from "@/i18n/messages";
import { loadLocalizedMarkdown } from "@/content/load";
import { DpaStep } from "./DpaStep";

/**
 * Team invite — DPA-gated entry point.
 *
 * Per PLG framework §4 ("What got deferred out of the wizard"):
 *  - DPA acceptance is captured the first time a tenant invites a second
 *    member, not at signup. A solo tenant is its own data controller and
 *    data subject, so pre-collection of DPA at signup is legally unnecessary
 *    (LGPD Art. 9 — passive privacy notice in the footer is sufficient).
 *  - This route is the first place a tenant attempts to add another natural
 *    person to their workspace, so it is the correct trigger.
 *
 * Flow:
 *   1. Server component loads the DPA text in the user's locale, hashes it.
 *   2. If `tenant.dpa_accepted_at` is already set, render the invite form.
 *      Otherwise render `<DpaStep />` (this server component renders the
 *      DPA component for now; gating against tenant state happens in the
 *      action handler once the tenant-lookup adapter lands).
 */

async function sha256Hex(text: string): Promise<string> {
  const encoder = new TextEncoder();
  const hashBuf = await crypto.subtle.digest("SHA-256", encoder.encode(text));
  return Array.from(new Uint8Array(hashBuf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

export default async function TeamInvitePage(props: {
  params: Promise<{ locale: Locale }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;
  const text = loadLocalizedMarkdown("dpa", locale);
  const hash = await sha256Hex(text);

  // Tenant id is read from the Clerk session in production. In Phase 0 this
  // page renders the DPA-acceptance gate; the actual invite form lives in
  // the next iteration once the tenant adapter is wired.
  const tenantId = ""; // resolved server-side via auth() in a follow-up.

  return (
    <main className="mx-auto max-w-2xl p-8">
      <h1 className="text-xl font-semibold">Invite a team member</h1>
      <p className="mt-2 text-sm text-gray-600">
        Before adding a second member, accept the Data Processing Agreement —
        you become the data controller for the people you invite.
      </p>
      <DpaStep
        locale={locale}
        tenantId={tenantId}
        dpaText={text}
        dpaVersion="1.0.0"
        noticeTextHash={hash}
      />
    </main>
  );
}
