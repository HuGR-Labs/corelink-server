"use client";

import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";

export interface MenuProps {
  trigger: ReactNode;
  children: ReactNode;
}

export function Menu({ trigger, children }: MenuProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (rootRef.current != null && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);

  return (
    <div ref={rootRef} style={{ position: "relative", display: "inline-flex" }}>
      <span onClick={() => setOpen((v) => !v)}>{trigger}</span>
      {open ? (
        <div
          className="lin-menu"
          role="menu"
          style={{ position: "absolute", top: "calc(100% + 6px)", right: 0 }}
        >
          {children}
        </div>
      ) : null}
    </div>
  );
}

export interface MenuItemProps {
  onClick?: () => void;
  children: ReactNode;
}

export function MenuItem({ onClick, children }: MenuItemProps) {
  return (
    <button
      type="button"
      className="lin-menu__item"
      role="menuitem"
      onClick={onClick}
      style={{ width: "100%", textAlign: "left", background: "transparent", border: 0 }}
    >
      {children}
    </button>
  );
}

export function MenuSep() {
  return <div className="lin-menu__sep" role="separator" />;
}
