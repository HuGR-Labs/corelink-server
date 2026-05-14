import React from "react";
import { render, type RenderOptions, type RenderResult } from "@testing-library/react";
import { LocaleContext, type Locale } from "@/i18n/LocaleContext";

interface AllProvidersProps {
  children: React.ReactNode;
  locale?: Locale;
}

function AllProviders({ children, locale = "en" }: AllProvidersProps) {
  return <LocaleContext.Provider value={{ locale, setLocale: () => {} }}>{children}</LocaleContext.Provider>;
}

export function renderWithProviders(
  ui: React.ReactElement,
  options?: RenderOptions & { locale?: Locale }
): RenderResult {
  const { locale, ...rest } = options ?? {};
  return render(ui, {
    wrapper: ({ children }) => <AllProviders locale={locale}>{children}</AllProviders>,
    ...rest,
  });
}

export * from "@testing-library/react";
