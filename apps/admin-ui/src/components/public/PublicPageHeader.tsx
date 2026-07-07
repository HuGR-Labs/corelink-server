import * as React from "react";

/**
 * PublicPageHeader — the Linear-doctrine page header for the public/legal
 * surfaces (replaces the light `@/components/layout/PageHeader` on the dark
 * shell). Title on `--t1`, description on `--t2` (both WCAG-AA on `--bg`),
 * hairline divider on `--line`. Tokens-only; page-h1 scale (24px / 560).
 */
export interface PublicPageHeaderProps {
  title: string;
  description?: React.ReactNode;
  actions?: React.ReactNode;
}

export function PublicPageHeader({
  title,
  description,
  actions,
}: PublicPageHeaderProps): React.ReactElement {
  return (
    <header className="mb-6 flex items-start justify-between gap-4 border-b border-[var(--line)] pb-4">
      <div>
        <h1 className="text-2xl font-[560] tracking-[-0.022em] text-[var(--t1)]">
          {title}
        </h1>
        {description != null ? (
          <p className="mt-1.5 text-sm text-[var(--t2)]">{description}</p>
        ) : null}
      </div>
      {actions != null ? (
        <div className="flex flex-shrink-0 items-center gap-2">{actions}</div>
      ) : null}
    </header>
  );
}

export default PublicPageHeader;
