"use client"

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react"
import {
  createTranslator,
  resolveLocalePreference,
  type LocalePreference,
  type MessageKey,
  type MessageVariables,
  type SupportedLocale,
} from "@/lib/localization"

export const LOCALE_COOKIE = "readio.locale"

type LocaleContextValue = {
  locale: SupportedLocale
  preference: LocalePreference
  setPreference: (preference: LocalePreference) => void
  t: (key: MessageKey, variables?: MessageVariables) => string
}

const LocaleContext = createContext<LocaleContextValue | null>(null)

export function LocaleProvider({
  children,
  initialLocale,
  initialPreference,
}: {
  children?: React.ReactNode
  initialLocale: SupportedLocale
  initialPreference: LocalePreference
}) {
  const [preference, setStoredPreference] =
    useState<LocalePreference>(initialPreference)
  const [locale, setLocale] = useState<SupportedLocale>(initialLocale)

  const setPreference = useCallback((next: LocalePreference) => {
    const browserLocales =
      typeof navigator === "undefined" ? [] : navigator.languages
    setStoredPreference(next)
    setLocale(resolveLocalePreference(next, browserLocales))
    document.cookie = `${LOCALE_COOKIE}=${encodeURIComponent(next)}; Path=/; Max-Age=31536000; SameSite=Lax`
  }, [])

  useEffect(() => {
    document.documentElement.lang = locale === "zh-Hans" ? "zh-Hans" : "en"
  }, [locale])

  useEffect(() => {
    if (preference !== "system") return
    const updateFromSystem = () => {
      setLocale(resolveLocalePreference("system", navigator.languages))
    }
    window.addEventListener("languagechange", updateFromSystem)
    return () => window.removeEventListener("languagechange", updateFromSystem)
  }, [preference])

  const value = useMemo<LocaleContextValue>(
    () => ({
      locale,
      preference,
      setPreference,
      t: createTranslator(locale),
    }),
    [locale, preference, setPreference]
  )

  return <LocaleContext.Provider value={value}>{children}</LocaleContext.Provider>
}

export function useLocale(): LocaleContextValue {
  const context = useContext(LocaleContext)
  if (!context) {
    throw new Error("useLocale must be used within LocaleProvider")
  }
  return context
}
