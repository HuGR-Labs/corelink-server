/**
 * GET /api/health — liveness probe (WI-S16-001 §6.6).
 *
 * Returns build metadata. Never reads tenant data.
 */

import { NextResponse } from "next/server";

export const dynamic = "force-dynamic";

export function GET(): NextResponse {
  const body = {
    status: "ok",
    version: process.env["NEXT_PUBLIC_APP_VERSION"] ?? "0.0.0",
    commit: process.env["NEXT_PUBLIC_APP_COMMIT"] ?? "unknown",
    timestamp: new Date().toISOString(),
  } as const;
  return NextResponse.json(body, {
    status: 200,
    headers: { "cache-control": "no-store" },
  });
}
