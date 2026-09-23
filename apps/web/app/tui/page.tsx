import type { Metadata } from "next";
import Link from "next/link";
import { BrandMark } from "@/components/brand-mark";

const siteUrl = process.env.NEXT_PUBLIC_SITE_URL ?? "https://read-io.com";

export const metadata: Metadata = {
  title: "TUI Ebook Reader for the Terminal",
  description:
    "Read EPUB, PDF, Markdown, and plain text without leaving the terminal. Readio is a keyboard-first TUI with local text-to-speech, real search, bookmarks, and honest reading progress.",
  alternates: {
    canonical: "/tui",
  },
  openGraph: {
    type: "website",
    url: "/tui",
    title: "Readio TUI — An ebook reader for your terminal",
    description:
      "A focused terminal ebook reader with keyboard-first controls and optional local read-aloud.",
  },
  twitter: {
    card: "summary",
    title: "Readio TUI — An ebook reader for your terminal",
    description:
      "Read EPUB, PDF, Markdown, and plain text in a focused terminal UI.",
  },
};

const jsonLd = {
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  name: "Readio TUI",
  applicationCategory: "ProductivityApplication",
  operatingSystem: "macOS, Linux, Windows",
  description: metadata.description,
  url: `${siteUrl}/tui`,
  downloadUrl: "https://github.com/hrhrng/readio/releases",
  softwareHelp: "https://github.com/hrhrng/readio#readme",
  isAccessibleForFree: true,
  license: "https://opensource.org/license/mit/",
  offers: {
    "@type": "Offer",
    price: "0",
    priceCurrency: "USD",
  },
  featureList: [
    "EPUB, PDF, Markdown, and plain-text reading",
    "Keyboard-first terminal interface",
    "Manual, automatic, and read-aloud modes",
    "Local text-to-speech engine support",
    "Full-text search and bookmarks",
  ],
};

const formats = ["EPUB", "PDF", "Markdown", "Plain text"];

const featureList = [
  [
    "Three reading modes",
    "Manual, Auto, and Read-aloud stay explicit, interruptible, and in sync.",
  ],
  [
    "Local voices",
    "Use a local TTS model or your own command. No bundled model or cloud account is required.",
  ],
  [
    "Search that lands",
    "Full-text search counts every match and jumps to the exact sentence in your book.",
  ],
  [
    "Positions that survive",
    "Bookmarks and progress are stored as content offsets, so reorganising a book does not lose your place.",
  ],
] as const;

