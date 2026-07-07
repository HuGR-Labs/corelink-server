// Customer-side "Usage & savings" (W4).
//
// DATA-TRUTH (mirrors customer_d1.rs / customer-types.ts):
//   cas_bytes / quota_bytes ....... [live]  → the Storage gauge
//   request_count ................. [live]  (BE-1a) → the Requests gauge vs the
//                                             tier's requestsPerMonthMax (pricing.ts,
//                                             plan from getOverview); if the plan is
//                                             unknown the raw count renders as a Stat,
//                                             never a gauge with a fabricated max
//   reads / writes / daily ........ [stub]  → prod returns 0 / [] (BE-1) →
//                                             teaching EmptyState, never a fake 0
//   $-ceiling ..................... [not-wired] (BE-7) → getDollarCeiling() throws
//                                             NotWiredError → teaching Callout, never a value
//   cache-hit-rate / $-saved / dedup [not-wired]/[stub] (BE-2/BE-7) → teaching hero EmptyState
//
// The #1 rule for this screen: a stub / not-wired field NEVER renders as a real
// number — it renders a teaching EmptyState/Callout that explains WHAT will
// appear and WHEN (the backend WP that closes it).

"use client";

import React from "react";
import { useAuth } from "@clerk/nextjs";
import { CustomerClient } from "@/lib/customer-client";
import type { CustomerOverview, CustomerUsage } from "@/lib/customer-types";
import { TIERS } from "@/lib/pricing";
import {
  Card,
  Callout,
  EmptyState,
  Gauge,
  HelpPopover,
  InlineError,
  Segmented,
  Skeleton,
  Stat,
} from "@/components/ui/linear";

type RangeKey = "7d" | "30d" | "period";

const RANGE_OPTIONS: Array<{ value: RangeKey; label: string }> = [
  { value: "7d", label: "7 days" },
  { value: "30d", label: "30 days" },
  { value: "period", label: "This period" },
];

function gib(bytes: number): number {
  return Math.round((bytes / 1024 ** 3) * 100) / 100;
}

// The monthly request ceiling per tier lives in the frozen rate card
// (pricing.ts) as a feature string like "20M cache requests/mo" — the signed
// source of truth. We read it from `TIERS` (no separate field to drift) and
// parse the leading K/M/B-suffixed count into `requestsPerMonthMax`. Returns
// null when the plan has no requests feature (e.g. Enterprise) or is unknown —
// in which case the caller keeps the raw count as a Stat, never a fake gauge.
const REQUESTS_FEATURE = /^([\d.]+)\s*([KMB]?)\s*cache requests\/mo/i;
const SUFFIX_MULT: Record<string, number> = { "": 1, K: 1e3, M: 1e6, B: 1e9 };

function requestsPerMonthMax(plan: CustomerOverview["plan"] | null): number | null {
  if (plan == null) return null;
  const tier = TIERS.find((t) => t.id === plan && (t.group ?? "cache") === "cache");
  if (!tier) return null;
  for (const feature of tier.features) {
    const m = REQUESTS_FEATURE.exec(feature.trim());
    if (m) {
      const mult = SUFFIX_MULT[(m[2] ?? "").toUpperCase()] ?? 1;
      return Number(m[1]) * mult;
    }
  }
  return null;
}

