"use client";

import * as React from "react";
import { cn } from "@/lib/cn";

export type SortDirection = "ascending" | "descending" | "none";

export interface TableColumn<T> {
  key: keyof T & string;
  header: string;
  sortable?: boolean;
  render?: (row: T) => React.ReactNode;
}

export interface TableProps<T extends { id: string }> {
  /** Accessible caption (visually hidden by default). */
  caption: string;
  hideCaption?: boolean;
  columns: TableColumn<T>[];
  rows: T[];
  /** Enables row-select via checkbox column. */
  selectable?: boolean;
  selectedIds?: Set<string>;
  onSelectChange?: (ids: Set<string>) => void;
  sortKey?: string;
  sortDirection?: SortDirection;
  onSortChange?: (key: string, direction: SortDirection) => void;
}

export function Table<T extends { id: string }>({
  caption,
  hideCaption = true,
  columns,
  rows,
  selectable,
  selectedIds,
  onSelectChange,
  sortKey,
  sortDirection = "none",
  onSortChange,
}: TableProps<T>) {
  const allSelected = selectable && rows.length > 0 && rows.every((r) => selectedIds?.has(r.id));
  const someSelected = selectable && rows.some((r) => selectedIds?.has(r.id)) && !allSelected;

  function toggleAll() {
    if (!onSelectChange) return;
    if (allSelected) onSelectChange(new Set());
    else onSelectChange(new Set(rows.map((r) => r.id)));
  }

  function toggleRow(id: string) {
    if (!onSelectChange) return;
    const next = new Set(selectedIds ?? []);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    onSelectChange(next);
  }

  function handleSort(key: string) {
    if (!onSortChange) return;
    const dir: SortDirection =
      sortKey === key && sortDirection === "ascending" ? "descending" : "ascending";
    onSortChange(key, dir);
  }

  return (
    <table className="w-full border-collapse text-left text-sm">
      <caption className={cn(hideCaption && "sr-only")}>{caption}</caption>
      <thead className="border-b border-slate-200 bg-slate-50">
        <tr>
          {selectable && (
            <th scope="col" className="px-3 py-2">
              <input
                type="checkbox"
                aria-label="Select all rows"
                checked={!!allSelected}
                ref={(el) => {
                  if (el) el.indeterminate = !!someSelected;
                }}
                onChange={toggleAll}
              />
            </th>
          )}
          {columns.map((col) => (
            <th
              key={col.key}
              scope="col"
              aria-sort={
                col.sortable
                  ? sortKey === col.key
                    ? sortDirection
                    : "none"
                  : undefined
              }
              className="px-3 py-2 font-semibold text-slate-900"
            >
              {col.sortable ? (
                <button
                  type="button"
                  onClick={() => handleSort(col.key)}
                  className="inline-flex items-center gap-1 underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700"
                >
                  {col.header}
                  <span aria-hidden="true">
                    {sortKey === col.key && sortDirection === "ascending" && "↑"}
                    {sortKey === col.key && sortDirection === "descending" && "↓"}
                  </span>
                </button>
              ) : (
                col.header
              )}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => {
          const selected = selectedIds?.has(row.id) ?? false;
          return (
            <tr key={row.id} className="border-b border-slate-100">
              {selectable && (
                <td className="px-3 py-2">
                  <input
                    type="checkbox"
                    aria-label={`Select row ${row.id}`}
                    checked={selected}
                    onChange={() => toggleRow(row.id)}
                  />
                </td>
              )}
              {columns.map((col) => (
                <td key={col.key} className="px-3 py-2 text-slate-900">
                  {col.render ? col.render(row) : String(row[col.key] ?? "")}
                </td>
              ))}
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
