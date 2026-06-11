// Customer-side audit viewer — tenant-scoped, no Merkle tab (operator-only).

"use client";

import React from "react";
import { useAuth } from "@clerk/nextjs";
import { CustomerClient } from "@/lib/customer-client";
import type { CustomerAuditEvent } from "@/lib/customer-types";

export function AuditClient(): React.ReactElement {
  const { getToken } = useAuth();
  const client = React.useMemo(() => new CustomerClient({ getToken }), [getToken]);
  const [rows, setRows] = React.useState<CustomerAuditEvent[]>([]);
  const [since, setSince] = React.useState<string>("");
  const [loading, setLoading] = React.useState(true);

  const reload = React.useCallback(async (sinceVal: string) => {
    setLoading(true);
    const res = await client.listAudit(sinceVal ? { since: new Date(sinceVal).toISOString() } : {});
    setRows(res.rows);
    setLoading(false);
  }, [client]);

  React.useEffect(() => {
    void reload(since);
  }, [reload, since]);

  return (
    <div data-testid="customer-audit-shell">
      <label>
        Since:{" "}
        <input
          type="date"
          data-testid="customer-audit-since"
          value={since}
          onChange={(e) => setSince(e.target.value)}
        />
      </label>
      {loading ? <p data-testid="customer-audit-loading">loading…</p> : null}
      <table data-testid="customer-audit-table">
        <thead>
          <tr>
            <th>Time</th>
            <th>Event</th>
            <th>Severity</th>
            <th>Actor</th>
            <th>Summary</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.event_id} data-testid={`customer-audit-row-${r.event_id}`}>
              <td>{r.ts}</td>
              <td>
                <code>{r.event_type}</code>
              </td>
              <td>{r.severity}</td>
              <td>{r.actor}</td>
              <td>{r.summary}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default AuditClient;
