import * as React from "react";
import type { Locale } from "@/i18n/messages";
import { PatStep } from "./PatStep";

export default async function Page(props: {
  params: Promise<{ locale: Locale }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;
  return <PatStep locale={locale} />;
}
