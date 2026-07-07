// WI-S16-005 — paginated audit table. Linear kit: `lin-table` + severity Badge.

"use client";

import React from "react";
import type { AuditEventSummary } from "@/lib/types";
import { Badge } from "@/components/ui/linear";
import { severityTone } from "./tones";

export interface AuditTableProps {
  rows: AuditEventSummary[];
  onRowClick: (eventId: string) => void;
}

const SEVERITY_LABEL: Record<string, string> = {
  info: "Info",
  warn: "Warning",
  critical: "Critical",
};

export function AuditTable({ rows, onRowClick }: AuditTableProps): React.ReactElement {
  return (
    <table className="lin-table" data-testid="audit-table">
      <thead>
        <tr>
          <th scope="col">Timestamp</th>
          <th scope="col">Tenant</th>
          <th scope="col">Event type</th>
          <th scope="col">Severity</th>
          <th scope="col">Actor</th>
          <th scope="col">Summary</th>
        </tr>
      </thead>
      <tbody>
        {rows.length === 0 && (
          <tr>
            <td colSpan={6} data-testid="empty-row">
              No events match the current filter.
            </td>
          </tr>
        )}
        {rows.map((row) => (
          <tr
            key={row.event_id}
            data-testid={`audit-row-${row.event_id}`}
            tabIndex={0}
            role="button"
            aria-label={`Open audit event ${row.event_id}`}
            onClick={() => onRowClick(row.event_id)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onRowClick(row.event_id);
              }
            }}
          >
            <td>{row.ts}</td>
            <td>
              <code>{row.tenant_id}</code>
            </td>
            <td>{row.event_type}</td>
            <td data-severity={row.severity}>
              <Badge tone={severityTone(row.severity)} dot>
                {SEVERITY_LABEL[row.severity] ?? row.severity}
              </Badge>
            </td>
            <td>{row.actor}</td>
            <td>{row.summary}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export default AuditTable;
