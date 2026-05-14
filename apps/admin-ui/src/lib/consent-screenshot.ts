// EVT-012 — screenshot evidence capture (client-side primary path).
//
// PRIVACY GUARD: the consent form root carries the `privacy-no-capture` CSS
// class. Any client-side analytics or session-replay tool MUST honour this
// class as an opt-out marker. See `ConsentForm.tsx` and the comment in
// `apps/admin-ui/src/app/[locale]/consent/new/page.tsx` — no analytics tooling
// is wired in this WI; this guard documents the invariant for WI-S16-007
// observability hardening.

import { safeLog } from "@/lib/safe-log";

export type Html2CanvasFn = (
  element: HTMLElement,
  options?: { backgroundColor?: string | null; logging?: boolean },
) => Promise<HTMLCanvasElement>;

let html2canvasImpl: Html2CanvasFn | null = null;

/** Test hook — inject html2canvas without importing the real bundle. */
export function __setHtml2Canvas(fn: Html2CanvasFn | null): void {
  html2canvasImpl = fn;
}

async function loadHtml2Canvas(): Promise<Html2CanvasFn> {
  if (html2canvasImpl) return html2canvasImpl;
  // Dynamic import keeps html2canvas out of the SSR bundle.
  const mod = await import("html2canvas");
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const fn = (mod as any).default ?? (mod as unknown as Html2CanvasFn);
  html2canvasImpl = fn as Html2CanvasFn;
  return html2canvasImpl;
}

export interface CaptureResult {
  /** PNG data URL, or null if capture failed (server-side fallback will run). */
  data_url: string | null;
  /** True iff the client-side path produced a non-empty data URL. */
  ok: boolean;
}

/**
 * Capture the consent confirmation root as a PNG data URL.
 * The caller passes the form root element which MUST include the visible
 * six fields + the captured timestamp.
 */
export async function captureConsentScreenshot(
  root: HTMLElement | null,
): Promise<CaptureResult> {
  if (!root) return { data_url: null, ok: false };
  try {
    const h2c = await loadHtml2Canvas();
    const canvas = await h2c(root, { backgroundColor: "#ffffff", logging: false });
    const data_url = canvas.toDataURL("image/png");
    if (!data_url || !data_url.startsWith("data:image/png")) {
      return { data_url: null, ok: false };
    }
    return { data_url, ok: true };
  } catch (err) {
    safeLog("warn", "evt_012_client_fallback", {
      error_code: "EVT_012_CLIENT_FALLBACK",
      reason: err instanceof Error ? err.message : "unknown",
    });
    return { data_url: null, ok: false };
  }
}
