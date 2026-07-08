"use client";

import { useEffect, useState } from "react";
import type { ConsentRow } from "@/lib/consent-types";
import type { ConsentApi } from "@/lib/consent-api";
import { defaultConsentApi } from "@/lib/consent-api";
import {
  Badge,
  Button,
  ConfirmDialog,
  EmptyState,
  InlineError,
  Skeleton,
} from "@/components/ui/linear";

export interface ConsentDashboardProps {
  api?: ConsentApi;
  /**
   * Active i18n locale from the `[locale]` route segment. Threaded so every
   * nav link is locale-prefixed (`/en/consent/…`) — without it the links drop
   * the segment and 404. Optional so tests can render without a router; when
   * absent links fall back to the bare path.
   */
  locale?: string;
}

export function ConsentDashboard({
  api = defaultConsentApi,
  locale,
}: ConsentDashboardProps) {
  const [rows, setRows] = useState<ConsentRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [confirmRow, setConfirmRow] = useState<ConsentRow | null>(null);

  // Locale prefix for every in-app link. Empty when locale is unknown (tests).
  const p = locale ? `/${locale}` : "";
  const newHref = `${p}/consent/new`;
  const historyHref = `${p}/consent/history`;
  const viewHref = (id: string) => `${p}/consent/${id}`;
  const withdrawHref = (id: string) => `${p}/consent/withdraw/${id}`;

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
          <p>
            A consent record is a permission you have granted for one specific
            data-processing purpose (its legal basis, data categories, and
            retention). This is your log of active permissions — to exercise
            access, deletion, or portability of your data instead, use the DSR
            rights center.
          </p>
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
                  cta={<Button href={newHref}>Grant a new consent</Button>}
                />
              </div>
            ) : (
              <div className="lin-card lin-card--pad">
                <table className="lin-table">
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
                          <Badge
                            tone={r.status === "active" ? "success" : "neutral"}
                            dot
                          >
                            {r.status}
                          </Badge>
                        </td>
                        <td>
                          <span className="lin-card__actions">
                            <Button
                              variant="ghost"
                              size="sm"
                              href={viewHref(r.id)}
                              aria-label={`View consent ${r.id}`}
                            >
                              View
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              href={withdrawHref(r.id)}
                              aria-label={`Withdraw consent ${r.id}`}
                              onClick={(e) => {
                                e.preventDefault();
                                setConfirmRow(r);
                              }}
                            >
                              Withdraw
                            </Button>
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>

          <div className="lin-mt-lg lin-card__actions">
            <Button href={newHref} variant="primary" size="sm">
              Grant a new consent
            </Button>
            <Button href={historyHref} variant="ghost" size="sm">
              View history
            </Button>
          </div>
        </main>
      </div>

      <ConfirmDialog
        open={confirmRow !== null}
        onClose={() => setConfirmRow(null)}
        title="Withdraw this consent?"
        danger
        confirmLabel="Continue to withdrawal"
        body={
          <p>
            Withdrawing revokes this permission going forward. You will be asked
            to re-authenticate (MFA) before the withdrawal is recorded and a
            signed receipt is issued.
          </p>
        }
        onConfirm={() => {
          if (confirmRow) window.location.assign(withdrawHref(confirmRow.id));
        }}
      />
    </div>
  );
}
