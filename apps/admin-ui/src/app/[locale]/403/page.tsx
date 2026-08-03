// WI-S16-005 — canonical 403 page for admin scope failures.

import React from "react";
import { PublicShell } from "@/components/public/PublicShell";
import { Callout } from "@/components/ui/linear";

/**
 * `searchParams` is a PROMISE in Next 16 (async request APIs) — typing it as a
 * plain object made `searchParams?.reason` read a property off a Promise, so it
 * was `undefined` on every request and every 403 rendered the generic fallback
 * reason instead of the specific one the caller passed. Repeated params are
 * possible on the wire, so the array shape is normalized to its first value.
 */
export interface ForbiddenPageProps {
  searchParams?: Promise<{ reason?: string | string[] }>;
}

export default async function ForbiddenPage({
  searchParams,
}: ForbiddenPageProps): Promise<React.ReactElement> {
  const params = (await searchParams) ?? {};
  const rawReason = Array.isArray(params.reason)
    ? params.reason[0]
    : params.reason;
  const reason = rawReason?.trim() ? rawReason : "operator role required";
  return (
    <PublicShell width="prose">
      <section aria-labelledby="forbidden-heading" className="py-10">
        <p className="mb-2 font-mono text-xs font-[510] uppercase tracking-[0.05em] text-[var(--t3)]">
          403
        </p>
        <h1
          id="forbidden-heading"
          className="text-2xl font-[560] tracking-[-0.022em] text-[var(--t1)]"
        >
          Forbidden
        </h1>
        <p className="mt-2 text-sm text-[var(--t2)]">{reason}</p>
        <div className="mt-6">
          <Callout tone="warn">
            Admin pages require the{" "}
            <code className="rounded-[var(--r-chip)] bg-[rgba(255,255,255,0.06)] px-1.5 py-0.5 font-mono text-[0.9em] text-[var(--t1)]">
              corelink-admin
            </code>{" "}
            Clerk org role. If you believe you should have access, contact your
            security lead.
          </Callout>
        </div>
      </section>
    </PublicShell>
  );
}
