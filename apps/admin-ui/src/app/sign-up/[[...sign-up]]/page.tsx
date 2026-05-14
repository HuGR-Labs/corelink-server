import { SignUp } from "@clerk/nextjs";
import { useTranslations } from "next-intl";

export default function SignUpPage(): React.ReactElement {
  const t = useTranslations("auth");
  const publishableKey = process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"];
  if (!publishableKey) {
    return (
      <main className="mx-auto max-w-md p-8">
        <h1 className="text-xl font-semibold">{t("signUp")}</h1>
        <p className="mt-4 text-sm text-gray-600">
          Clerk publishable key not configured.
        </p>
      </main>
    );
  }
  return (
    <main className="mx-auto max-w-md p-8">
      <SignUp />
    </main>
  );
}
