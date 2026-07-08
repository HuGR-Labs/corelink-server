import * as React from "react";

/**
 * PublicShell — the Linear-doctrine dark surface for the PUBLIC / marketing /
 * legal pages (pricing · privacy · security policy · legal docs · 403).
 *
 * The customer dashboard's dark theme is scoped to `.cx-shell` in globals.css
 * (so the light landing page keeps its own styling + Lighthouse contrast). The
 * public marketing/legal pages opt INTO the same Linear language by rendering
 * inside this shell, which:
 *   1. establishes the dark canvas (`bg-[var(--bg)]`, full-viewport) so text on
 *      the a11y-validated `--t1`/`--t2` tokens keeps WCAG-AA contrast — the same
 *      background `.cx-shell` sets, reproduced via the token (no raw hex);
 *   2. applies the frozen kit scope (`.lin`) for focus rings + base typography;
 *   3. centres a ≤1040px content column (dashboard rule) with the 40/44 desktop
 *      / 28/20 mobile page padding from the spacing grid.
 *
 * Tokens-only: colours are `var(--*)`, spacing is the 4px grid (`p-[…var(--sN)]`
 * style utilities are avoided — Tailwind grid steps map 1:1: 4=1,8=2,12=3,16=4,
 * 20=5,24=6,32=8,40=10,44=11). No inline styles, no off-grid px.
 */
export interface PublicShellProps {
  /** Wrap children in a `<main id="main">` landmark. Default true. */
  main?: boolean;
  /** Content max-width column. `prose` = ≤66ch legal measure; `wide` = 1040px. */
  width?: "prose" | "wide";
  className?: string;
  /** Forwarded onto the landmark element (test hook). */
  "data-testid"?: string;
  children: React.ReactNode;
}

export function PublicShell({
  main = true,
  width = "prose",
  className,
  "data-testid": testId,
  children,
}: PublicShellProps): React.ReactElement {
  const col =
    width === "prose"
      ? "mx-auto w-full max-w-[68ch]"
      : "mx-auto w-full max-w-[1040px]";
  const inner = <div className={col}>{children}</div>;
  return (
    <div className="lin min-h-screen w-full bg-[var(--bg)] text-[var(--t2)]">
      <div className="px-5 py-10 sm:px-11 sm:py-10">
        {main ? (
          <main id="main" className={className} data-testid={testId}>
            {inner}
          </main>
        ) : (
          <div className={className} data-testid={testId}>
            {inner}
          </div>
        )}
      </div>
    </div>
  );
}

export default PublicShell;
