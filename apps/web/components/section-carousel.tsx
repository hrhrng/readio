import Link from "next/link";
import { ChevronRight } from "lucide-react";

interface SectionCarouselProps {
  title: string;
  subtitle?: string;
  href?: string;
  children: React.ReactNode;
}

export function SectionCarousel({
  title,
  subtitle,
  href,
  children,
}: SectionCarouselProps) {
  return (
    <section className="mb-10">
      <div className="flex items-baseline justify-between mb-4">
        <div>
          <h2 className="text-2xl font-bold text-text-primary font-serif">
            {title}
          </h2>
          {subtitle && (
            <p className="text-sm text-text-secondary mt-0.5">{subtitle}</p>
          )}
        </div>
        {href && (
          <Link
            href={href}
            className="flex items-center gap-1 text-sm text-accent hover:underline"
          >
            See All
            <ChevronRight size={16} />
          </Link>
        )}
      </div>
      <div className="flex gap-4 overflow-x-auto scrollbar-hide pb-2">
        {children}
      </div>
    </section>
  );
}
