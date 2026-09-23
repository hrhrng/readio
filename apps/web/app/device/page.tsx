"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";

import { InlineFeedback } from "@/components/feedback";
import { authClient, useSession } from "@/lib/auth-client";
import { useLocale } from "@/components/locale-provider";

function normalizeCode(value: string) {
  return value.trim().replaceAll("-", "").toUpperCase();
}

export default function DeviceAuthorizationPage() {
  const { t } = useLocale();
  const router = useRouter();
  const { data: session, isPending } = useSession();
  const [userCode, setUserCode] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const code = new URLSearchParams(window.location.search).get("user_code");
    if (code) setUserCode(code);
  }, []);

  async function verify(event: React.FormEvent) {
    event.preventDefault();
    const code = normalizeCode(userCode);
    if (!code) return;

    const returnPath = `/device?user_code=${encodeURIComponent(code)}`;
    if (!session?.user) {
      router.push(`/login?redirect=${encodeURIComponent(returnPath)}`);
      return;
    }

    setLoading(true);
    setError("");
    const { error: verificationError } = await authClient.device({
      query: { user_code: code },
    });
    if (verificationError) {
      setError(
        verificationError.error_description ??
          t("device.invalidCode"),
      );
      setLoading(false);
      return;
    }
    router.push(`/device/approve?user_code=${encodeURIComponent(code)}`);
  }

  return (
    <main className="auth-frame auth-stack mx-auto min-h-screen max-w-md justify-center px-6">
      <div className="auth-heading">
        <h1 className="auth-brand">{t("device.connectTitle")}</h1>
        <p className="auth-description">
          {t("device.connectDescription")}
        </p>
      </div>

      <form onSubmit={verify} className="auth-form">
        {error && <InlineFeedback tone="danger">{error}</InlineFeedback>}
        <div className="form-field">
          <label htmlFor="user-code" className="form-label">
            {t("device.code")}
          </label>
          <input
            id="user-code"
            value={userCode}
            onChange={(event) => setUserCode(event.target.value)}
            className="ui-input font-mono uppercase tracking-widest"
            autoComplete="one-time-code"
            placeholder="ABCD-1234"
            maxLength={12}
            required
          />
        </div>
        <button
          type="submit"
          className="ui-button ui-button--primary w-full"
          disabled={loading || isPending}
        >
          {loading ? t("device.checking") : t("device.continue")}
        </button>
      </form>
    </main>
  );
}
