"use client";

import * as React from "react";
import { cn } from "@/lib/cn";

export interface SpinnerProps {
  /** Visible-to-AT label. Defaults to "Loading…". */
  label?: string;
  size?: "sm" | "md" | "lg";
}

const sizes = {
  sm: "h-4 w-4",
  md: "h-6 w-6",
  lg: "h-10 w-10",
} as const;

export function Spinner({ label = "Loading…", size = "md" }: SpinnerProps) {
  return (
    <div role="status" aria-live="polite" className="inline-flex items-center gap-2">
      <span
        aria-hidden="true"
        className={cn(
          "inline-block animate-spin rounded-full border-2 border-slate-300 border-t-blue-700",
          sizes[size]
        )}
      />
      <span className="sr-only">{label}</span>
    </div>
  );
}
