"use client";

import Link from "next/link";
import { ChevronRight } from "lucide-react";
import { useLocale } from "./locale-provider";

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
  const { t } = useLocale();
  return (
    <section className="content-section">
      <div className="section-header">
        <div>
          <h2 className="section-title">{title}</h2>
          {subtitle && (
            <p className="section-description">{subtitle}</p>
          )}
        </div>
        {href && (
          <Link
            href={href}
            className="section-link"
          >
            {t("common.seeAll")}
            <ChevronRight className="ui-icon-sm" aria-hidden="true" />
          </Link>
        )}
      </div>
      <div className="carousel-track">
        {children}
      </div>
    </section>
  );
}
