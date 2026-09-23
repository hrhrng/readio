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
      className="nav-item"
      data-active={isActive}
      aria-current={isActive ? "page" : undefined}
    >
      <span className="ui-icon-slot" aria-hidden="true">
        <Icon className="ui-icon" />
      </span>
      {label && <span>{label}</span>}
    </Link>
  );
}
