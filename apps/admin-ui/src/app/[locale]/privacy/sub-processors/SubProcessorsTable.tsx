"use client";

import * as React from "react";
import { formatDate } from "@/i18n/format";
import type { Locale } from "@/i18n/LocaleContext";
import type { SubProcessor } from "@/content/load";

/**
 * SubProcessorsTable — Linear-doctrine (`.lin-table`) table for the public
 * sub-processors register. The shared `@/components/ui/Table` is hard
 * light-themed (`bg-slate-50` / `text-slate-900`, no override) and would render
 * dark-on-dark on the public shell, so this surface-specific table applies the
 * frozen kit table styles with token colours. Semantics preserved: `role=table`
 * with an accessible name from `<caption>`, and per-column sort `<button>`s for
 * the sortable columns (name / region / last audit).
 */
export interface SubProcessorsTableProps {
  locale: Locale;
  caption: string;
  headers: { name: string; role: string; region: string; certs: string; audit: string };
  items: SubProcessor[];
}

type SortKey = "name" | "region" | "last_audit";

export function SubProcessorsTable({
  locale,
  caption,
  headers,
  items,
}: SubProcessorsTableProps): React.ReactElement {
  const [sortKey, setSortKey] = React.useState<SortKey | null>(null);
  const [asc, setAsc] = React.useState(true);

  const sorted = React.useMemo(() => {
    if (sortKey == null) return items;
    const copy = [...items];
    copy.sort((a, b) => {
      const av = String(a[sortKey]);
      const bv = String(b[sortKey]);
      return asc ? av.localeCompare(bv) : bv.localeCompare(av);
    });
    return copy;
  }, [items, sortKey, asc]);

  function onSort(key: SortKey) {
    if (sortKey === key) setAsc((v) => !v);
    else {
      setSortKey(key);
      setAsc(true);
    }
  }

  function ariaSort(key: SortKey): "ascending" | "descending" | "none" {
    if (sortKey !== key) return "none";
    return asc ? "ascending" : "descending";
  }

  function SortButton({ label, sortKey: key }: { label: string; sortKey: SortKey }) {
    return (
      <button
        type="button"
        onClick={() => onSort(key)}
        className="inline-flex items-center gap-1 text-[var(--t3)] underline-offset-2 transition-colors hover:text-[var(--t1)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--line-2)]"
      >
        {label}
        <span aria-hidden="true">
          {sortKey === key && (asc ? "↑" : "↓")}
        </span>
      </button>
    );
  }

  return (
    <table className="lin-table">
      <caption className="sr-only">{caption}</caption>
      <thead>
        <tr>
          <th scope="col" aria-sort={ariaSort("name")}>
            <SortButton label={headers.name} sortKey="name" />
          </th>
          <th scope="col">{headers.role}</th>
          <th scope="col" aria-sort={ariaSort("region")}>
            <SortButton label={headers.region} sortKey="region" />
          </th>
          <th scope="col">{headers.certs}</th>
          <th scope="col" aria-sort={ariaSort("last_audit")}>
            <SortButton label={headers.audit} sortKey="last_audit" />
          </th>
        </tr>
      </thead>
      <tbody>
        {sorted.map((row) => (
          <tr key={row.id}>
            <td className="text-[var(--t1)]">{row.name}</td>
            <td>{row.role}</td>
            <td>{row.region}</td>
            <td>{row.certifications.join(", ")}</td>
            <td>{formatDate(row.last_audit, locale)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export default SubProcessorsTable;
