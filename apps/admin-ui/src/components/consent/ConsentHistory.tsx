"use client";

import { useEffect, useState } from "react";
import type { ConsentApi, ConsentHistoryQuery } from "@/lib/consent-api";
import { defaultConsentApi } from "@/lib/consent-api";
import type { ConsentRow } from "@/lib/consent-types";

const PAGE_SIZE = 20;

export interface ConsentHistoryProps {
  api?: ConsentApi;
}

interface Filters {
  status: "" | "active" | "withdrawn";
  purpose: string;
  date_from: string;
  date_to: string;
}

export function ConsentHistory({ api = defaultConsentApi }: ConsentHistoryProps) {
  const [page, setPage] = useState(1);
  const [filters, setFilters] = useState<Filters>({
    status: "",
    purpose: "",
    date_from: "",
    date_to: "",
  });
  const [rows, setRows] = useState<ConsentRow[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    const q: ConsentHistoryQuery = {
      page,
      page_size: PAGE_SIZE,
      status: filters.status === "" ? undefined : filters.status,
      purpose: filters.purpose || undefined,
      date_from: filters.date_from || undefined,
      date_to: filters.date_to || undefined,
    };
    api
      .history(q)
      .then((res) => {
        if (cancelled) return;
        setRows(res.rows);
        setTotal(res.total);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : "history_failed");
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api, page, filters]);

  function updateFilter<K extends keyof Filters>(key: K, value: Filters[K]) {
    setFilters((f) => ({ ...f, [key]: value }));
    setPage(1);
  }

  const maxPage = Math.max(1, Math.ceil(total / PAGE_SIZE));

  return (
    <section aria-label="Consent history" data-testid="consent-history">
      <h1>History</h1>
      <fieldset>
        <legend>Filters</legend>
        <label htmlFor="filter-status">Status</label>
        <select
          id="filter-status"
          data-testid="filter-status"
          value={filters.status}
          onChange={(e) => updateFilter("status", e.target.value as Filters["status"])}
        >
          <option value="">all</option>
          <option value="active">active</option>
          <option value="withdrawn">withdrawn</option>
        </select>

        <label htmlFor="filter-purpose">Purpose</label>
        <input
          id="filter-purpose"
          data-testid="filter-purpose"
          value={filters.purpose}
          onChange={(e) => updateFilter("purpose", e.target.value)}
        />

        <label htmlFor="filter-from">Date from</label>
        <input
          id="filter-from"
          type="date"
          data-testid="filter-from"
          value={filters.date_from}
          onChange={(e) => updateFilter("date_from", e.target.value)}
        />

        <label htmlFor="filter-to">Date to</label>
        <input
          id="filter-to"
          type="date"
          data-testid="filter-to"
          value={filters.date_to}
          onChange={(e) => updateFilter("date_to", e.target.value)}
        />
      </fieldset>

      {loading && <p data-testid="history-loading">Loading...</p>}
      {error && <p role="alert" data-testid="history-error">{error}</p>}

      <table>
        <thead>
          <tr>
            <th scope="col">Purpose</th>
            <th scope="col">Granted at</th>
            <th scope="col">Status</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.id} data-testid={`history-row-${r.id}`}>
              <td>{r.purpose}</td>
              <td>{r.granted_at}</td>
              <td>{r.status}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <nav aria-label="Pagination">
        <button
          type="button"
          onClick={() => setPage((p) => Math.max(1, p - 1))}
          disabled={page <= 1}
          data-testid="page-prev"
        >
          Previous
        </button>
        <span data-testid="page-indicator">
          Page {page} / {maxPage}
        </span>
        <button
          type="button"
          onClick={() => setPage((p) => Math.min(maxPage, p + 1))}
          disabled={page >= maxPage}
          data-testid="page-next"
        >
          Next
        </button>
      </nav>
    </section>
  );
}
