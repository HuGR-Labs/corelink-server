"use client";

// Phase 0.G — Plausible install on admin-ui.
//
// Gated by the existing cookie-consent `analytics` category (see
// `apps/admin-ui/src/lib/consent.ts` + `CookiePolicyPage.tsx`). The
// component renders nothing on server-side render and on first client
// render, then mounts the script tag only after observing that the
// consent cookie has the `analytics` grant set to `true`.
//
// Why a client component instead of `<script>` in `layout.tsx`:
//   - Server-rendering the script would load Plausible before consent
//     has been observed (violates LGPD Art. 5 II free-and-informed gate).
//   - Reading `document.cookie` in a Server Component is impossible
//     anyway (the cookie is browser-side state, not request state).
//
// The admin-ui Plausible domain is separate from the docs-site Plausible
// domain so the funnels can be inspected independently:
//     docs:       data-domain="corelink-docs.humangr.com"
//     admin-ui:   data-domain="corelink-admin.humangr.com"

import { useEffect, useState } from "react";

const CONSENT_COOKIE = "corelink_consent_v1";

/**
 * Read the consent cookie (set by `lib/consent-api.ts` post-grant). Returns
 * true if `analytics` is granted; false on any parse error so a malformed
 * cookie defaults to "no tracking".
 */
function analyticsConsented(): boolean {
    if (typeof document === "undefined") return false;
    const match = document.cookie
        .split(";")
        .map(s => s.trim())
        .find(s => s.startsWith(`${CONSENT_COOKIE}=`));
    if (!match) return false;
    try {
        const raw = decodeURIComponent(match.slice(CONSENT_COOKIE.length + 1));
        const parsed = JSON.parse(raw) as { analytics?: boolean };
        return parsed.analytics === true;
    } catch {
        return false;
    }
}

export function PlausibleScript({
    domain = "corelink-admin.humangr.com",
}: { domain?: string }) {
    const [consented, setConsented] = useState(false);

    useEffect(() => {
        setConsented(analyticsConsented());
        // Listen for cookie updates so consent changes within the SPA
        // session take effect without a page reload.
        const onStorage = () => setConsented(analyticsConsented());
        window.addEventListener("storage", onStorage);
        return () => window.removeEventListener("storage", onStorage);
    }, []);

    useEffect(() => {
        if (!consented) return;
        if (document.querySelector("script[data-corelink-plausible]")) return;
        const s = document.createElement("script");
        s.defer = true;
        s.src = "https://plausible.io/js/script.js";
        s.setAttribute("data-domain", domain);
        s.setAttribute("data-corelink-plausible", "1");
        document.head.appendChild(s);
    }, [consented, domain]);

    return null;
}
