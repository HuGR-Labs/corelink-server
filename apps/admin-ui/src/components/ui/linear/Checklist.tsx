import type { ReactNode } from "react";

export interface ChecklistItem {
  id: string;
  done: boolean;
  label: ReactNode;
  cta?: ReactNode;
}

export interface ChecklistProps {
  items: Array<ChecklistItem>;
}

function CheckMark() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="3"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <polyline points="20 6 9 17 4 12" />
    </svg>
  );
}

export function Checklist({ items }: ChecklistProps) {
  return (
    <div className="lin-checklist">
      {items.map((item) => (
        <div key={item.id} className="lin-check" data-done={item.done}>
          <span className="lin-check__mark">{item.done ? <CheckMark /> : null}</span>
          <span className="lin-check__label">{item.label}</span>
          {item.cta != null ? item.cta : null}
        </div>
      ))}
    </div>
  );
}
