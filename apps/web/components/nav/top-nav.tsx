"use client";

import Link from "next/link";
import { useLocale, useTranslations } from "next-intl";
import { LocaleSwitcher } from "@/components/nav/locale-switcher";

export function TopNav() {
  const locale = useLocale();
  const t = useTranslations("nav");
  return (
    <nav className="sticky top-0 z-20 border-b border-zinc-800/70 bg-[#0a0c14]/75 backdrop-blur-xl">
      <div className="mx-auto flex h-14 max-w-6xl items-center justify-between px-6">
        <Link href={`/${locale}`} className="group flex items-center gap-2.5">
          <div className="relative flex h-8 w-8 items-center justify-center rounded-lg bg-gradient-to-br from-indigo-500 to-purple-600 font-mono text-sm font-bold text-white shadow-[0_0_16px_rgba(99,102,241,0.45)] transition group-hover:shadow-[0_0_24px_rgba(99,102,241,0.65)]">
            F
            <div className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full bg-emerald-400 shadow-[0_0_6px_rgba(52,211,153,0.8)]" />
          </div>
          <div className="flex flex-col">
            <span className="text-sm font-semibold tracking-tight text-zinc-100">
              EduForge
            </span>
            <span className="text-[9px] uppercase tracking-[0.2em] text-zinc-600">
              Pay Console
            </span>
          </div>
        </Link>
        <div className="flex items-center gap-3">
          <LocaleSwitcher locale={locale} />
          <div className="hidden h-5 w-px bg-zinc-800 sm:block" />
          <Link
            href={`/${locale}`}
            className="hidden text-xs text-zinc-400 transition hover:text-zinc-200 sm:block"
          >
            {t("store")}
          </Link>
          <Link
            href={`/${locale}/membership`}
            className="hidden text-xs text-zinc-400 transition hover:text-zinc-200 sm:block"
          >
            {t("membership")}
          </Link>
          <Link
            href={`/${locale}/my-files`}
            className="hidden text-xs text-zinc-400 transition hover:text-zinc-200 sm:block"
          >
            {t("myFiles")}
          </Link>
          <Link
            href={`/${locale}/admin`}
            className="rounded-lg border border-zinc-700/80 bg-zinc-900/50 px-3 py-1.5 text-xs font-medium text-zinc-300 transition hover:border-indigo-500/50 hover:text-indigo-300 hover:shadow-[0_0_14px_rgba(99,102,241,0.25)]"
          >
            {t("admin")}
          </Link>
        </div>
      </div>
    </nav>
  );
}
