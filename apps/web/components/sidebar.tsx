"use client";

import { useState } from "react";
import {
  Search,
  Home,
  BookOpen,
  Clock,
  CheckCircle,
  Globe,
  FileText,
  Book,
  Type,
  Plus,
  Upload,
} from "lucide-react";
import { SidebarLink } from "./sidebar-link";
import { ThemeToggle } from "./theme-toggle";
import { ImportDialog } from "./import-dialog";

function SidebarSection({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mt-4">
      <h3 className="px-3 mb-1 text-xs font-semibold uppercase tracking-wider text-text-tertiary">
        {label}
      </h3>
      <div className="flex flex-col gap-0.5">{children}</div>
    </div>
  );
}

export function Sidebar() {
  const [importOpen, setImportOpen] = useState(false);

  return (
    <>
      <nav className="w-60 h-screen fixed left-0 top-0 bg-surface-sidebar flex flex-col border-r border-border px-3 py-4 overflow-y-auto z-10">
        {/* Logo */}
        <div className="px-3 mb-4">
          <span className="text-2xl font-bold font-serif tracking-tight text-text-primary">
            Readio
          </span>
        </div>

        {/* Top Navigation */}
        <div className="flex flex-col gap-0.5">
          <SidebarLink href="/search" icon={Search} label="Search" />
          <SidebarLink href="/" icon={Home} label="Home" exact />
          <button
            onClick={() => setImportOpen(true)}
            className="flex items-center gap-3 rounded-lg px-3 py-2 text-sm text-text-secondary hover:bg-surface-hover hover:text-text-primary transition-colors cursor-pointer"
          >
            <Upload size={18} strokeWidth={1.5} />
            <span>Import</span>
          </button>
        </div>

        {/* Library Section */}
        <SidebarSection label="Library">
          <SidebarLink href="/library/all" icon={BookOpen} label="All" />
          <SidebarLink
            href="/library/want-to-read"
            icon={Clock}
            label="Want to Read"
          />
          <SidebarLink
            href="/library/finished"
            icon={CheckCircle}
            label="Finished"
          />
        </SidebarSection>

        {/* By Type */}
        <SidebarSection label="By Type">
          <SidebarLink href="/library/type/web" icon={Globe} label="Web" />
          <SidebarLink href="/library/type/pdf" icon={FileText} label="PDF" />
          <SidebarLink href="/library/type/epub" icon={Book} label="EPUB" />
          <SidebarLink href="/library/type/txt" icon={Type} label="TXT" />
        </SidebarSection>

        {/* Collections */}
        <SidebarSection label="My Collections">
          <button className="flex items-center gap-3 rounded-lg px-3 py-2 text-sm text-text-secondary hover:bg-surface-hover hover:text-text-primary transition-colors cursor-pointer">
            <Plus size={18} strokeWidth={1.5} />
            <span>New Collection</span>
          </button>
        </SidebarSection>

        {/* Bottom area */}
        <div className="mt-auto pt-4 flex flex-col gap-3">
          <ThemeToggle />
          <div className="flex items-center gap-3 px-3 py-2">
            <div className="w-8 h-8 rounded-full bg-surface-hover flex items-center justify-center text-text-secondary text-xs font-medium">
              U
            </div>
            <span className="text-sm text-text-secondary">User</span>
          </div>
        </div>
      </nav>

      <ImportDialog open={importOpen} onClose={() => setImportOpen(false)} />
    </>
  );
}
