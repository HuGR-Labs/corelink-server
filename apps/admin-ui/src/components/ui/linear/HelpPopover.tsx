"use client";

import { useId, useState } from "react";
import type { ReactNode } from "react";

export interface HelpPopoverProps {
  children: ReactNode;
  label?: string;
}

export function HelpPopover({ children, label = "More info" }: HelpPopoverProps) {
  const [open, setOpen] = useState(false);
  const popId = useId();

  return (
    <span
      style={{ position: "relative", display: "inline-flex" }}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <button
        type="button"
        className="lin-help"
        aria-label={label}
        aria-expanded={open}
        aria-describedby={open ? popId : undefined}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onClick={() => setOpen((v) => !v)}
      >
        ?
      </button>
      {open ? (
        <span
          id={popId}
          role="tooltip"
          className="lin-pop"
          style={{ position: "absolute", top: "calc(100% + 6px)", left: 0 }}
        >
          {children}
        </span>
      ) : null}
    </span>
  );
}
