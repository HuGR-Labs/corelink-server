import type { ReactNode } from "react";

export interface BadgeProps {
  tone?: "neutral" | "success" | "warn" | "danger";
  dot?: boolean;
  children: ReactNode;
}

export function Badge({ tone = "neutral", dot, children }: BadgeProps) {
  const classes = ["lin-badge"];
  if (tone === "success") classes.push("lin-badge--success");
  else if (tone === "warn") classes.push("lin-badge--warn");
  else if (tone === "danger") classes.push("lin-badge--danger");

  const dotClasses = ["lin-dot"];
  if (tone === "success") dotClasses.push("lin-dot--success");
  else if (tone === "warn") dotClasses.push("lin-dot--warn");
  else if (tone === "danger") dotClasses.push("lin-dot--danger");

  return (
    <span className={classes.join(" ")}>
      {dot ? <span className={dotClasses.join(" ")} aria-hidden="true" /> : null}
      {children}
    </span>
  );
}
