// Customer-side "Runners" (W9) — ephemeral CI runners on the CoreLink cache.
//
// DATA-TRUTH (mirrors customer_d1.rs / customer-types.ts):
//   GitHub-App install .......... [live]  → `/api/install/github`, a full-page
//                                           302 → GitHub (mints signed install
//                                           state server-side)
//   entitlement (sku /
//     max_concurrency /
//     max_vcpu_h /
//     consumed_vcpu_h /
//     install_status /
//     repo_allowlist) ........... [live → BE-10] → getRunnerEntitlement(): the
//                                           backend is now wired, so we read real
//                                           capacity + allowlist and render them.
//   recent runs ................. [live → BE-10] → listRunnerRuns(): the surface
//                                           exists but the history table is empty
//                                           server-side today, so the list is an
//                                           HONEST empty (EmptyState), never faked.
//
// The runner axis is a SEPARATE entitlement from the cache tier (see pricing.ts
// `runner_*` SKUs: max_concurrency + monthly vCPU-h). The value story is FLAT
// per parallel runner + unlimited minutes, warmed by your cache — the
// differentiator vs per-minute runner pricing.
//
// #1 rule for this screen: no fabricated numbers. A tenant with no runner plan
// shows the install/upgrade path; an empty runs history shows an EmptyState.

"use client";

import React from "react";
import { useCustomerClient } from "@/lib/use-customer-client";
import type {
  CustomerRunnerEntitlement,
  CustomerRunnerRun,
} from "@/lib/customer-types";
import {
  Badge,
  Button,
  Callout,
  Card,
  EmptyState,
  Gauge,
  HelpPopover,
  InlineError,
  Skeleton,
  Stat,
} from "@/components/ui/linear";

// The install flow is server-resolved + cross-origin (302 → GitHub), so it must
// be a full-page navigation, not client-side routing. Driven through the kit
// Button anchor form (no raw `<a>`-with-kit-classes, no invented primitive).
const INSTALL_HREF = "/api/install/github";

// Where "View runner plans" points — the runner SKUs live on the upgrade path.
const RUNNER_PLANS_HREF = "/upgrade?plan=runner_starter";

/** Humanize a runner SKU id (e.g. "runner_pro" → "Runner Pro"); null → none. */
function humanizeSku(sku: string | null): string {
  if (sku == null || sku.trim() === "") return "No runners plan";
  return sku
    .split("_")
    .map((w) => (w.length > 0 ? w.charAt(0).toUpperCase() + w.slice(1) : w))
    .join(" ");
}

/** Badge label + tone for a run status. */
const RUN_STATUS_META: Record<
  CustomerRunnerRun["status"],
  { label: string; tone: "neutral" | "success" | "warn" | "danger" }
> = {
  queued: { label: "Queued", tone: "neutral" },
  running: { label: "Running", tone: "warn" },
  success: { label: "Success", tone: "success" },
  failed: { label: "Failed", tone: "danger" },
  cancelled: { label: "Cancelled", tone: "neutral" },
};

