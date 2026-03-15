"use client";

import { useRef, useState } from "react";
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
  ChevronUp,
} from "lucide-react";
import { SidebarLink } from "./sidebar-link";
import { UserPopover } from "./user-popover";
import { SettingsDialog } from "./settings-dialog";
import { ImportDialog } from "./import-dialog";
import { useSession } from "@/lib/auth-client";

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
  const [popoverOpen, setPopoverOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const avatarRef = useRef<HTMLButtonElement>(null);
  const { data: session } = useSession();
  const userName = session?.user?.name || "reader";
  const userInitial = userName.charAt(0).toUpperCase();

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

        {/* Bottom area — avatar button + popover (portal-based) */}
        <div className="mt-auto pt-4">
          <UserPopover
            open={popoverOpen}
            onClose={() => setPopoverOpen(false)}
            onOpenSettings={() => {
              setPopoverOpen(false);
              setSettingsOpen(true);
            }}
            triggerRef={avatarRef}
          />
          <button
            ref={avatarRef}
            onClick={() => setPopoverOpen((v) => !v)}
            className="flex items-center gap-3 w-full rounded-lg px-3 py-2 hover:bg-surface-hover transition-colors cursor-pointer"
          >
            <div className="w-8 h-8 rounded-full bg-surface-hover flex items-center justify-center text-text-secondary text-xs font-bold">
              {userInitial}
            </div>
            <span className="flex-1 text-left text-sm text-text-secondary truncate">
              {userName}
            </span>
            <ChevronUp
              size={14}
              className={`text-text-tertiary transition-transform ${popoverOpen ? "rotate-180" : ""}`}
            />
          </button>
        </div>
      </nav>

      <ImportDialog open={importOpen} onClose={() => setImportOpen(false)} />
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </>
  );
}
