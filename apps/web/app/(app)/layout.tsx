import type { Metadata } from "next";
import { SettingsProvider } from "@/components/settings-provider";
import { LayoutShell } from "@/components/layout-shell";
import { CloudSyncBridge } from "@/components/cloud-sync-bridge";

export const metadata: Metadata = {
  title: "Your library",
  robots: {
    index: false,
    follow: false,
  },
};

export default function AppLayout({ children }: { children: React.ReactNode }) {
  return (
    <SettingsProvider>
      <CloudSyncBridge />
      <LayoutShell>{children}</LayoutShell>
    </SettingsProvider>
  );
}
