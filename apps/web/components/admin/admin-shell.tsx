"use client";

import Link from "next/link";
import { useLocale, useTranslations } from "next-intl";

const MODULES = [
  "overview",
  "orders",
  "products",
  "card-keys",
  "files",
  "fraud",
  "stats"
] as const;

export type AdminModule = (typeof MODULES)[number];

export function AdminShell({
  module,
  children
}: {
  module: AdminModule;
  children: React.ReactNode;
}) {
  const locale = useLocale();
  const t = useTranslations("admin");

  return (
    <div className="mx-auto max-w-[1400px] px-6 py-8">
      <header className="mb-6 flex items-end justify-between">
        <div>
          <h1 className="text-xl font-bold tracking-tight text-zinc-100">
            {t("title")}
          </h1>
          <p className="mt-0.5 text-xs text-zinc-500">{t("subtitle")}</p>
        </div>
        <div className="hidden items-center gap-3 text-[11px] text-zinc-600 sm:flex">
          <span className="font-mono">{t("moduleTitle." + module)}</span>
        </div>
      </header>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-[220px_1fr]">
        <aside className="rounded-2xl border border-zinc-800/80 bg-zinc-900/50 p-4 backdrop-blur-sm">
          <div className="mb-3 text-[11px] font-medium uppercase tracking-wider text-zinc-600">
            {t("modules")}
          </div>
          <nav className="space-y-1">
            {MODULES.map((m) => (
              <Link
                key={m}
                href={`/${locale}/admin/${m}`}
                className={
                  "block rounded-lg px-3 py-2 text-sm transition " +
                  (module === m
                    ? "bg-indigo-500/15 font-medium text-indigo-300 shadow-[inset_0_0_0_1px_rgba(99,102,241,0.35)]"
                    : "text-zinc-500 hover:bg-zinc-800/50 hover:text-zinc-300")
                }
              >
                {t(`module.${m}`)}
              </Link>
            ))}
          </nav>
        </aside>

        <section className="min-w-0">{children}</section>
      </div>
    </div>
  );
}
