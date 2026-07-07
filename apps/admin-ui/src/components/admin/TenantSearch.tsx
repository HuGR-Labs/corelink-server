// WI-S16-005 — tenant search with debounced query. Linear kit: Card + Field +
// Input, `lin-table`, BYOK Badge, proper Skeleton/InlineError/EmptyState states.

"use client";

import React from "react";
import Link from "next/link";
import type { Tenant } from "@/lib/types";
import type { AdminClient } from "@/lib/admin-client";
import {
  Badge,
  Card,
  EmptyState,
  Field,
  InlineError,
  Input,
  Skeleton,
} from "@/components/ui/linear";
import { byokTone } from "./tones";

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
  // Bump to force a refetch (used by the InlineError retry).
  const [reloadKey, setReloadKey] = React.useState(0);

  React.useEffect(() => {
    let cancelled = false;
    const handle = setTimeout(() => {
      setLoading(true);
      setError(null);
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
  }, [query, debounceMs, client, reloadKey]);

  return (
    <div data-testid="tenant-search">
      <Card title="Search">
        <Field label="Search tenants" htmlFor="tenant-search-input">
          <Input
            id="tenant-search-input"
            type="search"
            data-testid="tenant-search-input"
            placeholder="Search by tenant ID or name"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </Field>
      </Card>

      <Card title="Tenants" className="lin-mt">
        {error ? (
          <div data-testid="tenant-search-error">
            <InlineError error={error} onRetry={() => setReloadKey((k) => k + 1)} />
          </div>
        ) : loading ? (
          <div aria-busy="true" aria-label="Searching tenants">
            <Skeleton rows={4} />
          </div>
        ) : results.length === 0 ? (
          <EmptyState
            title="No tenants found"
            body="Adjust your search to find a tenant in operator scope."
          />
        ) : (
          <table className="lin-table" data-testid="tenant-table" aria-busy={loading}>
            <thead>
              <tr>
                <th>Tenant ID</th>
                <th>Name</th>
                <th>Plan</th>
                <th>Region</th>
                <th>BYOK</th>
                <th>Created</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {results.map((t) => (
                <tr key={t.tenant_id} data-testid={`tenant-row-${t.tenant_id}`}>
                  <td>
                    <code>{t.tenant_id}</code>
                  </td>
                  <td>{t.name}</td>
                  <td>{t.plan}</td>
                  <td>{t.region}</td>
                  <td>
                    <Badge tone={byokTone(t.byok_status)} dot>
                      {t.byok_status}
                    </Badge>
                  </td>
                  <td>{t.created_at}</td>
                  <td>
                    <Link
                      className="lin-btn lin-btn--ghost lin-btn--sm"
                      href={`./tenants/${t.tenant_id}`}
                    >
                      View
                    </Link>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}

export default TenantSearch;
