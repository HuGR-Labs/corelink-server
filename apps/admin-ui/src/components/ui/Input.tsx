"use client";

import * as React from "react";
import { cn } from "@/lib/cn";

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label: string;
  hint?: string;
  error?: string;
  /** Hide the visual label but keep it for screen readers. */
  hideLabel?: boolean;
}

let counter = 0;
function useId(prefix: string): string {
  const ref = React.useRef<string | null>(null);
  if (ref.current === null) {
    counter += 1;
    ref.current = `${prefix}-${counter}`;
  }
  return ref.current;
}

export const Input = React.forwardRef<HTMLInputElement, InputProps>(function Input(
  { label, hint, error, hideLabel, id, className, "aria-describedby": describedBy, ...props },
  ref
) {
  const autoId = useId("input");
  const inputId = id ?? autoId;
  const hintId = hint ? `${inputId}-hint` : undefined;
  const errorId = error ? `${inputId}-error` : undefined;
  const describedByIds = [describedBy, hintId, errorId].filter(Boolean).join(" ") || undefined;

  return (
    <div className="flex flex-col gap-1">
      <label
        htmlFor={inputId}
        className={cn(
          "text-sm font-medium text-slate-900",
          hideLabel && "sr-only"
        )}
      >
        {label}
      </label>
      {hint && (
        <span id={hintId} className="text-sm text-slate-600">
          {hint}
        </span>
      )}
      <input
        ref={ref}
        id={inputId}
        aria-describedby={describedByIds}
        aria-invalid={error ? true : undefined}
        className={cn(
          "min-h-[40px] rounded-md border bg-white px-3 text-base",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2",
          error ? "border-red-700" : "border-slate-300",
          className
        )}
        {...props}
      />
      {error && (
        <span id={errorId} role="alert" className="text-sm text-red-700">
          {error}
        </span>
      )}
    </div>
  );
});
