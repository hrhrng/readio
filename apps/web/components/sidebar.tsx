"use client"

import { useRef, useState } from "react"
import {
  Book,
  BookOpen,
  CheckCircle,
  Clock,
  FileText,
  Globe,
  Home,
  Search,
  Settings,
  Type,
  Upload,
} from "lucide-react"
import { SidebarLink } from "./sidebar-link"
import { UserPopover } from "./user-popover"
import { SettingsDialog } from "./settings-dialog"
import { ImportDialog } from "./import-dialog"
import { BrandMark } from "./brand-mark"
import { useSession } from "@/lib/auth-client"
import { useLocale } from "./locale-provider"

function SidebarSection({
  label,
  children,
}: {
  label: string
  children: React.ReactNode
}) {
  return (
    <section className="sidebar-section" aria-label={label}>
      <h2 className="sidebar-section-label">{label}</h2>
      <div className="sidebar-section-list">{children}</div>
    </section>
  )
}

function NavAction({
  icon: Icon,
  label,
  onClick,
}: {
  icon: typeof Upload
  label: string
  onClick: () => void
}) {
  return (
    <button type="button" onClick={onClick} className="nav-item">
      <span className="ui-icon-slot" aria-hidden="true">
        <Icon className="ui-icon" />
      </span>
      <span>{label}</span>
    </button>
  )
}

export function Sidebar() {
  const { t } = useLocale()
  const [importOpen, setImportOpen] = useState(false)
  const [popoverOpen, setPopoverOpen] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const avatarRef = useRef<HTMLButtonElement>(null)
  const { data: session } = useSession()
  const userName = session?.user?.name || t("nav.readerFallbackName")
  const userInitial = userName.charAt(0).toUpperCase()

  return (
    <>
      <aside className="app-sidebar">
        <div className="sidebar-brand">
          <BrandMark />
        </div>

        <nav className="sidebar-nav" aria-label={t("nav.primary")}>
          <SidebarLink href="/search" icon={Search} label={t("nav.search")} />
          <SidebarLink href="/" icon={Home} label={t("nav.home")} exact />
          <NavAction
            icon={Upload}
            label={t("nav.import")}
            onClick={() => setImportOpen(true)}
          />
        </nav>

        <SidebarSection label={t("nav.library")}>
          <SidebarLink href="/library/all" icon={BookOpen} label={t("nav.all")} />
          <SidebarLink
            href="/library/want-to-read"
            icon={Clock}
            label={t("nav.wantToRead")}
          />
          <SidebarLink
            href="/library/finished"
            icon={CheckCircle}
            label={t("nav.finished")}
          />
        </SidebarSection>

        <SidebarSection label={t("nav.fileType")}>
          <SidebarLink href="/library/type/web" icon={Globe} label="Web" />
          <SidebarLink href="/library/type/pdf" icon={FileText} label="PDF" />
          <SidebarLink href="/library/type/epub" icon={Book} label="EPUB" />
          <SidebarLink href="/library/type/txt" icon={Type} label="TXT" />
        </SidebarSection>

        <div className="mt-auto pt-4">
          <UserPopover
            open={popoverOpen}
            onClose={() => setPopoverOpen(false)}
            onOpenSettings={() => {
              setPopoverOpen(false)
              setSettingsOpen(true)
            }}
            triggerRef={avatarRef}
          />
          <button
            ref={avatarRef}
            type="button"
            onClick={() => setPopoverOpen((open) => !open)}
            className="nav-item"
            aria-expanded={popoverOpen}
          >
            <span className="user-avatar" aria-hidden="true">
              {userInitial}
            </span>
            <span className="min-w-0 flex-1 truncate text-left">
              {userName}
            </span>
          </button>
        </div>
      </aside>

      <nav className="app-mobile-nav" aria-label={t("nav.primary")}>
        <SidebarLink href="/" icon={Home} label={t("nav.home")} exact />
        <SidebarLink href="/search" icon={Search} label={t("nav.search")} />
        <SidebarLink href="/library/all" icon={BookOpen} label={t("nav.library")} />
        <NavAction
          icon={Upload}
          label={t("nav.import")}
          onClick={() => setImportOpen(true)}
        />
        <NavAction
          icon={Settings}
          label={t("nav.settings")}
          onClick={() => setSettingsOpen(true)}
        />
      </nav>

      <ImportDialog open={importOpen} onClose={() => setImportOpen(false)} />
      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />
    </>
  )
}
