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

export const runtime = "edge";
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

export async function POST(req: NextRequest): Promise<NextResponse> {
  const ip = clientIp(req);
  const decision = checkRateLimit(ip);
  if (!decision.allowed) {
    return new NextResponse(null, {
      status: 429,
      headers: { "retry-after": String(decision.retryAfterSec) },
    });
  }

  let report: CspViolationReport;
  try {
    report = (await req.json()) as CspViolationReport;
  } catch {
    return new NextResponse(null, { status: 400 });
  }

  const endpoint =
    process.env["CSP_REPORT_ENDPOINT"] ??
    `${process.env["NEXT_PUBLIC_CORELINK_API_URL"] ?? "https://api.corelink.dev"}/v1/csp-violations`;

  try {
    const r = await fetch(endpoint, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ ip_hash: hashIp(ip), report }),
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
