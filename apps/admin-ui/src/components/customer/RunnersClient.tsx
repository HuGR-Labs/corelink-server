// Customer-side "Runners" (W9) — ephemeral CI runners on the CoreLink cache.
//
// DATA-TRUTH (mirrors customer_d1.rs / customer-types.ts):
//   GitHub-App install .......... [live]  → `<a>/api/install/github`, the ONE
//                                           wired action here (mints signed
//                                           install-state server-side, 302 → GitHub)
//   entitlement (sku /
//     max_concurrency /
//     max_vcpu_h /
//     consumed_vcpu_h) .......... [not-wired → BE-10] → getRunnerEntitlement()
//                                           throws NotWiredError by contract, so
//                                           there is NOTHING to probe → teaching
//                                           EmptyState, NEVER a fabricated number
//   repo allowlist .............. [not-wired → BE-10] → teaching Callout
//   consumed-vCPU-h metered ..... [not-wired → BE-10] → honest "coming" framing
//
// The runner axis is a SEPARATE entitlement from the cache tier (see pricing.ts
// `runner_*` SKUs: max_concurrency + monthly vCPU-h). The value story is FLAT
// per parallel runner + unlimited minutes, warmed by your cache — the
// differentiator vs per-minute runner pricing.
//
// #1 rule for this screen: a not-wired field NEVER renders as a real number and
// we run NO dead probe against a method that throws by contract — we render a
// teaching state that names WHAT appears and WHEN (the backend WP that closes it).

"use client";

import React from "react";
import {
  Card,
  Button,
  Badge,
  Callout,
  EmptyState,
  HelpPopover,
} from "@/components/ui/linear";

// The install flow is server-resolved + cross-origin (302 → GitHub), so it must
// be a full-page navigation, not client-side routing. Mirrors the existing
// settings/runners install button, driven through the kit Button (no raw
// `<a>`-with-kit-classes, no invented primitive).
const INSTALL_HREF = "/api/install/github";

// [not-wired → BE-10] entitlement + consumed-vCPU-h + repo-allowlist reads.
// getRunnerEntitlement() throws NotWiredError by contract → we do NOT call it
// (no dead probe); we name the WP so the teaching copy stays honest + traceable.
const ENTITLEMENT_WP = "BE-10 runners entitlement read";

export function RunnersClient(): React.ReactElement {
  return (
    <div data-testid="runners-shell">
      {/* Value prop — the differentiator vs per-minute runner pricing. [copy] */}
      <Card title="Flat per parallel runner. Unlimited minutes.">
        <p className="lin-card__meta" data-testid="runners-valueprop">
          CoreLink runners are billed <strong>flat per concurrent runner</strong>,
          not per build-minute — run your CI as long as you like at a predictable
          monthly price. Every job is <strong>warmed by your cache</strong>, so
          repeat builds pull prebuilt artifacts instead of recompiling: faster for
          you, cheaper to run.{" "}
          <HelpPopover label="What is a runner, and why flat pricing?">
            A <strong>runner</strong> is an ephemeral machine that executes one CI
            job (a build/test run), then disappears. Most providers meter you
            per-minute, so long or flaky builds get expensive. CoreLink charges a
            flat monthly price per <strong>concurrent</strong> runner (how many jobs
            run at once) with unlimited minutes — the cache does the heavy lifting,
            so your bill does not grow with build time.
          </HelpPopover>
        </p>
      </Card>

      {/* Install — the ONE wired action on this screen. [live] */}
      <Card
        title="Connect GitHub"
        meta="Install the CoreLink GitHub App to dispatch CI jobs to your runners against the shared cache."
      >
        <div data-testid="runners-install">
          {/* Full-page navigation (302 → GitHub) via the kit anchor Button —
              client-side routing cannot follow the cross-origin redirect. */}
          <Button
            variant="primary"
            href={INSTALL_HREF}
            data-testid="runners-install-cta"
          >
            Install GitHub App
          </Button>
          <p className="lin-card__meta">
            You&rsquo;ll be redirected to GitHub to choose which repositories to
            grant access to. CoreLink resolves your tenant and mints a signed
            install request server-side.
          </p>
        </div>
      </Card>

      {/* Entitlement + consumption — [not-wired → BE-10]. NEVER a number, NO
          probe against a method that throws by contract. Teach instead. */}
      <Card
        title="Runner capacity"
        meta="Concurrency, monthly vCPU-hours, and consumption"
        actions={<Badge tone="neutral">Coming soon</Badge>}
      >
        <div data-testid="runners-entitlement">
          <EmptyState
            title="Your runner capacity and usage appear here once you add a runner plan"
            body="Concurrency (how many jobs run at once), your monthly vCPU-hour allowance, and consumed vCPU-hours land here once runner metering ships (BE-10). Until then we show no fabricated numbers — add a runner plan to provision capacity."
            cta={
              <Button
                variant="ghost"
                onClick={() => window.location.assign("/upgrade?plan=runner_starter")}
                data-testid="runners-upgrade-cta"
              >
                View runner plans
              </Button>
            }
          />
          <p className="lin-card__meta">
            <HelpPopover label="What are concurrency and vCPU-hours?">
              <strong>Concurrency</strong> is the maximum number of CI jobs that can
              run in parallel — a bigger runner SKU lets more of your pipeline run at
              once. A <strong>vCPU-hour</strong> is one virtual CPU running for one
              hour; your plan includes a monthly vCPU-hour allowance, and{" "}
              <strong>consumed vCPU-hours</strong> is how much of it you have used.
              Both are a separate entitlement from your cache tier.
            </HelpPopover>{" "}
            Tracked once metering ships ({ENTITLEMENT_WP}).
          </p>
        </div>
      </Card>

      {/* Repo allowlist — [not-wired → BE-10]. Teaching Callout, no dead list. */}
      <Card title="Repository allowlist">
        <div data-testid="runners-allowlist">
          <Callout tone="info">
            <strong>Allowlist coming.</strong> After you install the GitHub App,
            you&rsquo;ll manage which repositories may dispatch jobs to your runners
            here. The management UI ships with the runner entitlement backend (
            {ENTITLEMENT_WP}) — until then, repository access is set during the
            GitHub App install above.
          </Callout>
        </div>
      </Card>
    </div>
  );
}

export default RunnersClient;
