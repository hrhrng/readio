type BrandMarkProps = {
  className?: string
  size?: "sm" | "md" | "lg"
  tone?: "default" | "terminal"
}

/**
 * The prompt is not terminal costume pasted onto the product: it is the same
 * glyph that starts every command in the reader. The underscore gives the
 * lockup a stable cursor without introducing decorative animation.
 */
export function BrandMark({
  className = "",
  size = "md",
  tone = "default",
}: BrandMarkProps) {
  return (
    <span
      role="img"
      aria-label="Readio"
      className={["brand-mark", className].filter(Boolean).join(" ")}
      data-size={size}
      data-tone={tone}
    >
      <span className="brand-mark__prompt" aria-hidden="true">
        ❯_
      </span>
      <span className="brand-mark__word" aria-hidden="true">
        readio
      </span>
    </span>
  )
}
