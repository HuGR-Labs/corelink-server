import type { ReactNode } from "react";

export interface StatProps {
  label: string;
  value: ReactNode;
  sub?: string;
  trend?: { dir: "up" | "down"; label: string };
}

export function Stat({ label, value, sub, trend }: StatProps) {
  return (
    <div>
      <div className="lin-stat__label">{label}</div>
      <div className="lin-stat__value">{value}</div>
      {sub != null ? <div className="lin-stat__sub">{sub}</div> : null}
      {trend != null ? (
        <div
          className={
            trend.dir === "up" ? "lin-stat__trend--up" : "lin-stat__trend--down"
          }
        >
          {trend.label}
        </div>
      ) : null}
    </div>
  );
}
