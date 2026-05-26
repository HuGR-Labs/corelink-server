import * as React from "react";
import type { Locale } from "@/i18n/messages";
import { loadLocalizedMarkdown } from "@/content/load";
import { DpaStep } from "./DpaStep";

/**
 * Compute a SHA-256 hex digest using the Web Crypto API (Edge Runtime compat).
 * Web Crypto is available globally in Cloudflare Workers / Edge Runtime and in
 * Node.js ≥ 19 without import — no node:crypto required.
 */
async function sha256Hex(text: string): Promise<string> {
  const encoder = new TextEncoder();
  const hashBuf = await crypto.subtle.digest("SHA-256", encoder.encode(text));
  return Array.from(new Uint8Array(hashBuf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

export default async function Page(props: {
  params: Promise<{ locale: Locale }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;
  const text = loadLocalizedMarkdown("dpa", locale);
  const hash = await sha256Hex(text);
  return (
    <DpaStep
      locale={locale}
      dpaText={text}
      dpaVersion="1.0.0"
      noticeTextHash={hash}
    />
  );
}