export function UsageClient(): React.ReactElement {
  const { getToken } = useAuth();
  const client = React.useMemo(() => new CustomerClient({ getToken }), [getToken]);
  const [data, setData] = React.useState<CustomerUsage | null>(null);
  // Plan drives the request-ceiling gauge. It is a cheap side read from the
  // overview snapshot — a failure must NOT block Usage, so we degrade `plan` to
  // null (→ the request count renders as a Stat, never a fabricated ceiling).
  const [plan, setPlan] = React.useState<CustomerOverview["plan"] | null>(null);
  const [err, setErr] = React.useState<unknown>(null);
  const [range, setRange] = React.useState<RangeKey>("period");
  const [reloadKey, setReloadKey] = React.useState(0);

  React.useEffect(() => {
    let alive = true;
    setData(null);
    setPlan(null);
    setErr(null);
    // Usage is the load-bearing fetch; overview is a best-effort read purely for
    // the plan (request ceiling). An overview failure degrades to a null plan,
    // never blocks the screen.
    Promise.all([
      client.getUsage(),
      client.getOverview().then(
        (o) => o.plan,
        () => null,
      ),
    ])
      .then(([u, p]) => {
        if (!alive) return;
        setData(u);
        setPlan(p);
      })
      .catch((e: unknown) => alive && setErr(e));
    return () => {
      alive = false;
    };
  }, [client, reloadKey]);

  // $-ceiling is [not-wired] (BE-7): the client method throws NotWiredError by
  // contract, so there is nothing to probe — we render the teaching state and
  // NEVER a fabricated value. (Wire the real read here when BE-7 lands.)
  const dollarCeilingWp = "BE-7 dollar-ceiling read/update";

  if (err) {
    return (
      <div data-testid="usage-error">
        <InlineError error={err} onRetry={() => setReloadKey((k) => k + 1)} />
      </div>
    );
  }

  if (!data) {
    return (
      <div data-testid="usage-loading">
        <Card title="Usage & savings">
          <Skeleton rows={4} />
        </Card>
      </div>
    );
  }

  const storagePct = data.quota_bytes > 0 ? data.cas_bytes / data.quota_bytes : 0;
  const casPctLabel = `${Math.round(storagePct * 100)}%`;

  // Requests — [live] via BE-1a. request_count is real; the ceiling comes from
  // the tier rate card. With a known ceiling we show a real gauge; otherwise
  // (unknown/enterprise plan) we keep the raw count as a Stat — never a gauge
  // with a fabricated max.
  const requestMax = requestsPerMonthMax(plan);
  const requestPctLabel =
    requestMax != null && requestMax > 0
      ? `${Math.round((data.request_count / requestMax) * 100)}%`
      : null;
  const hasDaily = data.daily.length > 0;

  return (
    <div data-testid="usage-shell">
      {/* Hero ROI slot — cache-hit-rate / time-saved / moat dedup are all
          [not-wired]/[stub] today (BE-2/BE-7). Teach, never fabricate. */}
      <Card title="Your cache ROI">
        <EmptyState
          title="Your savings appear here after your first builds"
          body="Cache-hit rate, build time saved, and dollars saved land once metering ships (BE-2). A cold start is normal — hits climb from D+1 to D+7 as your cache fills, and the shared _public mirror warms deterministic deps for free."
        />
      </Card>

      <Card
        title="Ceilings"
        meta={`Period ${data.period}`}
        actions={
          <Segmented<RangeKey>
            options={RANGE_OPTIONS}
            value={range}
            onChange={setRange}
          />
        }
      >
        {/* Storage — [live] */}
        <div data-testid="usage-storage-gauge">
          <Gauge
            label="Storage"
            value={gib(data.cas_bytes)}
            max={gib(data.quota_bytes)}
            unit="GiB"
            hint={`${casPctLabel} of your storage cap (CAS — the content-addressed store that dedups every build artifact by hash)`}
          />
          <span data-testid="usage-cas-pct" hidden>
            {casPctLabel} of cap
          </span>
        </div>

        {/* Requests — [live] request_count (BE-1a). With a known tier ceiling
            we render a real gauge (count vs requestsPerMonthMax); when the plan
            (hence ceiling) is unknown we keep the raw count as a Stat rather
            than fabricating a max. */}
        <div data-testid="usage-requests">
          {requestMax != null ? (
            <>
              <Gauge
                label="Requests"
                value={data.request_count}
                max={requestMax}
                unit="req"
                hint={`${requestPctLabel} of your monthly request ceiling (billable cache requests this period, from usage metering).`}
              />
              <span data-testid="usage-requests-pct" hidden>
                {requestPctLabel} of ceiling
              </span>
            </>
          ) : (
            <div data-testid="usage-requests-stat">
              <Stat
                label="Requests this period"
                value={data.request_count.toLocaleString()}
                sub="Billable cache requests. Your plan ceiling appears once your tier is known."
              />
            </div>
          )}
          <p className="lin-card__meta">
            <HelpPopover label="What counts as a request?">
              A request is any billable cache operation this period — lookups
              (hits that serve you a prebuilt artifact) and stores (new
              artifacts). Every request counts against your monthly request
              ceiling. Per-read/write and daily breakdowns land with BE-1.
            </HelpPopover>
          </p>
        </div>

        {/* $-ceiling — [not-wired] (BE-7). NEVER a value; teach instead. */}
        <div data-testid="usage-dollar-ceiling">
          <Callout tone="info">
            <strong>Spend cap coming.</strong> Soon you&rsquo;ll set a hard monthly limit
            here — at 100% you get a 429, never a surprise bill. The control ships with the
            spend-cap backend ({dollarCeilingWp}).
          </Callout>
        </div>
      </Card>

      {/* Daily breakdown — [stub] prod=[] (BE-1). Teaching empty, not an empty table. */}
      <Card title="Daily breakdown">
        {hasDaily ? (
          <table data-testid="usage-daily-table" className="lin-table">
            <thead>
              <tr>
                <th>Day</th>
                <th>Reads</th>
                <th>Writes</th>
                <th>Storage</th>
              </tr>
            </thead>
            <tbody>
              {data.daily.map((d) => (
                <tr key={d.day} data-testid={`usage-day-${d.day}`}>
                  <td>{d.day}</td>
                  <td>{d.reads.toLocaleString()}</td>
                  <td>{d.writes.toLocaleString()}</td>
                  <td>{gib(d.cas_bytes)} GiB</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <div data-testid="usage-daily-empty">
            <EmptyState
              title="Per-day usage lands once metering ships"
              body="A day-by-day breakdown of reads, writes, and storage growth appears here after usage metering ships (BE-1)."
            />
          </div>
        )}
      </Card>
    </div>
  );
}

export default UsageClient;