export default function TuiPage() {
  return (
    <main className="min-h-screen overflow-hidden bg-[#101311] text-[#e8eee8]">
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd) }}
      />

      <div className="mx-auto max-w-6xl px-6 py-6 sm:px-10 lg:px-12">
        <header className="flex items-center justify-between">
          <Link
            href="/tui"
            className="text-[#dcebdc]"
          >
            <BrandMark size="lg" tone="terminal" />
          </Link>
          <nav aria-label="Primary" className="flex items-center gap-5 text-sm">
            <a
              href="https://github.com/hrhrng/readio"
              target="_blank"
              rel="noreferrer"
              className="text-[#aebcaf] transition-colors hover:text-white"
            >
              GitHub
            </a>
            <Link
              href="/"
              className="rounded-full border border-[#718873] px-4 py-2 text-[#dcebdc] transition-colors hover:bg-[#1d2a20]"
            >
              Web app
            </Link>
          </nav>
        </header>

        <section className="grid gap-14 pb-24 pt-24 lg:grid-cols-[1.05fr_0.95fr] lg:items-center lg:gap-20 lg:pb-32 lg:pt-32">
          <div>
            <p className="mb-6 font-mono text-sm uppercase tracking-[0.22em] text-[#8fc293]">
              Terminal ebook reader · open source
            </p>
            <h1 className="max-w-3xl font-serif text-5xl font-bold leading-[1.04] tracking-tight text-[#f1f6f1] sm:text-6xl lg:text-7xl">
              Read books where your hands already are.
            </h1>
            <p className="mt-7 max-w-2xl text-lg leading-8 text-[#b7c4b8] sm:text-xl">
              Readio is a keyboard-first TUI for EPUB, PDF, Markdown, and plain
              text. It brings focused reading, real progress, and optional local
              read-aloud into the terminal — without another media player to
              manage.
            </p>
            <div className="mt-9 flex flex-wrap gap-3">
              <a
                href="https://github.com/hrhrng/readio#install"
                target="_blank"
                rel="noreferrer"
                className="rounded-full bg-[#9bd39f] px-5 py-3 font-medium text-[#102014] transition-colors hover:bg-[#b6e5b9]"
              >
                Install readio
              </a>
              <a
                href="https://github.com/hrhrng/readio"
                target="_blank"
                rel="noreferrer"
                className="rounded-full border border-[#718873] px-5 py-3 font-medium text-[#dcebdc] transition-colors hover:bg-[#1d2a20]"
              >
                Explore the source
              </a>
            </div>
            <p className="mt-5 font-mono text-xs text-[#819083]">
              macOS · Linux · Windows builds from source
            </p>
          </div>

          <div className="rounded-2xl border border-[#35463a] bg-[#0a0d0b] p-4 shadow-2xl shadow-black/30 sm:p-5">
            <div className="mb-4 flex items-center gap-2 border-b border-[#29352c] pb-3 font-mono text-xs text-[#819083]">
              <span className="h-2.5 w-2.5 rounded-full bg-[#d87979]" />
              <span className="h-2.5 w-2.5 rounded-full bg-[#d9b56d]" />
              <span className="h-2.5 w-2.5 rounded-full bg-[#86c78a]" />
              <span className="ml-2">❯_ readio · The Shape of Attention</span>
            </div>
            <pre className="overflow-x-auto font-mono text-[11px] leading-6 text-[#c9d7ca] sm:text-xs">
              <code>{` ❯_ readio   The Shape of Attention     ch 1/4

 ❙ Thought for 1.3s

 ● Read attention.md  L1-9  ·  0.3s
   # One  Attention in eighty columns
   The first time I noticed that attention
   had a shape, I was watching a cursor…

╭────────────────────────────────────────────╮
│❯ enter to keep reading; / opens commands   │
╰────────────────────────────────────────────╯
  ⠼ Read-aloud  ·  0:06  ·  64 tok`}</code>
            </pre>
          </div>
        </section>

        <section
          aria-labelledby="formats-heading"
          className="border-t border-[#29352c] py-20"
        >
          <p className="font-mono text-sm uppercase tracking-[0.18em] text-[#8fc293]">
            Bring your books
          </p>
          <h2
            id="formats-heading"
            className="mt-3 max-w-2xl font-serif text-3xl font-bold tracking-tight text-[#f1f6f1] sm:text-4xl"
          >
            A serious reader for ordinary files.
          </h2>
          <div className="mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {formats.map((format) => (
              <div
                key={format}
                className="rounded-xl border border-[#35463a] bg-[#161c17] p-5"
              >
                <p className="font-mono text-sm text-[#9bd39f]">{format}</p>
                <p className="mt-3 text-sm leading-6 text-[#aebcaf]">
                  Parsed into chapters, paragraphs, positions, and searchable text.
                </p>
              </div>
            ))}
          </div>
        </section>

        <section
          aria-labelledby="features-heading"
          className="grid gap-12 border-t border-[#29352c] py-20 lg:grid-cols-[0.8fr_1.2fr] lg:py-24"
        >
          <div>
            <p className="font-mono text-sm uppercase tracking-[0.18em] text-[#8fc293]">
              Made for deep reading
            </p>
            <h2
              id="features-heading"
              className="mt-3 font-serif text-3xl font-bold tracking-tight text-[#f1f6f1] sm:text-4xl"
            >
              The terminal is the interface.
            </h2>
          </div>
          <div className="grid gap-x-8 gap-y-10 sm:grid-cols-2">
            {featureList.map(([title, description]) => (
              <article key={title}>
                <h3 className="font-medium text-[#e8eee8]">{title}</h3>
                <p className="mt-2 text-sm leading-6 text-[#aebcaf]">
                  {description}
                </p>
              </article>
            ))}
          </div>
        </section>

        <section
          aria-labelledby="install-heading"
          className="border-t border-[#29352c] py-20 lg:py-24"
        >
          <div className="rounded-2xl border border-[#49664d] bg-[#18241a] p-8 sm:p-10">
            <p className="font-mono text-sm uppercase tracking-[0.18em] text-[#9bd39f]">
              Start in one command
            </p>
            <h2
              id="install-heading"
              className="mt-3 font-serif text-3xl font-bold tracking-tight text-[#f1f6f1] sm:text-4xl"
            >
              Your next chapter is already on disk.
            </h2>
            <pre className="mt-7 overflow-x-auto rounded-xl bg-[#0a0d0b] p-4 font-mono text-sm leading-7 text-[#d9e7da]">
              <code>
                curl -fsSL https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.sh | sh
              </code>
            </pre>
            <p className="mt-5 max-w-2xl text-sm leading-6 text-[#b7c4b8]">
              The installer downloads a verified release for macOS or Linux.
              Windows users can build the same pure-Rust binary from source.
            </p>
          </div>
        </section>

        <footer className="flex flex-col gap-3 border-t border-[#29352c] py-8 text-sm text-[#819083] sm:flex-row sm:items-center sm:justify-between">
          <p>Readio is open source and MIT licensed.</p>
          <div className="flex gap-5">
            <a
              href="https://github.com/hrhrng/readio"
              target="_blank"
              rel="noreferrer"
              className="hover:text-[#dcebdc]"
            >
              GitHub
            </a>
            <Link href="/" className="hover:text-[#dcebdc]">
              Web app
            </Link>
          </div>
        </footer>
      </div>
    </main>
  );
}
