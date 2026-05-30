/**
 * E2E test-mode catch-all mock for `/api/v1/**` (wt-r3-7).
 *
 * Activates ONLY when NEXT_PUBLIC_E2E_TEST_MODE === "1" AND
 * NODE_ENV !== "production". In production this returns 503 so a misconfigured
 * deploy fails loudly rather than silently shadowing the real backend.
 *
 * Why a catch-all instead of MSW?
 *   - MSW (`msw/node`) intercepts fetch in the Next.js server runtime, but
 *     wiring it cleanly into the App Router's per-request worker pool requires
 *     a custom server (instrumentation API works but is brittle across Next
 *     15 patch versions). The catch-all route gives equivalent isolation with
 *     zero extra deps — it IS the mock service worker, in Next.js-native form.
 *   - Browser-side fetches go through page.route() in playwright (same
 *     fixtures shared via getFixtureResponse).
 *
 * Endpoints mirrored from `playwright/fixtures/api-mocks.ts` PLUS the
 * admin-client surface (`/v1/admin/tenants`, `/v1/admin/audit`,
 * `/v1/admin/ops`) that the Sprint-16 viewers need.
 */

import { NextResponse, type NextRequest } from "next/server";
import { getFixtureResponse } from "@/lib/e2e-mock-fixtures";

// Edge Runtime — no Node.js APIs used; in production this route returns 503.
export const dynamic = "force-dynamic";

function disabled(): NextResponse {
  return NextResponse.json(
    {
      type: "about:blank",
      title: "E2E mock disabled",
      status: 503,
      detail: "this endpoint is only available when NEXT_PUBLIC_E2E_TEST_MODE=1",
    },
    { status: 503 },
  );
}

function isTestMode(): boolean {
  return (
    process.env["NEXT_PUBLIC_E2E_TEST_MODE"] === "1" &&
    process.env["NODE_ENV"] !== "production"
  );
}

async function handle(
  req: NextRequest,
  context: { params: Promise<{ path: string[] }> },
): Promise<NextResponse> {
  if (!isTestMode()) return disabled();
  const { path } = await context.params;
  const fullPath = "/v1/" + path.join("/");
  let body: unknown = undefined;
  if (req.method !== "GET" && req.method !== "HEAD") {
    try {
      body = await req.json();
    } catch {
      body = null;
    }
  }
  const result = getFixtureResponse({
    method: req.method,
    path: fullPath,
    query: Object.fromEntries(new URL(req.url).searchParams.entries()),
    body,
    headers: Object.fromEntries(req.headers.entries()),
  });
  return NextResponse.json(result.body, {
    status: result.status,
    headers: result.contentType
      ? { "content-type": result.contentType }
      : undefined,
  });
}

export const GET = handle;
export const POST = handle;
export const PUT = handle;
export const PATCH = handle;
export const DELETE = handle;
