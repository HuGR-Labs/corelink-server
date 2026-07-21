import { forwardRef } from "react";
import type {
  AnchorHTMLAttributes,
  ButtonHTMLAttributes,
  ReactNode,
} from "react";

import { withAppBasePath } from "@/lib/route-matcher";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "ghost" | "danger";
  size?: "sm" | "md";
  loading?: boolean;
  iconLeft?: ReactNode;
  /**
   * When provided, the kit renders a styled `<a href>` (same `lin-btn`
   * variant/size classes) instead of a `<button>` — the canonical way to build
   * a nav/CTA link without hand-rolling `<a className="lin-btn …">`. The
   * `<button>`-only props (`type`, `disabled`, `loading`-as-submit) do not apply
   * to the anchor form; `loading`/`iconLeft` still render.
   */
  href?: string;
  /**
   * Anchor-only `download` attribute — meaningful only alongside `href` (the
   * anchor variant). Ignored by the `<button>` form. Lets a CTA offer a file
   * download (e.g. a signed receipt) without hand-rolling `<a download>`.
   */
  download?: AnchorHTMLAttributes<HTMLAnchorElement>["download"];
  /**
   * Anchor-only `target` — meaningful only alongside `href`. Use `"_blank"` for
   * EXTERNAL links (docs, dashboards) so they open in a new tab. Ignored by the
   * `<button>` form. When set to `"_blank"`, also pass `rel="noopener noreferrer"`.
   */
  target?: AnchorHTMLAttributes<HTMLAnchorElement>["target"];
  /**
   * Anchor-only `rel` — meaningful only alongside `href` (e.g.
   * `"noopener noreferrer"` for `target="_blank"`). Ignored by the `<button>` form.
   */
  rel?: AnchorHTMLAttributes<HTMLAnchorElement>["rel"];
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    variant = "primary",
    size = "md",
    loading,
    iconLeft,
    className,
    children,
    disabled,
    type,
    href,
    download,
    target,
    rel,
    ...rest
  },
  ref,
) {
  const classes = ["lin-btn"];
  if (variant === "primary") classes.push("lin-btn--primary");
  else if (variant === "ghost") classes.push("lin-btn--ghost");
  else if (variant === "danger") classes.push("lin-btn--danger");
  if (size === "sm") classes.push("lin-btn--sm");
  if (className) classes.push(className);

  const content = (
    <>
      {loading ? <span className="lin-btn__spin" aria-hidden="true" /> : null}
      {!loading && iconLeft != null ? iconLeft : null}
      {children}
    </>
  );

  // Anchor variant — a styled link. `<button>`-only attributes (type/disabled)
  // are dropped; the remaining `rest` are anchor-compatible DOM props.
  if (href != null) {
    return (
      <a
        // Next does NOT auto-basePath a raw anchor href — prefix INTERNAL
        // absolute paths with `/corelink` so a `<Button href="/upgrade">` lands
        // on the app, not the apex marketing site (idempotent; leaves external/
        // hash/relative/already-prefixed hrefs untouched).
        href={withAppBasePath(href)}
        download={download}
        target={target}
        rel={rel}
        className={classes.join(" ")}
        {...(rest as AnchorHTMLAttributes<HTMLAnchorElement>)}
      >
        {content}
      </a>
    );
  }

  return (
    <button
      ref={ref}
      type={type ?? "button"}
      className={classes.join(" ")}
      disabled={disabled || loading}
      {...rest}
    >
      {content}
    </button>
  );
});
