import { NextResponse, type NextRequest } from "next/server";

export const dynamic = "force-dynamic";

export function GET(req: NextRequest): NextResponse {
  return NextResponse.redirect(
    new URL("https://humangr.com/corelink/pricing", req.url),
    { status: 307 },
  );
}
