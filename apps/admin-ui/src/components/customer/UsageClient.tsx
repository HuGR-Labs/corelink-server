// Customer-side usage detail — CAS storage, reads/writes, daily breakdown.

"use client";

import React from "react";
import { useAuth } from "@clerk/nextjs";
import { CustomerClient } from "@/lib/customer-client";
import type { CustomerUsage } from "@/lib/customer-types";

function mb(b: number): string {
  return (b / 1024 ** 2).toFixed(1) + " MiB";
}

export function UsageClient(): React.ReactElement {
  const { getToken } = useAuth();
  const client = React.useMemo(() => new CustomerClient({ getToken }), [getToken]);
  const [data, setData] = React.useState<CustomerUsage | null>(null);
  const [err, setErr] = React.useState<string | null>(null);

  React.useEffect(() => {
    let alive = true;
    client
      .getUsage()
      .then((d) => alive && setData(d))
      .catch((e: unknown) => alive && setErr(String(e)));
    return () => {
      alive = false;
    };
  }, [client]);

  if (err) return <p data-testid="usage-error">{err}</p>;
  if (!data) return <p data-testid="usage-loading">loading…</p>;

  const pct = Math.min(100, Math.round((data.cas_bytes / data.quota_bytes) * 100));

  return (
    <div data-testid="usage-shell">
      <section>
        <h2>Period {data.period}</h2>
        <p>
          CAS:{" "}
          <strong data-testid="usage-cas-pct">{pct}%</strong> of quota (
          {(data.cas_bytes / 1024 ** 3).toFixed(2)} /{" "}
          {(data.quota_bytes / 1024 ** 3).toFixed(2)} GiB)
        </p>
        <p>
          Reads: <span data-testid="usage-reads">{data.reads.toLocaleString()}</span> · Writes:{" "}
          <span data-testid="usage-writes">{data.writes.toLocaleString()}</span>
        </p>
      </section>
      <table data-testid="usage-daily-table">
        <thead>
          <tr>
            <th>Day</th>
            <th>Reads</th>
            <th>Writes</th>
            <th>CAS bytes</th>
          </tr>
        </thead>
        <tbody>
          {data.daily.map((d) => (
            <tr key={d.day} data-testid={`usage-day-${d.day}`}>
              <td>{d.day}</td>
              <td>{d.reads.toLocaleString()}</td>
              <td>{d.writes.toLocaleString()}</td>
              <td>{mb(d.cas_bytes)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default UsageClient;
