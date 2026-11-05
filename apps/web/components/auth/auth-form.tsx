"use client";

import { useState, useActionState } from "react";
import { useTranslations } from "next-intl";
import { loginAction, registerAction, type AuthState } from "@/app/actions/auth";

export function AuthForm({ locale }: { locale: string }) {
  const t = useTranslations("auth");
  const [mode, setMode] = useState<"login" | "register">("login");
  const action = mode === "login" ? loginAction : registerAction;
  const [state, formAction, isPending] = useActionState<AuthState | null, FormData>(
    action,
    null
  );

  return (
    <section className="forge-fade-up relative overflow-hidden rounded-2xl border border-zinc-800/80 bg-zinc-900/50 p-6 backdrop-blur-md">
      <div className="pointer-events-none absolute inset-0 bg-gradient-to-br from-indigo-500/5 via-transparent to-purple-500/5" />
      <div className="relative">
        <h1 className="text-lg font-semibold tracking-tight text-zinc-100">
          {mode === "login" ? t("loginTitle") : t("registerTitle")}
        </h1>

        <div className="mt-4 flex gap-2">
          <button
            type="button"
            onClick={() => setMode("login")}
            className={
              "rounded-lg px-3 py-1.5 text-xs font-medium transition " +
              (mode === "login"
                ? "bg-indigo-500/20 text-indigo-300"
                : "text-zinc-400 hover:text-zinc-200")
            }
          >
            {t("login")}
          </button>
          <button
            type="button"
            onClick={() => setMode("register")}
            className={
              "rounded-lg px-3 py-1.5 text-xs font-medium transition " +
              (mode === "register"
                ? "bg-indigo-500/20 text-indigo-300"
                : "text-zinc-400 hover:text-zinc-200")
            }
          >
            {t("register")}
          </button>
        </div>

        <form action={formAction} className="mt-4 space-y-3">
          <div>
            <label className="mb-1 block text-xs text-zinc-400">{t("email")}</label>
            <input
              type="email"
              name="email"
              required
              autoComplete="email"
              className="w-full rounded-lg border border-zinc-700/80 bg-zinc-950/50 px-3 py-2 text-sm text-zinc-100 outline-none transition focus:border-indigo-500/60"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs text-zinc-400">{t("password")}</label>
            <input
              type="password"
              name="password"
              required
              minLength={8}
              autoComplete={mode === "login" ? "current-password" : "new-password"}
              className="w-full rounded-lg border border-zinc-700/80 bg-zinc-950/50 px-3 py-2 text-sm text-zinc-100 outline-none transition focus:border-indigo-500/60"
            />
            <p className="mt-1 text-[11px] text-zinc-600">{t("passwordHint")}</p>
          </div>
          <button
            type="submit"
            disabled={isPending}
            className="w-full rounded-xl bg-gradient-to-r from-indigo-600 via-purple-600 to-indigo-600 px-4 py-3 text-sm font-semibold text-white shadow-[0_0_24px_rgba(99,102,241,0.35)] transition disabled:opacity-50"
          >
            {isPending
              ? t("submitting")
              : mode === "login"
                ? t("submitLogin")
                : t("submitRegister")}
          </button>
        </form>

        {state?.ok ? (
          <p className="mt-3 rounded-lg border border-emerald-500/30 bg-emerald-500/12 px-3 py-2.5 text-xs text-emerald-400">
            {mode === "login" ? t("loginOk") : t("registerOk")}
          </p>
        ) : null}
        {state?.error ? (
          <p className="mt-3 rounded-lg border border-rose-500/30 bg-rose-500/12 px-3 py-2.5 text-xs text-rose-400">
            {t("error")}
          </p>
        ) : null}
      </div>
    </section>
  );
}
