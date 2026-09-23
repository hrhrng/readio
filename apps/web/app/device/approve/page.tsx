"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";

import { InlineFeedback } from "@/components/feedback";
import { authClient, useSession } from "@/lib/auth-client";
import { useLocale } from "@/components/locale-provider";

export default function DeviceApprovalPage() {
  const { t } = useLocale();
  const router = useRouter();
  const { data: session, isPending } = useSession();
  const [userCode, setUserCode] = useState("");
  const [status, setStatus] = useState<"idle" | "approved" | "denied">("idle");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const code = new URLSearchParams(window.location.search).get("user_code") ?? "";
    setUserCode(code);
    if (!isPending && !session?.user) {
      const returnPath = `/device/approve?user_code=${encodeURIComponent(code)}`;
      router.replace(`/login?redirect=${encodeURIComponent(returnPath)}`);
    }
  }, [isPending, router, session?.user]);

  async function decide(approved: boolean) {
    if (!userCode) return;
    setLoading(true);
    setError("");
    const result = approved
      ? await authClient.device.approve({ userCode })
      : await authClient.device.deny({ userCode });
    if (result.error) {
      setError(
        result.error.error_description ??
          t("device.updateFailed"),
      );
      setLoading(false);
      return;
    }
    setStatus(approved ? "approved" : "denied");
    setLoading(false);
  }

  if (status !== "idle") {
    return (
      <main className="auth-frame auth-stack mx-auto min-h-screen max-w-md justify-center px-6 text-center">
        <h1 className="auth-brand">
          {status === "approved" ? t("device.connected") : t("device.denied")}
        </h1>
        <p className="auth-description">
          {status === "approved"
            ? t("device.connectedDescription")
            : t("device.deniedDescription")}
        </p>
        <Link href="/" className="ui-button ui-button--primary">
          {t("device.backToLibrary")}
        </Link>
      </main>
    );
  }

  return (
    <main className="auth-frame auth-stack mx-auto min-h-screen max-w-md justify-center px-6">
      <div className="auth-heading">
        <h1 className="auth-brand">{t("device.approveTitle")}</h1>
        <p className="auth-description">
          {t("device.approveDescription")}
        </p>
      </div>
      {error && <InlineFeedback tone="danger">{error}</InlineFeedback>}
      <div className="surface-popover p-4 text-center">
        <p className="text-xs text-text-secondary">{t("device.code")}</p>
        <p className="mt-1 font-mono text-lg tracking-widest text-text-primary">
          {userCode || "—"}
        </p>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <button
          type="button"
          className="ui-button ui-button--secondary"
          disabled={loading || isPending}
          onClick={() => void decide(false)}
        >
          {t("device.deny")}
        </button>
        <button
          type="button"
          className="ui-button ui-button--primary"
          disabled={loading || isPending}
          onClick={() => void decide(true)}
        >
          {t("device.approve")}
        </button>
      </div>
    </main>
  );
}
