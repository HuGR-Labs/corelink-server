import { NextResponse } from "next/server";

export const dynamic = "force-dynamic";

export function GET(): NextResponse {
  return NextResponse.redirect("https://humangr.com/corelink/llms.txt", {
    status: 307,
  });
}
