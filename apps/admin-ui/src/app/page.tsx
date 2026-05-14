import Link from "next/link";
import { useTranslations } from "next-intl";

export default function HomePage(): React.ReactElement {
  return <LandingContent />;
}

function LandingContent(): React.ReactElement {
  const t = useTranslations("landing");
  const brand = useTranslations("brand");
  return (
    <main className="mx-auto max-w-3xl px-6 py-16">
      <h1 className="text-3xl font-semibold tracking-tight">{t("heading")}</h1>
      <p className="mt-2 text-sm text-gray-600">{brand("tagline")}</p>
      <p className="mt-6 text-base">{t("subheading")}</p>
      <Link
        href="/sign-in"
        className="mt-8 inline-block rounded-md border px-4 py-2 text-sm font-medium"
      >
        {t("cta")}
      </Link>
    </main>
  );
}
