"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { signIn } from "@/lib/auth-client";
import { BrandMark } from "@/components/brand-mark";
import { InlineFeedback } from "@/components/feedback";
import { useLocale } from "@/components/locale-provider";

export default function LoginPage() {
  const { t } = useLocale();
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setLoading(true);

    const { error: authError } = await signIn.email({
      email,
      password,
    });

    if (authError) {
      setError(authError.message ?? t("auth.signInFailed"));
      setLoading(false);
      return;
    }

    const requested = new URLSearchParams(window.location.search).get("redirect");
    const destination =
      requested?.startsWith("/") && !requested.startsWith("//")
        ? requested
        : "/";
    router.push(destination);
    router.refresh();
  }

  return (
    <div className="auth-frame auth-stack">
      <div className="auth-heading">
        <h1 className="auth-brand">
          <BrandMark size="lg" />
        </h1>
        <p className="auth-description">{t("auth.signInDescription")}</p>
      </div>

      <form onSubmit={handleSubmit} className="auth-form">
        {error && <InlineFeedback tone="danger">{error}</InlineFeedback>}

        <div className="form-field">
          <label
            htmlFor="email"
            className="form-label"
          >
            {t("auth.email")}
          </label>
          <input
            id="email"
            type="email"
            required
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className="ui-input"
            placeholder="you@example.com"
          />
        </div>

        <div className="form-field">
          <label
            htmlFor="password"
            className="form-label"
          >
            {t("auth.password")}
          </label>
          <input
            id="password"
            type="password"
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="ui-input"
            placeholder={t("auth.enterPassword")}
          />
        </div>

        <button
          type="submit"
          disabled={loading}
          className="ui-button ui-button--primary w-full"
        >
          {loading ? t("auth.signingIn") : t("auth.signIn")}
        </button>
      </form>

      <p className="auth-footer text-center">
        {t("auth.noAccount")}{" "}
        <Link href="/signup" className="text-accent hover:underline underline-offset-4">
          {t("auth.signUp")}
        </Link>
      </p>
      <p className="auth-footer text-center">
        <Link href="/" className="text-text-secondary hover:text-text-primary">
          {t("auth.continueLocal")}
        </Link>
      </p>
    </div>
  );
}
