// Decode JWT header + payload WITHOUT verifying signature. Client-side
// decode is for display only; trust comes from the backend that issued it.

import type { JwtReceiptClaims } from "@/lib/consent-types";

export interface DecodedJwt {
  header: Record<string, unknown>;
  payload: JwtReceiptClaims;
}

function b64UrlDecode(s: string): string {
  const pad = s.length % 4 === 0 ? "" : "=".repeat(4 - (s.length % 4));
  const b64 = s.replace(/-/g, "+").replace(/_/g, "/") + pad;
  if (typeof atob === "function") return atob(b64);
  // Node fallback for tests.
  return Buffer.from(b64, "base64").toString("binary");
}

export function decodeJwtReceipt(jwt: string): DecodedJwt {
  const parts = jwt.split(".");
  if (parts.length < 2) throw new Error("invalid_jwt");
  const header = JSON.parse(b64UrlDecode(parts[0])) as Record<string, unknown>;
  const payload = JSON.parse(b64UrlDecode(parts[1])) as JwtReceiptClaims;
  return { header, payload };
}