function formatStartedAt(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso.slice(0, 16);
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Human duration for a run; null (still running / unknown) → em dash. */
function formatDuration(seconds: number | null): string {
  if (seconds == null) return "—";
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rem = s % 60;
  if (m < 60) return rem > 0 ? `${m}m ${rem}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const remM = m % 60;
  return remM > 0 ? `${h}h ${remM}m` : `${h}h`;
}

export function RunnersClient(): React.ReactElement {
  const client = useCustomerClient();
  const [entitlement, setEntitlement] =
    React.useState<CustomerRunnerEntitlement | null>(null);
  const [runs, setRuns] = React.useState<CustomerRunnerRun[] | null>(null);
  const [err, setErr] = React.useState<unknown>(null);
  const [reloadKey, setReloadKey] = React.useState(0);

  React.useEffect(() => {
    let alive = true;
    setEntitlement(null);
    setRuns(null);
    setErr(null);
    // Entitlement is the load-bearing fetch; the runs history is a best-effort
    // read — a runs failure must NOT blank the capacity view, so it degrades to
    // an empty list (which honestly renders the "no runs yet" EmptyState).
    Promise.all([
      client.getRunnerEntitlement(),
      client.listRunnerRuns().then(
        (r) => r.runs,
        () => [] as CustomerRunnerRun[],
      ),
    ])
      .then(([e, r]) => {
        if (!alive) return;
        setEntitlement(e);
        setRuns(r);
      })
      .catch((e: unknown) => alive && setErr(e));
    return () => {
      alive = false;
    };
  }, [client, reloadKey]);

  if (err) {
    return (
      <div data-testid="runners-error">
        <InlineError error={err} onRetry={() => setReloadKey((k) => k + 1)} />
      </div>
    );
  }

  if (!entitlement) {
    return (
      <div
        data-testid="runners-loading"
        aria-busy="true"
        aria-label="Loading your runners"
      >
        <Card title="Runner capacity">
          <Skeleton rows={4} />
        </Card>
      </div>
    );
  }

  const isEntitled =
    entitlement.sku != null &&
    entitlement.sku.trim() !== "" &&
    entitlement.install_status === "installed";
  const hasVcpuAllowance = entitlement.max_vcpu_h > 0;
  const allowlist = entitlement.repo_allowlist;
  const runList = runs ?? [];

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

      {/* Runner capacity — [live] BE-10. Real plan / concurrency / vCPU-hours. */}
      <Card
        title="Runner capacity"
        meta="Your runner plan, concurrency, and monthly vCPU-hours"
        className="lin-mt-lg"
        actions={
          <Badge tone={isEntitled ? "success" : "neutral"} dot>
            {entitlement.install_status === "installed"
              ? "GitHub App installed"
              : "Not installed"}
          </Badge>
        }
      >
        <div data-testid="runners-entitlement">
          <div data-testid="runners-plan">
            <span className="lin-card__meta">Plan</span>{" "}
            <Badge tone={isEntitled ? "success" : "neutral"}>
              {humanizeSku(entitlement.sku)}
            </Badge>
          </div>

          <div className="lin-mt" data-testid="runners-concurrency">
            <Stat
              label="Concurrency"
              value={entitlement.max_concurrency.toLocaleString()}
              sub="Maximum CI jobs that run in parallel"
            />
          </div>

          {/* vCPU-hours — real Gauge when a plan grants an allowance; otherwise
              a raw consumed Stat (never a gauge against a fabricated max). */}
          <div className="lin-mt" data-testid="runners-vcpu">
            {hasVcpuAllowance ? (
              <Gauge
                label="vCPU-hours"
                value={entitlement.consumed_vcpu_h}
                max={entitlement.max_vcpu_h}
                unit="vCPU-h"
                hint={`${Math.round(
                  (entitlement.consumed_vcpu_h / entitlement.max_vcpu_h) * 100,
                )}% of your monthly vCPU-hour allowance consumed`}
              />
            ) : (
              <Stat
                label="vCPU-hours consumed"
                value={entitlement.consumed_vcpu_h.toLocaleString()}
                sub="Your monthly allowance appears once you add a runner plan"
              />
            )}
            <p className="lin-card__meta">
              <HelpPopover label="What are concurrency and vCPU-hours?">
                <strong>Concurrency</strong> is the maximum number of CI jobs that
                can run in parallel — a bigger runner SKU lets more of your pipeline
                run at once. A <strong>vCPU-hour</strong> is one virtual CPU running
                for one hour; your plan includes a monthly vCPU-hour allowance, and{" "}
                <strong>consumed vCPU-hours</strong> is how much of it you have used.
                Both are a separate entitlement from your cache tier.
              </HelpPopover>
            </p>
          </div>

          {/* Not entitled → the install + upgrade path. */}
          {!isEntitled ? (
            <div className="lin-mt-lg" data-testid="runners-cta">
              <Button
                variant="primary"
                href={INSTALL_HREF}
                data-testid="runners-install-cta"
              >
                Install GitHub App
              </Button>{" "}
              <Button
                variant="ghost"
                href={RUNNER_PLANS_HREF}
                data-testid="runners-upgrade-cta"
              >
                View runner plans
              </Button>
              <p className="lin-card__meta">
                {entitlement.install_status === "not_installed"
                  ? "Install the CoreLink GitHub App and add a runner plan to provision capacity. You’ll choose which repositories to grant access to on GitHub."
                  : "Add a runner plan to provision concurrency and a monthly vCPU-hour allowance."}
              </p>
            </div>
          ) : null}
        </div>
      </Card>

      {/* Install — the ONE mutating action on this screen, kept available even
          once entitled (reconfigure repo access on GitHub). [live] */}
      <Card
        title="Connect GitHub"
        meta="Install (or reconfigure) the CoreLink GitHub App to dispatch CI jobs to your runners against the shared cache."
        className="lin-mt-lg"
      >
        <div data-testid="runners-install">
          {/* Full-page navigation (302 → GitHub) via the kit anchor Button —
              client-side routing cannot follow the cross-origin redirect. */}
          <Button
            variant={isEntitled ? "ghost" : "primary"}
            href={INSTALL_HREF}
            data-testid="runners-install-app-cta"
          >
            {entitlement.install_status === "installed"
              ? "Manage GitHub App"
              : "Install GitHub App"}
          </Button>
          <p className="lin-card__meta">
            You&rsquo;ll be redirected to GitHub to choose which repositories to
            grant access to. CoreLink resolves your tenant and mints a signed
            install request server-side.
          </p>
        </div>
      </Card>

      {/* Repository allowlist — [live] BE-10. Real list; empty → honest state. */}
      <Card
        title="Repository allowlist"
        meta="Repositories permitted to dispatch jobs to your runners"
        className="lin-mt-lg"
      >
        <div data-testid="runners-allowlist">
          {allowlist.length > 0 ? (
            <ul data-testid="runners-allowlist-list">
              {allowlist.map((repo, i) => (
                <li
                  key={repo}
                  data-testid={`runners-allowlist-${repo}`}
                  className={i > 0 ? "lin-mt" : undefined}
                >
                  <Badge tone="neutral" dot>
                    {repo}
                  </Badge>
                </li>
              ))}
            </ul>
          ) : (
            <div data-testid="runners-allowlist-empty">
              <EmptyState
                title="No repositories allowed yet"
                body="After you install the CoreLink GitHub App you’ll choose which repositories may dispatch jobs to your runners. Granted repositories appear here."
                cta={
                  <Button
                    variant="ghost"
                    href={INSTALL_HREF}
                    data-testid="runners-allowlist-install-cta"
                  >
                    Install GitHub App
                  </Button>
                }
              />
              <Callout tone="info">
                <strong>Repository access is set on GitHub.</strong> Use the GitHub
                App install above to grant or revoke which repositories can reach
                your runners.
              </Callout>
            </div>
          )}
        </div>
      </Card>

      {/* Recent runs — [live] BE-10. History table is empty server-side today, so
          this honestly shows an EmptyState rather than a fabricated run. */}
      <Card
        title="Recent runs"
        meta="Your latest CI jobs dispatched to CoreLink runners"
        className="lin-mt-lg"
      >
        <div data-testid="runners-runs">
          {runList.length > 0 ? (
            <table data-testid="runners-runs-table" className="lin-table">
              <thead>
                <tr>
                  <th>Run</th>
                  <th>Repository</th>
                  <th>Status</th>
                  <th>Started</th>
                  <th>Duration</th>
                </tr>
              </thead>
              <tbody>
                {runList.map((run) => {
                  const meta = RUN_STATUS_META[run.status];
                  return (
                    <tr key={run.run_id} data-testid={`runners-run-${run.run_id}`}>
                      <td>{run.run_id}</td>
                      <td>{run.repo_full_name}</td>
                      <td>
                        <Badge tone={meta.tone} dot>
                          {meta.label}
                        </Badge>
                      </td>
                      <td>{formatStartedAt(run.started_at)}</td>
                      <td>{formatDuration(run.duration_s)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          ) : (
            <div data-testid="runners-runs-empty">
              <EmptyState
                title="No runs yet"
                body="Once you install the GitHub App and your CI dispatches a job to your runners, each run appears here with its repository, status, and duration."
              />
            </div>
          )}
        </div>
      </Card>
    </div>
  );
}

export default RunnersClient;
