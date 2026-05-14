"use client";

import React from "react";

export type Locale = "en" | "pt" | "es";

export const SUPPORTED_LOCALES: ReadonlyArray<Locale> = ["en", "pt", "es"];

export const LOCALE_COOKIE = "corelink_locale";

export interface LocaleContextValue {
  locale: Locale;
  setLocale: (locale: Locale) => void;
}

export const LocaleContext = React.createContext<LocaleContextValue>({
  locale: "en",
  setLocale: () => {},
});

export function useLocale(): LocaleContextValue {
  return React.useContext(LocaleContext);
}
