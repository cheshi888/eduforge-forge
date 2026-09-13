"use client";

import { useLocale } from "next-intl";

const LOCALES = ["zh", "en", "ja", "ko"] as const;

export function LocaleSwitcher({ locale }: { locale: string }) {
  return (
    <div className="flex items-center gap-0.5 rounded-lg border border-zinc-800 bg-zinc-900/50 p-0.5">
      {LOCALES.map((l) => {
        const currentPath = typeof window !== "undefined" ? window.location.pathname : `/${locale}`;
        const target = currentPath.replace(new RegExp(`^/${locale}`), `/${l}`) || `/${l}`;
        return (
          <a
            key={l}
            href={target}
            className={
              "rounded-md px-2 py-1 text-[11px] font-medium transition " +
              (l === locale
                ? "bg-indigo-500/20 text-indigo-300 shadow-[inset_0_0_0_1px_rgba(99,102,241,0.35)]"
                : "text-zinc-500 hover:text-zinc-300")
            }
          >
            {l.toUpperCase()}
          </a>
        );
      })}
    </div>
  );
}
