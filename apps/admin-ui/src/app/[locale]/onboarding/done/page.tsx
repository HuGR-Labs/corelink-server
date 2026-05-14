import * as React from "react";
import Link from "next/link";
import type { Locale } from "@/i18n/messages";
import { t } from "@/i18n/messages";

export default async function Page(props: {
  params: Promise<{ locale: Locale }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;
  return (
    <div>
      <h1>{t(locale, "onboarding.done.title")}</h1>
      <Link href={`/${locale}/dashboard`} data-testid="done-cta">
        {t(locale, "onboarding.done.cta")}
      </Link>
    </div>
  );
}
