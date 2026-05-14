/**
 * Client-side JWT decoder for receipt display ONLY.
 *
 * Security:
 *   - This decoder DOES NOT verify the JWT signature. Verification happens
 *     on the backend public endpoint GET /v1/privacy/dsr/verify?token=<jwt>.
 *   - We must NEVER trust decoded values for any access-control decision.
 *   - We expose decoded fields purely so the user can see them in the
 *     receipt modal (request_id, action, jurisdiction, sla_deadline, jti, exp).
 *
 * Implementation note: we accept malformed tokens silently and return
 * `null`; the modal then shows the raw token (still copy/downloadable).
 */

import type { DsrReceiptPayload } from "./dsr-types";

function b64UrlToBytes(s: string): Uint8Array {
  const pad = "=".repeat((4 - (s.length % 4)) % 4);
  const b64 = (s + pad).replace(/-/g, "+").replace(/_/g, "/");
  if (typeof atob === "function") {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }
  // Node fallback (used only in tests).
  return Uint8Array.from(Buffer.from(b64, "base64"));
}

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
