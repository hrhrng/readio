import type { Metadata } from "next";
import { cookies, headers } from "next/headers";
import { Playfair_Display, Inter } from "next/font/google";
import { LocaleProvider, LOCALE_COOKIE } from "@/components/locale-provider";
import { ThemeProvider } from "@/components/theme-provider";
import { ToastProvider } from "@/components/toast-provider";
import {
  normalizeLocalePreference,
  resolveLocalePreference,
} from "@/lib/localization";
import "./globals.css";

const playfair = Playfair_Display({
  subsets: ["latin"],
  variable: "--font-serif",
  display: "swap",
});

const inter = Inter({
  subsets: ["latin"],
  variable: "--font-sans",
  display: "swap",
});

export const metadata: Metadata = {
  metadataBase: new URL(
    process.env.NEXT_PUBLIC_SITE_URL ?? "https://read-io.com"
  ),
  title: {
    default: "Readio — Read books in your terminal",
    template: "%s | Readio",
  },
  description:
    "Read EPUB, PDF, Markdown, and plain text in a keyboard-first terminal reader with optional local text-to-speech.",
  applicationName: "Readio",
  keywords: [
    "terminal ebook reader",
    "TUI ebook reader",
    "read EPUB in terminal",
    "local text to speech",
    "audiobook player",
    "keyboard-first reader",
  ],
  authors: [
    {
      name: "Readio contributors",
      url: "https://github.com/hrhrng/readio",
    },
  ],
  creator: "Readio contributors",
  alternates: {
    canonical: "/",
  },
  openGraph: {
    type: "website",
    siteName: "Readio",
    title: "Readio — Read books in your terminal",
    description:
      "A keyboard-first terminal reader for EPUB, PDF, Markdown, and plain text, with optional local read-aloud.",
    url: "/",
  },
  twitter: {
    card: "summary",
    title: "Readio — Read books in your terminal",
    description:
      "A keyboard-first terminal reader for EPUB, PDF, Markdown, and plain text, with optional local read-aloud.",
  },
};

export default async function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const [cookieStore, headerStore] = await Promise.all([cookies(), headers()]);
  const localePreference = normalizeLocalePreference(
    cookieStore.get(LOCALE_COOKIE)?.value
  );
  const requestedLocales = (headerStore.get("accept-language") ?? "")
    .split(",")
    .map((part) => part.split(";", 1)[0]);
  const initialLocale = resolveLocalePreference(
    localePreference,
    requestedLocales
  );

  return (
    <html
      lang={initialLocale === "zh-Hans" ? "zh-Hans" : "en"}
      className={`${playfair.variable} ${inter.variable}`}
      suppressHydrationWarning
    >
      <body className="font-sans antialiased bg-surface text-text-primary">
        <LocaleProvider
          initialLocale={initialLocale}
          initialPreference={localePreference}
        >
          <ThemeProvider>
            <ToastProvider>{children}</ToastProvider>
          </ThemeProvider>
        </LocaleProvider>
      </body>
    </html>
  );
}
