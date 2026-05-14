// WI-S16-005 — tenant search with debounced query.

"use client";

import React from "react";
import Link from "next/link";
import type { Tenant } from "@/lib/types";
import type { AdminClient } from "@/lib/admin-client";

export interface TenantSearchProps {
  client: AdminClient;
  debounceMs?: number;
}

export function TenantSearch({
  client,
  debounceMs = 200,
}: TenantSearchProps): React.ReactElement {
  const [query, setQuery] = React.useState("");
  const [results, setResults] = React.useState<Tenant[]>([]);
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let cancelled = false;
    const handle = setTimeout(() => {
      setLoading(true);
      void client
        .searchTenants(query)
        .then((tenants) => {
          if (!cancelled) setResults(tenants);
        })
        .catch((e: unknown) => {
          if (!cancelled) setError(e instanceof Error ? e.message : "unknown");
        })
        .finally(() => {
          if (!cancelled) setLoading(false);
        });
    }, debounceMs);
    return () => {
      cancelled = true;
      clearTimeout(handle);
    };
  }, [query, debounceMs, client]);

  return (
    <div data-testid="tenant-search">
      <label>
        Search tenants
        <input
          type="search"
          data-testid="tenant-search-input"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>

      {error && (
        <p role="alert" data-testid="tenant-search-error">
          {error}
        </p>
      )}

      <table data-testid="tenant-table" aria-busy={loading}>
        <thead>
          <tr>
            <th>tenant_id</th>
            <th>name</th>
            <th>plan</th>
            <th>region</th>
            <th>byok</th>
            <th>created_at</th>
            <th>actions</th>
          </tr>
        </thead>
        <tbody>
          {results.length === 0 && (
            <tr>
              <td colSpan={7}>No tenants found.</td>
            </tr>
          )}
          {results.map((t) => (
            <tr key={t.tenant_id} data-testid={`tenant-row-${t.tenant_id}`}>
              <td>
                <code>{t.tenant_id}</code>
              </td>
              <td>{t.name}</td>
              <td>{t.plan}</td>
              <td>{t.region}</td>
              <td>{t.byok_status}</td>
              <td>{t.created_at}</td>
              <td>
                <Link href={`./tenants/${t.tenant_id}`}>view</Link>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default TenantSearch;
