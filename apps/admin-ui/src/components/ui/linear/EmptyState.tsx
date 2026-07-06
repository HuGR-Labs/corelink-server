import type { ReactNode } from "react";

export interface EmptyStateProps {
  icon?: ReactNode;
  title: string;
  body?: string;
  cta?: ReactNode;
}

export function EmptyState({ icon, title, body, cta }: EmptyStateProps) {
  return (
    <div className="lin-empty">
      {icon != null ? <div className="lin-empty__ic">{icon}</div> : null}
      <div className="lin-empty__title">{title}</div>
      {body != null ? <div className="lin-empty__body">{body}</div> : null}
      {cta != null ? <div className="lin-empty__cta">{cta}</div> : null}
    </div>
  );
}
