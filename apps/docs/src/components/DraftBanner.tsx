/**
 * DraftBanner — DRAFT — pending Legal + Finance + Security review banner.
 *
 * Renders a high-visibility banner at the top of compliance / security /
 * pricing pages, signalling that the page content has not yet been signed off
 * by the cross-functional reviewers required by the S-18 spec contract §10
 * anti-scope clause.
 *
 * Lives under `apps/docs/src/components/` so it can be imported from every
 * MDX page under `compliance/`, `security/`, and `pricing/`.
 *
 * The cross-functional test (`tests/cross-functional.test.ts`) enforces that
 * every MDX file under those three trees renders this banner — see that test
 * for the regex that detects banner presence.
 */

import type { ReactElement } from "react";

export type DraftBannerPageType = "compliance" | "security" | "pricing";

const PAGE_LABELS: Record<DraftBannerPageType, string> = {
  compliance: "Compliance",
  security: "Security",
  pricing: "Pricing",
};

export interface DraftBannerProps {
  readonly pageType: DraftBannerPageType;
}

export function DraftBanner({ pageType }: DraftBannerProps): ReactElement {
  const label = PAGE_LABELS[pageType];
  return (
    <aside
      role="status"
      aria-label={`${label} draft notice`}
      style={{
        border: "2px solid #b45309",
        background: "#fef3c7",
        color: "#7c2d12",
        padding: "1rem",
        marginBottom: "1.5rem",
        borderRadius: "0.5rem",
        fontWeight: 600,
      }}
    >
      <strong>DRAFT — pending Legal + Finance + Security review.</strong>{" "}
      This {label.toLowerCase()} page is published in a non-binding draft state
      until the cross-functional reviewers signed off in{" "}
      <code>apps/docs/CONTENT-REVIEW.md</code> approve every claim. Do not rely
      on the contents for procurement, audit, or contractual purposes until the{" "}
      <code>draft: true</code> flag is removed from the page frontmatter.
    </aside>
  );
}
