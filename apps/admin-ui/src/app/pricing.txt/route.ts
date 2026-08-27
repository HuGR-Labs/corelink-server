import { NextResponse } from "next/server";

export const dynamic = "force-dynamic";

export function GET(): NextResponse {
  return NextResponse.redirect("https://humangr.com/corelink/pricing.txt", {
    status: 307,
  });
}
