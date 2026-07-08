// WI-S16-005 — tenant search with debounced query + client-side plan/region/
// BYOK filters. Linear kit: Card + Field + Input/Select, `lin-table`, BYOK
// Badge, kit Button for row nav, proper Skeleton/InlineError/EmptyState states.

"use client";

import React from "react";
import type { Tenant } from "@/lib/types";
import type { AdminClient } from "@/lib/admin-client";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  Field,
  InlineError,
  Input,
  Select,
  Skeleton,
} from "@/components/ui/linear";
import { byokTone } from "./tones";

export interface TenantSearchProps {
  client: AdminClient;
  debounceMs?: number;
}

const PLAN_OPTIONS: ReadonlyArray<Tenant["plan"]> = [
  "free",
  "solo",
  "starter",
  "team",
  "pro",
  "max",
  "enterprise",
];

const REGION_OPTIONS: ReadonlyArray<Tenant["region"]> = [
  "us-east",
  "us-west",
  "eu-west",
  "ap-south",
];

const BYOK_OPTIONS: ReadonlyArray<Tenant["byok_status"]> = [
  "none",
  "active",
  "rotation_pending",
];

export function TenantSearch({
  client,
  debounceMs = 200,
}: TenantSearchProps): React.ReactElement {
  const [query, setQuery] = React.useState("");
  const [planFilter, setPlanFilter] = React.useState<"all" | Tenant["plan"]>("all");
  const [regionFilter, setRegionFilter] = React.useState<"all" | Tenant["region"]>("all");
  const [byokFilter, setByokFilter] = React.useState<"all" | Tenant["byok_status"]>("all");
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

  // Plan / region / BYOK filtering is client-side over the fetched page — the
  // search endpoint only matches on id/name, so these narrow the returned set.
  const filtered = React.useMemo(
    () =>
      results.filter(
        (t) =>
          (planFilter === "all" || t.plan === planFilter) &&
          (regionFilter === "all" || t.region === regionFilter) &&
          (byokFilter === "all" || t.byok_status === byokFilter),
      ),
    [results, planFilter, regionFilter, byokFilter],
  );

  const hasSearch = query.trim().length > 0;
  const hasFilter = planFilter !== "all" || regionFilter !== "all" || byokFilter !== "all";
  const listTitle = hasSearch || hasFilter ? "Results" : "Recent tenants";
  const listMeta = !loading && !error ? `${filtered.length} shown` : undefined;

  return (
    <div data-testid="tenant-search">
      <Card title="Find a tenant">
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

        <div className="lin-mt">
          <Field label="Plan" htmlFor="tenant-filter-plan">
            <Select
              id="tenant-filter-plan"
              data-testid="tenant-filter-plan"
              value={planFilter}
              onChange={(e) => setPlanFilter(e.target.value as typeof planFilter)}
            >
              <option value="all">All plans</option>
              {PLAN_OPTIONS.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </Select>
          </Field>
        </div>

        <div className="lin-mt">
          <Field label="Region" htmlFor="tenant-filter-region">
            <Select
              id="tenant-filter-region"
              data-testid="tenant-filter-region"
              value={regionFilter}
              onChange={(e) => setRegionFilter(e.target.value as typeof regionFilter)}
            >
              <option value="all">All regions</option>
              {REGION_OPTIONS.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </Select>
          </Field>
        </div>

        <div className="lin-mt">
          <Field label="BYOK state" htmlFor="tenant-filter-byok">
            <Select
              id="tenant-filter-byok"
              data-testid="tenant-filter-byok"
              value={byokFilter}
              onChange={(e) => setByokFilter(e.target.value as typeof byokFilter)}
            >
              <option value="all">Any BYOK</option>
              {BYOK_OPTIONS.map((b) => (
                <option key={b} value={b}>
                  {b}
                </option>
              ))}
            </Select>
          </Field>
        </div>
      </Card>

      <Card title={listTitle} meta={listMeta} className="lin-mt">
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
            title={hasSearch ? "No tenants found" : "No tenants in scope"}
            body={
              hasSearch
                ? "No tenant matches that ID or name. Check the value or your operator scope."
                : "Search a tenant ID or name to begin, or check your operator scope."
            }
          />
        ) : filtered.length === 0 ? (
          <EmptyState
            title="No tenants match these filters"
            body="Clear the plan, region, or BYOK filters to widen the list."
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
              {filtered.map((t) => (
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
                    <Button href={`./tenants/${t.tenant_id}`} size="sm" variant="ghost">
                      View
                    </Button>
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
