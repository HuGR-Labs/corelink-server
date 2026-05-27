/**
 * Navbar/Content — wrap swizzle of the Docusaurus classic theme's navbar
 * content slot.
 *
 * Why a wrap, not an eject:
 *   - The upstream `Navbar/Content` is ~100 LoC of layout glue (mobile
 *     toggle, items splitter, search wiring). Ejecting it would lock
 *     us to the current Docusaurus version's internals and break on
 *     upgrade.
 *   - The wrap pattern keeps the upstream component as the source of
 *     truth and only adds one extra child: our `<StatusPill>`.
 *   - We mount the pill via a client-only effect into the right side
 *     of the navbar so it sits next to the GitHub / locale switcher /
 *     search items.
 *
 * Mount strategy:
 *   - The original component renders a `.navbar__items--right` div on
 *     desktop. We use a small client-side effect to portal the
 *     `<StatusPill>` into that container so we don't have to fork the
 *     upstream layout markup.
 *   - On SSR pass we render the original untouched (no `<StatusPill>`),
 *     so first paint is identical to the upstream Docusaurus output and
 *     the hydration is non-disruptive.
 *
 * Statuspage URL: read from `siteConfig.customFields.statuspageUrl`
 * (already wired by the canonical `getStatuspageUrl()` helper in
 * `docusaurus.config.ts`). Operators override via the `STATUSPAGE_URL`
 * env var without touching this file.
 */

import { useEffect, useState, type ReactElement } from "react";
import { createPortal } from "react-dom";
import OriginalNavbarContent from "@theme-original/Navbar/Content";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";
import StatusPill from "@site/src/components/StatusPill/StatusPill";

interface StatuspageCustomFields {
  statuspageUrl?: string;
}

export default function NavbarContentWrapper(): ReactElement {
  const { siteConfig } = useDocusaurusContext();
  const customFields = (siteConfig.customFields ?? {}) as StatuspageCustomFields;
  const statuspageUrl =
    typeof customFields.statuspageUrl === "string" && customFields.statuspageUrl.length > 0
      ? customFields.statuspageUrl
      : "https://status.corelink.humangr.com";

  const [host, setHost] = useState<HTMLElement | null>(null);

  useEffect(() => {
    // Locate the right-side navbar container. Docusaurus emits the
    // element after the navbar mounts; we wait one tick to be safe.
    let cancelled = false;
    const find = (): void => {
      if (cancelled) return;
      const target = document.querySelector<HTMLElement>(".navbar__items--right");
      if (target) {
        // Reuse an existing host if a previous render already created
        // one (e.g. after a client-side route change). This keeps the
        // portal stable across navigations.
        let mount = target.querySelector<HTMLElement>(
          ".navbar__item--status-pill",
        );
        if (!mount) {
          mount = document.createElement("div");
          mount.className = "navbar__item navbar__item--status-pill";
          // Insert as the FIRST child of the right-side group so the
          // pill sits left of GitHub / locale dropdown / search.
          target.insertBefore(mount, target.firstChild);
        }
        setHost(mount);
      } else {
        // The navbar might not have mounted yet on the very first paint
        // (rare, but possible during hydration). Retry once on rAF.
        requestAnimationFrame(find);
      }
    };
    find();
    return (): void => {
      cancelled = true;
    };
  }, []);

  return (
    <>
      <OriginalNavbarContent />
      {host ? createPortal(<StatusPill statuspageUrl={statuspageUrl} />, host) : null}
    </>
  );
}
