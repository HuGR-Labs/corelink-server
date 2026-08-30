import { NextResponse, type NextRequest } from "next/server";

import { PUBLIC_PRICING_URL } from "@/lib/public-url";

export const dynamic = "force-dynamic";

export function GET(req: NextRequest): NextResponse {
  return NextResponse.redirect(new URL(PUBLIC_PRICING_URL, req.url), {
    status: 307,
  });
}
