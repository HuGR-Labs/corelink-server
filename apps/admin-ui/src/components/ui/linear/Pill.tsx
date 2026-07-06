import type { ReactNode } from "react";

export interface PillProps {
  children: ReactNode;
}

export function Pill({ children }: PillProps) {
  return <span className="lin-pill">{children}</span>;
}
