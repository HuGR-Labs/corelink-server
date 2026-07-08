"use client";

import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { listMyDsrs, type ListMyDsrsResult } from "@/lib/dsr-client";
import type { DsrRequestSummary, DsrStatus } from "@/lib/dsr-types";
import { interpolate, tFor, type Locale } from "@/i18n";
import { SlaCountdown } from "@/components/dsr/SlaCountdown";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  InlineError,
  Skeleton,
} from "@/components/ui/linear";

export interface DsrStatusListClientProps {
  locale: Locale;
  /** Test injection: pre-supplied pages of results. */
  pages?: ListMyDsrsResult[];
  /** Clerk session token resolved server-side in `page.tsx`. */
  token?: string;
}

/** DSR status → Linear Badge tone (semantic status dots only). */
const STATUS_TONE: Record<DsrStatus, "neutral" | "success" | "warn" | "danger"> =
  {
    pending: "warn",
    in_progress: "neutral",
    completed: "success",
    rejected: "danger",
  };

/**
 * Status viewer (`/<locale>/dsr/status`).
 *
 * Renders the current user's DSR requests with cursor-based pagination.
 * Each row links to the detail route. The session token is supplied by the
 * server page (`auth().getToken()`); without it the loader can't fetch.
 */
export function DsrStatusListClient(props: DsrStatusListClientProps) {
  const t = (k: string) => tFor(props.locale, k);

  const [items, setItems] = useState<DsrRequestSummary[]>(
    props.pages?.[0]?.items ?? [],
  );
  const [pageIdx, setPageIdx] = useState(0);
  const [cursor, setCursor] = useState<string | undefined>(
    props.pages?.[0]?.next_cursor,
  );
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [loading, setLoading] = useState<boolean>(
    !props.pages && props.token != null,
  );
  const [error, setError] = useState<unknown | null>(null);

  const loadPage = useCallback(
    async (c: string | undefined, dir: "next" | "prev") => {
      if (props.pages) {
        const idx = dir === "next" ? pageIdx + 1 : pageIdx - 1;
        const page = props.pages[idx];
        if (!page) return;
        setItems(page.items);
        setCursor(page.next_cursor);
        setPageIdx(idx);
        return;
      }
      if (!props.token) return;
      setLoading(true);
      setError(null);
      try {
        const result = await listMyDsrs({ token: props.token, cursor: c });
        setItems(result.items);
        setCursor(result.next_cursor);
        if (dir === "next") {
          setHistory((h) => [...h, c]);
          setPageIdx((p) => p + 1);
        } else {
          setHistory((h) => h.slice(0, -1));
          setPageIdx((p) => Math.max(0, p - 1));
        }
      } catch (e) {
        setError(e);
      } finally {
        setLoading(false);
      }
    },
    [props.pages, props.token, pageIdx],
  );

  /** Re-fetch the current page in place (used by the InlineError retry). */
  const refetch = useCallback(() => {
    const token = props.token;
    if (props.pages || !token) return;
    setLoading(true);
    setError(null);
    void (async () => {
      try {
        const result = await listMyDsrs({
          token,
          cursor: history[pageIdx],
        });
        setItems(result.items);
        setCursor(result.next_cursor);
      } catch (e) {
        setError(e);
      } finally {
        setLoading(false);
      }
    })();
  }, [props.pages, props.token, history, pageIdx]);

  // Initial load — fetch the first page once the server-supplied token is here.
  useEffect(() => {
    const token = props.token;
    if (!token || props.pages) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    void (async () => {
      try {
        const result = await listMyDsrs({ token });
        if (cancelled) return;
        setItems(result.items);
        setCursor(result.next_cursor);
      } catch (e) {
        if (!cancelled) setError(e);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.token]);

  return (
    <div className="cx-shell lin">
      <div className="cx-main">
        <main aria-labelledby="dsr-status-title">
          <h1 id="dsr-status-title">{t("dsr.status.title")}</h1>

          <Card>
            {loading ? (
              <div
                data-testid="dsr-status-loading"
                aria-busy="true"
                aria-label="Loading"
              >
                <Skeleton rows={4} />
              </div>
            ) : error != null ? (
              <div data-testid="dsr-status-error">
                <InlineError error={error} onRetry={refetch} />
              </div>
            ) : items.length === 0 ? (
              <EmptyState title={t("dsr.status.empty")} />
            ) : (
              <div className="overflow-x-auto">
                <table className="lin-table" data-testid="dsr-status-table">
                  <thead>
                    <tr>
                      <th>{t("dsr.status.table_request_id")}</th>
                      <th>{t("dsr.status.table_action")}</th>
                      <th>{t("dsr.status.table_status")}</th>
                      <th>{t("dsr.status.table_submitted")}</th>
                      <th>{t("dsr.status.table_deadline")}</th>
                      <th>{t("dsr.status.table_actions")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {items.map((row) => (
                      <tr
                        key={row.request_id}
                        data-testid={`dsr-row-${row.request_id}`}
                      >
                        <td>
                          <code>{row.request_id.slice(-8)}</code>
                        </td>
                        <td>{t(`dsr.rights.${row.action}.label`)}</td>
                        <td>
                          <Badge tone={STATUS_TONE[row.status]} dot>
                            {t(`dsr.status_states.${row.status}`)}
                          </Badge>
                        </td>
                        <td>{row.submitted_at}</td>
                        <td>
                          <SlaCountdown
                            locale={props.locale}
                            deadline={row.sla_deadline}
                          />
                        </td>
                        <td>
                          <Link
                            href={`/${props.locale}/dsr/status/${encodeURIComponent(row.request_id)}`}
                            className="lin-btn lin-btn--ghost lin-btn--sm"
                            data-testid={`dsr-row-view-${row.request_id}`}
                          >
                            {t("dsr.status.view_details")}
                          </Link>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Card>

          <nav
            aria-label="pagination"
            className="lin-mt flex items-center gap-3"
          >
            <Button
              variant="ghost"
              size="sm"
              onClick={() => loadPage(history[pageIdx - 1], "prev")}
              disabled={pageIdx === 0}
              data-testid="dsr-status-prev"
            >
              {t("dsr.status.prev_page")}
            </Button>
            <span
              className="lin-card__meta"
              data-testid="dsr-status-page-indicator"
            >
              {interpolate(t("dsr.status.page_indicator"), {
                page: pageIdx + 1,
              })}
            </span>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => loadPage(cursor, "next")}
              disabled={!cursor}
              data-testid="dsr-status-next"
            >
              {t("dsr.status.next_page")}
            </Button>
          </nav>
        </main>
      </div>
    </div>
  );
}
