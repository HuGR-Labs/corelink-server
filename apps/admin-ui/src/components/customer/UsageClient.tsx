// Customer-side "Usage & savings" (W4).
//
// DATA-TRUTH (mirrors customer_d1.rs / customer-types.ts):
//   cas_bytes / quota_bytes ....... [live]  → the Storage gauge
//   request_count ................. [live]  (BE-1a) → the Requests gauge vs the
//                                             tier's requestsPerMonthMax (pricing.ts,
//                                             plan from getOverview); if the plan is
//                                             unknown the raw count renders as a Stat,
//                                             never a gauge with a fabricated max
//   reads / writes / daily ........ [live]  (BE-1) → real period totals + the
//                                             Day/Reads/Writes table (daily.cas_bytes
//                                             is always 0 → no per-day storage column)
//   hit_rate / time_saved / $-saved [live]  (BE-2) → the ROI hero. hit_rate null =
//                                             cold start → teaching state, never a fake %.
//                                             $-saved is a MODELED estimate — labelled so.
//   $-ceiling ..................... [not-wired] (BE-7) → getDollarCeiling() throws
//                                             NotWiredError → teaching Callout, never a value
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

// Humanize a build-time-saved duration: seconds → "45 s" / "12 min" / "3.7 h".
function humanizeSeconds(s: number): string {
  if (s < 60) return `${Math.round(s)} s`;
  if (s < 3600) return `${Math.round(s / 60)} min`;
  return `${Math.round((s / 3600) * 10) / 10} h`;
}

// Render modeled dollar savings from cents as "$X" (whole dollars once ≥ $10,
// otherwise 2dp so a small early-days figure isn't rounded to "$0").
function dollarsSaved(cents: number): string {
  const d = cents / 100;
  return d >= 10
    ? `$${Math.round(d).toLocaleString()}`
    : `$${(Math.round(d * 100) / 100).toLocaleString()}`;
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
      {/* Hero ROI slot — [live] via BE-2. hit_rate == null means no reads yet
          (cold start): teach, never fabricate a %. Otherwise show the real
          hit-rate, humanized time saved, and the MODELED $ saved (labelled). */}
      <Card title="Your cache ROI">
        {data.hit_rate == null ? (
          <div data-testid="usage-roi-coldstart">
            <EmptyState
              title="Your savings appear here after your first builds"
              body="Cache-hit rate, build time saved, and dollars saved appear once your cache serves its first reads. A cold start is normal — hits climb from D+1 to D+7 as your cache fills, and the shared _public mirror warms deterministic deps for free."
            />
          </div>
        ) : (
          <div data-testid="usage-roi">
            <div data-testid="usage-roi-hit-rate">
              <Stat
                label="Cache-hit rate"
                value={`${Math.round(data.hit_rate * 100)}%`}
                sub="Share of cache lookups served from the cache instead of rebuilt this period."
              />
            </div>
            <div data-testid="usage-roi-time-saved">
              <Stat
                label="Build time saved"
                value={humanizeSeconds(data.time_saved_seconds)}
                sub="Wall-clock build time your cache hits avoided this period."
              />
            </div>
            <div data-testid="usage-roi-dollars-saved">
              <Stat
                label="Saved (estimated)"
                value={dollarsSaved(data.dollars_saved_cents)}
                sub="A modeled estimate of compute cost avoided — not a billed figure."
              />
              <p className="lin-card__meta">
                <HelpPopover label="How is this estimated?">
                  This is a modeled estimate, not a billed amount: we credit
                  roughly 15 seconds of compute saved per cache hit and price it
                  at typical CI compute rates. Treat it as a directional savings
                  signal, not an invoice line.
                </HelpPopover>
              </p>
            </div>
          </div>
        )}
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

      {/* Daily breakdown — [live] (BE-1). Day/Reads/Writes only: daily.cas_bytes
          is always 0 (no per-day byte history) so we drop the storage column
          rather than render a fake "0 B". */}
      <Card title="Daily breakdown">
        {hasDaily ? (
          <table data-testid="usage-daily-table" className="lin-table">
            <thead>
              <tr>
                <th>Day</th>
                <th>Reads</th>
                <th>Writes</th>
              </tr>
            </thead>
            <tbody>
              {data.daily.map((d) => (
                <tr key={d.day} data-testid={`usage-day-${d.day}`}>
                  <td>{d.day}</td>
                  <td>{d.reads.toLocaleString()}</td>
                  <td>{d.writes.toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <div data-testid="usage-daily-empty">
            <EmptyState
              title="Per-day usage lands as your cache is used"
              body="A day-by-day breakdown of reads and writes appears here once your cache starts serving traffic."
            />
          </div>
        )}
      </Card>
    </div>
  );
}

export default UsageClient;
