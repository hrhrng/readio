"use client"

import { Check, Grid2X2, List, MoreHorizontal } from "lucide-react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "./dropdown-menu"
import { useLocale } from "./locale-provider"

export type LibraryLayout = "shelf" | "list"

interface LibraryViewMenuProps {
  layout: LibraryLayout
  sortBy: string
  sortOptions: { value: string; label: string }[]
  onLayoutChange: (layout: LibraryLayout) => void
  onSortChange: (value: string) => void
}

export function LibraryViewMenu({
  layout,
  sortBy,
  sortOptions,
  onLayoutChange,
  onSortChange,
}: LibraryViewMenuProps) {
  const { t } = useLocale()
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="ui-icon-button library-view-trigger"
          aria-label={t("library.viewOptions")}
        >
          <MoreHorizontal className="ui-icon-lg" aria-hidden="true" />
        </button>
      </DropdownMenuTrigger>

      <DropdownMenuContent
        className="library-view-popover"
        align="end"
        sideOffset={8}
      >
        <DropdownMenuRadioGroup
          value={layout}
          onValueChange={(value) => onLayoutChange(value as LibraryLayout)}
        >
          <DropdownMenuRadioItem value="shelf">
            <Grid2X2 className="ui-icon-sm" aria-hidden="true" />
            <span className="flex-1">{t("library.grid")}</span>
            {layout === "shelf" && (
              <Check className="ui-icon-sm text-accent" aria-hidden="true" />
            )}
          </DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="list">
            <List className="ui-icon-sm" aria-hidden="true" />
            <span className="flex-1">{t("library.list")}</span>
            {layout === "list" && (
              <Check className="ui-icon-sm text-accent" aria-hidden="true" />
            )}
          </DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>

        <DropdownMenuSeparator />
        <DropdownMenuLabel>{t("library.sortBy")}</DropdownMenuLabel>
        <DropdownMenuGroup>
          <DropdownMenuRadioGroup
            value={sortBy}
            onValueChange={onSortChange}
          >
            {sortOptions.map((option) => (
              <DropdownMenuRadioItem key={option.value} value={option.value}>
                <span className="flex-1">{option.label}</span>
                {sortBy === option.value && (
                  <Check
                    className="ui-icon-sm text-accent"
                    aria-hidden="true"
                  />
                )}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
