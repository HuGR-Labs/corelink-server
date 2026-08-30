import { NextResponse } from "next/server";

import { PUBLIC_PRICING_TXT_URL } from "@/lib/public-url";

export const dynamic = "force-dynamic";

export function GET(): NextResponse {
  return NextResponse.redirect(PUBLIC_PRICING_TXT_URL, { status: 307 });
}
