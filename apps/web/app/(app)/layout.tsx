"use client";

import { SettingsProvider } from "@/components/settings-provider";
import { LayoutShell } from "@/components/layout-shell";

export default function AppLayout({ children }: { children: React.ReactNode }) {
  return (
    <SettingsProvider>
      <LayoutShell>{children}</LayoutShell>
    </SettingsProvider>
  );
}
