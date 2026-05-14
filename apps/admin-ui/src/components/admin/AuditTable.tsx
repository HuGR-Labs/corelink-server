// WI-S16-005 — paginated audit table.

"use client";

import React from "react";
import type { AuditEventSummary } from "@/lib/types";

export interface AuditTableProps {
  rows: AuditEventSummary[];
  onRowClick: (eventId: string) => void;
}

export function AuditTable({ rows, onRowClick }: AuditTableProps): React.ReactElement {
  return (
    <table data-testid="audit-table">
      <thead>
        <tr>
          <th scope="col">timestamp</th>
          <th scope="col">tenant_id</th>
          <th scope="col">event_type</th>
          <th scope="col">severity</th>
          <th scope="col">actor</th>
          <th scope="col">summary</th>
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
            onClick={() => onRowClick(row.event_id)}
            style={{ cursor: "pointer" }}
          >
            <td>{row.ts}</td>
            <td>{row.tenant_id}</td>
            <td>{row.event_type}</td>
            <td data-severity={row.severity}>{row.severity}</td>
            <td>{row.actor}</td>
            <td>{row.summary}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export default AuditTable;
