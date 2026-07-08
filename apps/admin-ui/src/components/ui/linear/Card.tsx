import type { ReactNode } from "react";

export interface CardProps {
  title?: string;
  meta?: string;
  actions?: ReactNode;
  padded?: boolean;
  hover?: boolean;
  className?: string;
  children: ReactNode;
}

export function Card({
  title,
  meta,
  actions,
  padded = true,
  hover,
  className,
  children,
}: CardProps) {
  const classes = ["lin-card"];
  if (padded) classes.push("lin-card--pad");
  if (hover) classes.push("lin-card--hover");
  if (className) classes.push(className);

  const showHead = title != null || actions != null;

  return (
    <div className={classes.join(" ")}>
      {showHead ? (
        <div className="lin-card__head">
          <div>
            {title != null ? <h3 className="lin-card__title">{title}</h3> : null}
            {meta != null ? <div className="lin-card__meta">{meta}</div> : null}
          </div>
          {actions != null ? <div className="lin-card__actions">{actions}</div> : null}
        </div>
      ) : null}
      {children}
    </div>
  );
}
