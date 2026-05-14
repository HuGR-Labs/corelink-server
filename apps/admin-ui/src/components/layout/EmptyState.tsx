"use client";

import * as React from "react";

export interface EmptyStateProps {
  /** Optional decorative icon. */
  icon?: React.ReactNode;
  heading: string;
  body?: string;
  cta?: React.ReactNode;
}

export function EmptyState({ icon, heading, body, cta }: EmptyStateProps) {
  return (
    <div role="status" className="mx-auto max-w-md py-12 text-center">
      {icon && (
        <div aria-hidden="true" className="mb-3 inline-flex">
          {icon}
        </div>
      )}
      <h2 className="text-lg font-semibold text-slate-900">{heading}</h2>
      {body && <p className="mt-1 text-sm text-slate-700">{body}</p>}
      {cta && <div className="mt-4">{cta}</div>}
    </div>
  );
}
