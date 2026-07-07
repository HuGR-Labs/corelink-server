"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import type { ConsentRow } from "@/lib/consent-types";
import type { ConsentApi } from "@/lib/consent-api";
import { defaultConsentApi } from "@/lib/consent-api";
import {
  Badge,
  Button,
  EmptyState,
  InlineError,
  Skeleton,
} from "@/components/ui/linear";

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

  return (
    <div className="cx-shell lin">
      <div className="cx-main">
        <main>
          <h1>Consents</h1>
          <section aria-label="Active consents" data-testid="consent-dashboard">
            {loading ? (
              <div
                data-testid="dashboard-loading"
                aria-busy="true"
                aria-label="Loading consents"
              >
                <Skeleton rows={3} />
              </div>
            ) : error ? (
              <div role="alert" data-testid="dashboard-error">
                <InlineError error={error} />
              </div>
            ) : rows.length === 0 ? (
              <div data-testid="dashboard-empty">
                <EmptyState
                  title="No active consents"
                  body="You have not granted any consents yet."
                  cta={<Button href="/consent/new">Grant a new consent</Button>}
                />
              </div>
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
                      <td>
                        <Badge tone="success" dot>
                          {r.status}
                        </Badge>
                      </td>
                      <td>
                        <a
                          href={`/consent/${r.id}`}
                          aria-label={`View consent ${r.id}`}
                        >
                          view
                        </a>{" "}
                        <a
                          href={`/consent/withdraw/${r.id}`}
                          aria-label={`Withdraw consent ${r.id}`}
                        >
                          withdraw
                        </a>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </section>
          <p>
            <Link href="/consent/new">Grant a new consent</Link> ·{" "}
            <Link href="/consent/history">View history</Link>
          </p>
        </main>
      </div>
    </div>
  );
}
