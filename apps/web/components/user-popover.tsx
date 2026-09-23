"use client";

import { useEffect, useRef } from "react";
import { useRouter } from "next/navigation";
import { createPortal } from "react-dom";
import { Settings, HelpCircle, LogIn, LogOut } from "lucide-react";
import { useSession, signOut } from "@/lib/auth-client";

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
  const router = useRouter();
  const { data: session } = useSession();

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

  const triggerRect = triggerRef.current?.getBoundingClientRect();
  const style: React.CSSProperties = {
    position: "fixed",
    bottom: triggerRect
      ? `calc(100vh - ${triggerRect.top}px + var(--space-2))`
      : "var(--space-16)",
    left: triggerRect?.left ?? "var(--space-3)",
    width: triggerRect?.width ?? "calc(var(--shell-sidebar-width) - var(--space-6))",
  };

  const userEmail = session?.user?.email;

  return createPortal(
    <div
      ref={panelRef}
      style={style}
      className="surface-popover"
    >
      <div className="px-3 py-2">
        <p className="text-sm font-medium text-text-primary truncate">
          {userEmail ?? "Local library"}
        </p>
        {!userEmail && (
          <p className="mt-0.5 text-xs text-text-secondary">
            Stored on this device
          </p>
        )}
      </div>

      <Divider />

      {/* Settings — opens the settings dialog */}
      <PopoverItem
        icon={Settings}
        label="Settings"
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

      {session ? (
        <PopoverItem
          icon={LogOut}
          label="Log out"
          onClick={async () => {
            onClose();
            await signOut();
            router.push("/");
          }}
        />
      ) : (
        <PopoverItem
          icon={LogIn}
          label="Sign in to sync"
          onClick={() => {
            onClose();
            router.push("/login");
          }}
        />
      )}
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
  onClick,
}: {
  icon: React.ComponentType<{
    size?: number;
    strokeWidth?: number;
    className?: string;
  }>;
  label: string;
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="popover-item"
    >
      <Icon className="ui-icon-sm" />
      <span className="flex-1 text-left">{label}</span>
    </button>
  );
}
