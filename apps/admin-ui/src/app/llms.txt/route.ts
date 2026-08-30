import { NextResponse } from "next/server";

import { PUBLIC_LLMS_TXT_URL } from "@/lib/public-url";

export const dynamic = "force-dynamic";

export function GET(): NextResponse {
  return NextResponse.redirect(PUBLIC_LLMS_TXT_URL, { status: 307 });
}
