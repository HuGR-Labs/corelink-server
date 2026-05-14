import * as React from "react";
import { promises as fs } from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import type { Locale } from "@/i18n/messages";
import { DpaStep } from "./DpaStep";

async function loadDpa(locale: Locale): Promise<string> {
  const candidate = path.resolve(
    process.cwd(),
    "src/content",
    `dpa.${locale}.md`,
  );
  try {
    return await fs.readFile(candidate, "utf8");
  } catch {
    const fallback = path.resolve(process.cwd(), "src/content/dpa.en.md");
    return fs.readFile(fallback, "utf8");
  }
}

export default async function Page(props: {
  params: Promise<{ locale: Locale }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;
  const text = await loadDpa(locale);
  const hash = createHash("sha256").update(text, "utf8").digest("hex");
  return (
    <DpaStep
      locale={locale}
      dpaText={text}
      dpaVersion="1.0.0"
      noticeTextHash={hash}
    />
  );
}
