// Customer-side "Usage & savings" (W4).
//
// DATA-TRUTH (mirrors customer_d1.rs / customer-types.ts):
//   cas_bytes / quota_bytes ....... [live]  → the Storage gauge
//   reads / writes / daily ........ [stub]  → prod returns 0 / [] (BE-1) →
//                                             teaching EmptyState, never a fake 0
//   request-count ceiling ......... not a field on CustomerUsage → [stub] teaching hint
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
import type { CustomerUsage } from "@/lib/customer-types";
import {
  Card,
  Callout,
  EmptyState,
  Gauge,
  HelpPopover,
  InlineError,
  Segmented,
  Skeleton,
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

export function UsageClient(): React.ReactElement {
  const { getToken } = useAuth();
  const client = React.useMemo(() => new CustomerClient({ getToken }), [getToken]);
  const [data, setData] = React.useState<CustomerUsage | null>(null);
  const [err, setErr] = React.useState<unknown>(null);
  const [range, setRange] = React.useState<RangeKey>("period");
  const [reloadKey, setReloadKey] = React.useState(0);

  React.useEffect(() => {
    let alive = true;
    setData(null);
    setErr(null);
    client
      .getUsage()
      .then((d) => alive && setData(d))
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

  const hasRequestMetering = data.reads > 0 || data.writes > 0;
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

        {/* Requests — no request-count field on CustomerUsage → [stub].
            Show the gauge track with a teaching hint, never a fake number. */}
        <div data-testid="usage-requests">
          {hasRequestMetering ? (
            <>
              <Gauge
                label="Requests"
                value={data.reads + data.writes}
                max={data.reads + data.writes}
                unit="ops"
                hint="Reads (cache hits served) + writes (artifacts stored) this period."
              />
              <p className="lin-card__meta">
                Reads{" "}
                <strong data-testid="usage-reads">{data.reads.toLocaleString()}</strong>
                {" · "}Writes{" "}
                <strong data-testid="usage-writes">{data.writes.toLocaleString()}</strong>{" "}
                <HelpPopover label="What are reads and writes?">
                  A read is a cache lookup (a hit serves you a prebuilt artifact); a
                  write stores a new artifact. Both count against your monthly request
                  ceiling.
                </HelpPopover>
              </p>
            </>
          ) : (
            <EmptyState
              title="Request usage vs. your plan ceiling lands with metering"
              body="Per-request read/write counts (and your % of the monthly request ceiling) appear here once usage metering ships (BE-1). Until then we show storage — your live cap — rather than a fabricated 0."
            />
          )}
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
