"use client";

import * as React from "react";
import { cn } from "@/lib/cn";

export interface SkeletonProps extends React.HTMLAttributes<HTMLDivElement> {
  /** Optional explicit height. */
  height?: number | string;
  /** Optional explicit width. */
  width?: number | string;
}

export function Skeleton({ className, height, width, style, ...rest }: SkeletonProps) {
  return (
    <div
      aria-hidden="true"
      aria-busy="true"
      role="presentation"
      className={cn("animate-pulse rounded bg-slate-200", className)}
      style={{ height: height ?? "1rem", width: width ?? "100%", ...style }}
      {...rest}
    />
  );
}
