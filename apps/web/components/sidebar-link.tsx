"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { LucideIcon } from "lucide-react";

interface SidebarLinkProps {
  href: string;
  icon: LucideIcon;
  label?: string;
  exact?: boolean;
}

export function SidebarLink({
  href,
  icon: Icon,
  label,
  exact = false,
}: SidebarLinkProps) {
  const pathname = usePathname();
  const isActive = exact ? pathname === href : pathname.startsWith(href);

  return (
    <Link
      href={href}
      className={`flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors ${
        isActive
          ? "bg-surface-active text-text-primary font-medium"
          : "text-text-secondary hover:bg-surface-hover hover:text-text-primary"
      }`}
    >
      <Icon size={18} strokeWidth={isActive ? 2 : 1.5} />
      {label && <span>{label}</span>}
    </Link>
  );
}
