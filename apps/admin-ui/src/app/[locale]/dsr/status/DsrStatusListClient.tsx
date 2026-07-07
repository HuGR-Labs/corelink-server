"use client";

import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { listMyDsrs, type ListMyDsrsResult } from "@/lib/dsr-client";
import type { DsrRequestSummary } from "@/lib/dsr-types";
import { interpolate, tFor, type Locale } from "@/i18n";
import { SlaCountdown } from "@/components/dsr/SlaCountdown";
import { Badge, Button, EmptyState } from "@/components/ui/linear";

export interface DsrStatusListClientProps {
  locale: Locale;
  /** Test injection: pre-supplied pages of results. */
  pages?: ListMyDsrsResult[];
  token?: string;
}

/**
 * Status viewer (`/<locale>/dsr/status`).
 *
 * Renders the current user's DSR requests with cursor-based pagination.
 * Each row links to the detail route + offers a receipt-download stub.
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
    },
    [props.pages, props.token, pageIdx],
  );

  useEffect(() => {
    if (!props.token || props.pages) return;
    void loadPage(undefined, "next");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.token]);

  return (
    <div className="cx-shell lin">
      <div className="cx-main">
        <main aria-labelledby="dsr-status-title">
          <h1 id="dsr-status-title">{t("dsr.status.title")}</h1>
          {items.length === 0 ? (
            <section>
              <EmptyState title={t("dsr.status.empty")} />
            </section>
          ) : (
            <section>
              <table data-testid="dsr-status-table">
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
                        <Badge dot>{t(`dsr.status_states.${row.status}`)}</Badge>
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
                          data-testid={`dsr-row-view-${row.request_id}`}
                        >
                          {t("dsr.status.view_details")}
                        </Link>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}

          <nav aria-label="pagination">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => loadPage(history[pageIdx - 1], "prev")}
              disabled={pageIdx === 0}
              data-testid="dsr-status-prev"
            >
              {t("dsr.status.prev_page")}
            </Button>
            <span data-testid="dsr-status-page-indicator">
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
