/**
 * Client-side JWT decoder for display ONLY.
 *
 * Security: NEVER verify signatures here. Trust must come from the backend.
 * Used by both consent receipts (WI-S16-003) and DSR receipts (WI-S16-004).
 */

import type { JwtReceiptClaims } from "@/lib/consent-types";
import type { DsrReceiptPayload } from "./dsr-types";

export interface DecodedJwt {
  header: Record<string, unknown>;
  payload: JwtReceiptClaims;
}

function b64UrlDecode(s: string): string {
  const pad = s.length % 4 === 0 ? "" : "=".repeat(4 - (s.length % 4));
  const b64 = s.replace(/-/g, "+").replace(/_/g, "/") + pad;
  if (typeof atob === "function") return atob(b64);
  return Buffer.from(b64, "base64").toString("binary");
}

function b64UrlToBytes(s: string): Uint8Array {
  const pad = "=".repeat((4 - (s.length % 4)) % 4);
  const b64 = (s + pad).replace(/-/g, "+").replace(/_/g, "/");
  if (typeof atob === "function") {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }
  return Uint8Array.from(Buffer.from(b64, "base64"));
}

// WI-S16-003 surface: throws on malformed input, returns full header+payload.
export function decodeJwtReceipt(jwt: string): DecodedJwt {
  const parts = jwt.split(".");
  if (parts.length < 2) throw new Error("invalid_jwt");
  const header = JSON.parse(b64UrlDecode(parts[0])) as Record<string, unknown>;
  const payload = JSON.parse(b64UrlDecode(parts[1])) as JwtReceiptClaims;
  return { header, payload };
}

// WI-S16-004 surface: returns null on malformed input, payload-only.
export function decodeJwtPayload(token: string): DsrReceiptPayload | null {
  try {
    const parts = token.split(".");
    if (parts.length !== 3) return null;
    const bytes = b64UrlToBytes(parts[1]);
    const json = new TextDecoder().decode(bytes);
    const obj = JSON.parse(json) as Partial<DsrReceiptPayload>;
    if (
      typeof obj.request_id !== "string" ||
      typeof obj.action !== "string" ||
      typeof obj.sla_deadline !== "string"
    ) {
      return null;
    }
    return obj as DsrReceiptPayload;
  } catch {
    return null;
  }
}
