"use client";

import * as React from "react";

export interface Crumb {
  label: string;
  href?: string;
}

export interface BreadcrumbsProps {
  items: Crumb[];
  /** Override the accessible label. */
  label?: string;
}

export function Breadcrumbs({ items, label = "Breadcrumb" }: BreadcrumbsProps) {
  return (
    <nav aria-label={label}>
      <ol className="flex flex-wrap items-center gap-1 text-sm text-slate-700">
        {items.map((c, idx) => {
          const last = idx === items.length - 1;
          return (
            <li key={`${c.label}-${idx}`} className="flex items-center gap-1">
              {c.href && !last ? (
                <a
                  href={c.href}
                  className="underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700"
                >
                  {c.label}
                </a>
              ) : (
                <span aria-current={last ? "page" : undefined} className="font-medium text-slate-900">
                  {c.label}
                </span>
              )}
              {!last && (
                <span aria-hidden="true" className="text-slate-400">
                  /
                </span>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
