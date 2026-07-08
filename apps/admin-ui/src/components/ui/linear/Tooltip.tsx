"use client";

import { useId, useState } from "react";
import type { ReactNode } from "react";

export interface TooltipProps {
  label: string;
  children: ReactNode;
}

export function Tooltip({ label, children }: TooltipProps) {
  const [open, setOpen] = useState(false);
  const tipId = useId();

  return (
    <span
      style={{ position: "relative", display: "inline-flex" }}
      tabIndex={0}
      aria-describedby={open ? tipId : undefined}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
    >
      {children}
      {open ? (
        <span
          id={tipId}
          role="tooltip"
          className="lin-pop"
          style={{
            position: "absolute",
            top: "calc(100% + 6px)",
            left: 0,
            whiteSpace: "nowrap",
          }}
        >
          {label}
        </span>
      ) : null}
    </span>
  );
}
