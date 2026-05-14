"use client";

import * as React from "react";
import { LocaleContext, type Locale } from "./LocaleContext";

export interface LocaleProviderProps {
  initialLocale: Locale;
  children: React.ReactNode;
}

export function LocaleProvider({ initialLocale, children }: LocaleProviderProps) {
  const [locale, setLocale] = React.useState<Locale>(initialLocale);
  return (
    <LocaleContext.Provider value={{ locale, setLocale }}>{children}</LocaleContext.Provider>
  );
}
