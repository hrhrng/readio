import { resolvePageLayout, type PageKind } from "@/lib/ui-contracts"

interface PageLayoutProps {
  kind: Exclude<PageKind, "reader">
  title: string
  description?: string
  actions?: React.ReactNode
  children: React.ReactNode
}

export function PageLayout({
  kind,
  title,
  description,
  actions,
  children,
}: PageLayoutProps) {
  const layout = resolvePageLayout(kind)

  return (
    <div
      className="page-frame"
      data-frame={layout.frame}
      data-density={layout.density}
      data-alignment={layout.alignment}
    >
      <header className="page-header">
        <div className="page-title-group">
          <h1 className="page-title">{title}</h1>
          {description && <p className="page-description">{description}</p>}
        </div>
        {actions && <div className="page-actions">{actions}</div>}
      </header>
      <main className="page-content">{children}</main>
    </div>
  )
}
