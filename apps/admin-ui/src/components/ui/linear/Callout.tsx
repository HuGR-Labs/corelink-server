import type { ReactNode } from "react";

export interface CalloutProps {
  tone?: "info" | "warn" | "danger";
  children: ReactNode;
}

function CalloutIcon({ tone }: { tone: "info" | "warn" | "danger" }) {
  if (tone === "info") {
    return (
      <svg
        width="15"
        height="15"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <circle cx="12" cy="12" r="10" />
        <line x1="12" y1="16" x2="12" y2="12" />
        <line x1="12" y1="8" x2="12.01" y2="8" />
      </svg>
    );
  }
  // warn + danger share a triangle-alert glyph (color comes from border/tone context)
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0Z" />
      <line x1="12" y1="9" x2="12" y2="13" />
      <line x1="12" y1="17" x2="12.01" y2="17" />
    </svg>
  );
}

export function Callout({ tone = "info", children }: CalloutProps) {
  const classes = ["lin-callout"];
  if (tone === "warn") classes.push("lin-callout--warn");
  else if (tone === "danger") classes.push("lin-callout--danger");

  return (
    <div className={classes.join(" ")}>
      <span className="lin-callout__ic">
        <CalloutIcon tone={tone} />
      </span>
      <div>{children}</div>
    </div>
  );
}
