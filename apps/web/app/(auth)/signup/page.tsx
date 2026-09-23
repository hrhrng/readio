"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { signUp } from "@/lib/auth-client";
import { BrandMark } from "@/components/brand-mark";
import { InlineFeedback } from "@/components/feedback";
import { useLocale } from "@/components/locale-provider";

export default function SignupPage() {
  const { t } = useLocale();
  const router = useRouter();
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setLoading(true);

    const { error: authError } = await signUp.email({
      name,
      email,
      password,
    });

    if (authError) {
      setError(authError.message ?? t("auth.signUpFailed"));
      setLoading(false);
      return;
    }

    router.push("/");
    router.refresh();
  }

  return (
    <div className="auth-frame auth-stack">
      <div className="auth-heading">
        <h1 className="auth-brand">
          <BrandMark size="lg" />
        </h1>
        <p className="auth-description">{t("auth.createAccount")}</p>
      </div>

      <form onSubmit={handleSubmit} className="auth-form">
        {error && <InlineFeedback tone="danger">{error}</InlineFeedback>}

        <div className="form-field">
          <label
            htmlFor="name"
            className="form-label"
          >
            {t("auth.name")}
          </label>
          <input
            id="name"
            type="text"
            required
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="ui-input"
            placeholder={t("auth.yourName")}
          />
        </div>

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
            minLength={8}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="ui-input"
            placeholder={t("auth.passwordHint")}
          />
        </div>

        <button
          type="submit"
          disabled={loading}
          className="ui-button ui-button--primary w-full"
        >
          {loading ? t("auth.creatingAccount") : t("auth.signUp")}
        </button>
      </form>

      <p className="auth-footer text-center">
        {t("auth.haveAccount")}{" "}
        <Link href="/login" className="text-accent hover:underline underline-offset-4">
          {t("auth.signIn")}
        </Link>
      </p>
    </div>
  );
}
