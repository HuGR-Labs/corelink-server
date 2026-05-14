"use client";

import { useEffect, useState } from "react";
import type { ConsentRow } from "@/lib/consent-types";
import type { ConsentApi } from "@/lib/consent-api";
import { defaultConsentApi } from "@/lib/consent-api";

export interface ConsentDashboardProps {
  api?: ConsentApi;
}

export function ConsentDashboard({ api = defaultConsentApi }: ConsentDashboardProps) {
  const [rows, setRows] = useState<ConsentRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .listActive()
      .then((r) => {
        if (!cancelled) setRows(r);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(e instanceof Error ? e.message : "load_failed");
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api]);

  if (loading) return <p data-testid="dashboard-loading">Loading consents...</p>;
  if (error) return <p role="alert" data-testid="dashboard-error">{error}</p>;

  return (
    <section aria-label="Active consents" data-testid="consent-dashboard">
      <h1>Consents</h1>
      {rows.length === 0 ? (
        <p data-testid="dashboard-empty">No active consents.</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th scope="col">Purpose</th>
              <th scope="col">Granted at</th>
              <th scope="col">Status</th>
              <th scope="col">Actions</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.id} data-testid={`row-${r.id}`}>
                <td>{r.purpose}</td>
                <td>{r.granted_at}</td>
                <td>{r.status}</td>
                <td>
                  <a href={`/consent/${r.id}`} aria-label={`View consent ${r.id}`}>view</a>
                  {" "}
                  <a href={`/consent/withdraw/${r.id}`} aria-label={`Withdraw consent ${r.id}`}>
                    withdraw
                  </a>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <p>
        <a href="/consent/new">Grant a new consent</a> ·{" "}
        <a href="/consent/history">View history</a>
      </p>
    </section>
  );
}
