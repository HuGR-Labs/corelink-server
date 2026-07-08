import * as React from "react";
import { MarkdownView } from "@/components/content/MarkdownView";

/**
 * LegalProse — dark, Linear-doctrine prose for the legal/privacy markdown
 * bodies. `@tailwindcss/typography` is NOT installed in this app, so the shared
 * `MarkdownView`'s `prose` classes are inert; this wrapper supplies the reading
 * styles explicitly via token-driven Tailwind descendant selectors.
 *
 * a11y (HARD GATE — /en/privacy is Lighthouse a11y=100): body copy sits on
 * `--t2`, headings/strong/links/code on `--t1` — both WCAG-AA verified on
 * `--bg`. Never `--t4` for text. Measure is capped at ≤66ch by the shell's
 * prose column + `max-w-[66ch]` here; generous line-height (`leading-7`).
 * Tokens-only: colours are `var(--*)`, spacing on the 4px grid, radii via the
 * `--r-*` tokens. No raw hex, no inline styles.
 */
export interface LegalProseProps {
  content: string;
}

const PROSE = [
  "max-w-[66ch] text-[15px] leading-7 text-[var(--t2)]",
  // headings
  "[&_h1]:mt-10 [&_h1]:mb-4 [&_h1]:text-2xl [&_h1]:font-[560] [&_h1]:tracking-[-0.022em] [&_h1]:text-[var(--t1)]",
  "[&_h2]:mt-8 [&_h2]:mb-3 [&_h2]:text-lg [&_h2]:font-[560] [&_h2]:tracking-[-0.012em] [&_h2]:text-[var(--t1)]",
  "[&_h3]:mt-6 [&_h3]:mb-2 [&_h3]:text-base [&_h3]:font-[560] [&_h3]:text-[var(--t1)]",
  "[&_h4]:mt-5 [&_h4]:mb-2 [&_h4]:text-sm [&_h4]:font-[560] [&_h4]:text-[var(--t1)]",
  // paragraphs + lists
  "[&_p]:my-4 [&_p]:text-[var(--t2)]",
  "[&_ul]:my-4 [&_ul]:list-disc [&_ul]:pl-6 [&_ol]:my-4 [&_ol]:list-decimal [&_ol]:pl-6",
  "[&_li]:my-1 [&_li]:text-[var(--t2)] [&_li]:marker:text-[var(--t3)]",
  // inline
  "[&_strong]:font-[560] [&_strong]:text-[var(--t1)]",
  "[&_a]:text-[var(--t1)] [&_a]:underline [&_a]:decoration-[var(--line-2)] [&_a]:underline-offset-2 hover:[&_a]:decoration-[var(--t2)]",
  "[&_code]:rounded-[var(--r-chip)] [&_code]:bg-[rgba(255,255,255,0.06)] [&_code]:px-1.5 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[0.9em] [&_code]:text-[var(--t1)]",
  // blockquote
  "[&_blockquote]:my-4 [&_blockquote]:border-l-2 [&_blockquote]:border-[var(--line-2)] [&_blockquote]:pl-4 [&_blockquote]:text-[var(--t3)]",
  // rules + tables
  "[&_hr]:my-8 [&_hr]:border-[var(--line)]",
  "[&_table]:my-5 [&_table]:w-full [&_table]:border-collapse [&_table]:text-sm",
  "[&_th]:border-b [&_th]:border-[var(--line)] [&_th]:px-3 [&_th]:py-2 [&_th]:text-left [&_th]:font-[510] [&_th]:text-[var(--t3)]",
  "[&_td]:border-b [&_td]:border-[var(--line)] [&_td]:px-3 [&_td]:py-2 [&_td]:text-[var(--t2)]",
].join(" ");

export function LegalProse({ content }: LegalProseProps): React.ReactElement {
  return <MarkdownView content={content} className={PROSE} />;
}

export default LegalProse;
