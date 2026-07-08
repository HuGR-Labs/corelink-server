// WI-S16-005 — client component composing filter + table + drawer + export.

"use client";

import React from "react";
import type {
  AuditEventDetail,
  AuditFilter,
  AuditPage,
} from "@/lib/types";
import { AdminClient } from "@/lib/admin-client";
import { Button, Card, InlineError, Segmented, Skeleton } from "@/components/ui/linear";
import AuditFilterBar from "./AuditFilterBar";
import AuditTable from "./AuditTable";
import AuditEventDrawer from "./AuditEventDrawer";

export interface AuditViewerProps {
  client: AdminClient;
  initialFilter?: AuditFilter;
}

export function AuditViewer({
  client,
  initialFilter,
}: AuditViewerProps): React.ReactElement {
  const [filter, setFilter] = React.useState<AuditFilter>(initialFilter ?? {});
  const [page, setPage] = React.useState<AuditPage>({ rows: [], next_cursor: null });
  const [selected, setSelected] = React.useState<AuditEventDetail | null>(null);
  const [drawerOpen, setDrawerOpen] = React.useState(false);
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [exportFormat, setExportFormat] = React.useState<"csv" | "json">("csv");

  // Capture every filter version we have rendered, so pagination preserves the
  // *applied* filter even if user edits inputs while results are loading.
  const filterRef = React.useRef(filter);
  filterRef.current = filter;

  const fetchPage = React.useCallback(
    async (overrideCursor?: string | null) => {
      setLoading(true);
      setError(null);
      try {
        const effectiveFilter: AuditFilter = {
          ...filterRef.current,
          cursor: overrideCursor ?? filterRef.current.cursor ?? null,
        };
        const result = await client.listAuditEvents(effectiveFilter);
        setPage(result);
      } catch (err) {
        setError(err instanceof Error ? err.message : "unknown");
      } finally {
        setLoading(false);
      }
    },
    [client],
  );

  // re-fetch whenever the *filter shape* changes (excluding cursor).
  const filterKey = JSON.stringify({ ...filter, cursor: undefined });
  React.useEffect(() => {
    void fetchPage(null);
  }, [filterKey, fetchPage]);

  const openEvent = async (eventId: string) => {
    try {
      const detail = await client.getAuditEvent(eventId);
      setSelected(detail);
      setDrawerOpen(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : "unknown");
    }
  };

  const exportFilter = async () => {
    try {
      const res = await client.exportAudit(filterRef.current, exportFormat);
      // Signed URL TTL 24h is server-enforced; we surface it but never store.
      if (typeof window !== "undefined") {
        window.open(res.signed_url, "_blank", "noopener,noreferrer");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "unknown");
    }
  };

  const exportAction = (
    <>
      <span data-testid="export-format">
        <Segmented<"csv" | "json">
          options={[
            { value: "csv", label: "CSV" },
            { value: "json", label: "JSON" },
          ]}
          value={exportFormat}
          onChange={setExportFormat}
        />
      </span>
      <Button
        variant="ghost"
        size="sm"
        data-testid="export-btn"
        onClick={() => void exportFilter()}
        disabled={loading}
      >
        Export current filter
      </Button>
    </>
  );

  return (
    <div data-testid="audit-viewer">
      <Card title="Filters">
        <AuditFilterBar filter={filter} onChange={setFilter} />
      </Card>

      <Card title="Events" actions={exportAction} className="lin-mt">
        {error && (
          <div data-testid="audit-error">
            <InlineError error={error} onRetry={() => void fetchPage(null)} />
          </div>
        )}

        {loading ? (
          <div aria-busy="true" aria-label="Loading audit events">
            <Skeleton rows={5} />
          </div>
        ) : (
          <>
            <AuditTable rows={page.rows} onRowClick={(id) => void openEvent(id)} />

            <nav
              className="lin-mt"
              data-testid="audit-pagination"
              aria-label="Audit pagination"
            >
              <Button
                variant="ghost"
                size="sm"
                data-testid="next-page"
                disabled={!page.next_cursor || loading}
                onClick={() => {
                  setFilter((f) => ({ ...f, cursor: page.next_cursor }));
                  void fetchPage(page.next_cursor);
                }}
              >
                Next page
              </Button>
            </nav>
          </>
        )}
      </Card>

      <AuditEventDrawer
        event={selected}
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
      />
    </div>
  );
}

export default AuditViewer;
