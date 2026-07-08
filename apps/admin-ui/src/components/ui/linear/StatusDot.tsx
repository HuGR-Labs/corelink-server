export interface StatusDotProps {
  tone?: "neutral" | "success" | "warn" | "danger";
}

export function StatusDot({ tone = "neutral" }: StatusDotProps) {
  const classes = ["lin-dot"];
  if (tone === "success") classes.push("lin-dot--success");
  else if (tone === "warn") classes.push("lin-dot--warn");
  else if (tone === "danger") classes.push("lin-dot--danger");

  return <span className={classes.join(" ")} aria-hidden="true" />;
}
