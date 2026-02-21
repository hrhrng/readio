"use client";

import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { Settings, HelpCircle, LogOut } from "lucide-react";

interface UserPopoverProps {
  open: boolean;
  onClose: () => void;
  onOpenSettings: () => void;
  /** Ref to the trigger button — clicks on it won't fire the outside-click handler */
  triggerRef: React.RefObject<HTMLButtonElement | null>;
}

/**
 * Popover panel rendered via portal so it escapes the sidebar's overflow clipping.
 * Positioned fixed, horizontally centered within the sidebar, above the avatar trigger.
 * Closes on outside click (excluding trigger) or ESC.
 */
export function UserPopover({ open, onClose, onOpenSettings, triggerRef }: UserPopoverProps) {
  const panelRef = useRef<HTMLDivElement>(null);

  // Close on ESC
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onClose]);

  // Close on outside click — ignore clicks inside panel AND on the trigger button
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      const target = e.target as Node;
      if (panelRef.current?.contains(target)) return;
      if (triggerRef.current?.contains(target)) return;
      onClose();
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open, onClose, triggerRef]);

  if (!open) return null;

  // Horizontally centered within the sidebar (w-60 = 240px), above trigger
  const SIDEBAR_WIDTH = 240;
  const POPOVER_WIDTH = SIDEBAR_WIDTH - 24;
  const triggerRect = triggerRef.current?.getBoundingClientRect();
  const style: React.CSSProperties = {
    position: "fixed",
    bottom: triggerRect ? window.innerHeight - triggerRect.top + 6 : 60,
    left: (SIDEBAR_WIDTH - POPOVER_WIDTH) / 2,
    width: POPOVER_WIDTH,
  };

  return createPortal(
    <div
      ref={panelRef}
      style={style}
      className="z-50 bg-surface-card border border-border rounded-xl shadow-lg py-1.5"
    >
      {/* User info */}
      <div className="px-3.5 py-2.5">
        <p className="text-sm font-medium text-text-primary truncate">
          reader@readio.app
        </p>
      </div>

      <Divider />

      {/* Settings — opens the settings dialog */}
      <PopoverItem
        icon={Settings}
        label="Settings"
        shortcut="⌘,"
        onClick={onOpenSettings}
      />

      {/* Help */}
      <PopoverItem
        icon={HelpCircle}
        label="Get help"
        onClick={() => {
          onClose();
          window.open("https://github.com/nicepkg/readio", "_blank");
        }}
      />

      <Divider />

      {/* Log out (placeholder) */}
      <PopoverItem icon={LogOut} label="Log out" onClick={onClose} />
    </div>,
    document.body
  );
}

/* ─── Internal helpers ─── */

function Divider() {
  return <div className="my-1 border-t border-border" />;
}

function PopoverItem({
  icon: Icon,
  label,
  shortcut,
  onClick,
}: {
  icon: React.ComponentType<{ size?: number; strokeWidth?: number }>;
  label: string;
  shortcut?: string;
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="flex items-center gap-3 w-full px-3.5 py-2 text-sm text-text-secondary hover:bg-surface-hover hover:text-text-primary transition-colors cursor-pointer"
    >
      <Icon size={16} strokeWidth={1.5} />
      <span className="flex-1 text-left">{label}</span>
      {shortcut && (
        <span className="text-xs text-text-tertiary">{shortcut}</span>
      )}
    </button>
  );
}
