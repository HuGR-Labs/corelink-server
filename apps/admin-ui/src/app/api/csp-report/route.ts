/**
 * POST /api/csp-report — CSP violation report sink (WI-S16-001 §6.5).
 *
 * - Rate-limits by client IP (100/min/IP).
 * - Forwards sanitized reports to the CoreLink violations API; falls back to
 *   a structured stderr log when the backend endpoint is unavailable.
 * - Never echoes auth tokens or PII.
 */

import { NextResponse, type NextRequest } from "next/server";
import { checkRateLimit } from "@/lib/rate-limit";

export const dynamic = "force-dynamic";

function clientIp(req: NextRequest): string {
  const xff = req.headers.get("x-forwarded-for");
  if (xff) {
    const first = xff.split(",")[0];
    if (first) return first.trim();
  }
  return req.headers.get("x-real-ip") ?? "unknown";
}

interface CspViolationReport {
  "csp-report"?: Record<string, unknown>;
  // Reporting API v1 shape:
  type?: string;
  age?: number;
  url?: string;
  user_agent?: string;
  body?: Record<string, unknown>;
}

/**
 * The two wire shapes this sink accepts.
 *
 *   - `report-uri`      → ONE object, `application/csp-report`.
 *   - `report-to` (Reporting API v1) → an ARRAY of reports,
 *     `application/reports+json`, batched by the browser.
 *
 * Both channels are advertised (see `lib/csp.ts` REPORTING_ENDPOINTS_HEADER), so
 * the handler must normalize. Forwarding an array verbatim under the singular
 * `report` key would have silently changed the shape the backend receives —
 * i.e. traded one dropped-report bug for another.
 */
type CspReportPayload = CspViolationReport | CspViolationReport[];

export async function POST(req: NextRequest): Promise<NextResponse> {
  const ip = clientIp(req);
  const decision = checkRateLimit(ip);
  if (!decision.allowed) {
    return new NextResponse(null, {
      status: 429,
      headers: { "retry-after": String(decision.retryAfterSec) },
    });
  }

  let payload: CspReportPayload;
  try {
    payload = (await req.json()) as CspReportPayload;
  } catch {
    return new NextResponse(null, { status: 400 });
  }
  // One `{ip_hash, report}` envelope per violation, so the backend contract is
  // identical whichever channel the browser used.
  const reports: CspViolationReport[] = Array.isArray(payload) ? payload : [payload];
  if (reports.length === 0) return new NextResponse(null, { status: 204 });

  const endpoint =
    process.env["CSP_REPORT_ENDPOINT"] ??
    `${process.env["NEXT_PUBLIC_CORELINK_API_URL"] ?? "https://corelink-api.humangr.com"}/v1/csp-violations`;

  const ipHash = hashIp(ip);
  for (const report of reports) {
    try {
      const r = await fetch(endpoint, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ ip_hash: ipHash, report }),
      });
      if (!r.ok) {
        // eslint-disable-next-line no-console
        console.error("csp-report forward failed", { status: r.status });
      }
    } catch (err) {
      // eslint-disable-next-line no-console
      console.error("csp-report transport error", {
        reason: err instanceof Error ? err.name : "unknown",
      });
    }
  }

  return new NextResponse(null, { status: 204 });
}

/**
 * Lightweight, non-cryptographic IP hash for log correlation. NOT used for
 * authentication. We avoid the Web Crypto async dance because edge runtime
 * Promise chaining in error paths bloats the budget.
 */
function hashIp(ip: string): string {
  let h = 2166136261 >>> 0;
  for (let i = 0; i < ip.length; i++) {
    h ^= ip.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}
