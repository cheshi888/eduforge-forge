"use client";

import { useEffect, useMemo, useState } from "react";
import { useLocale, useTranslations } from "next-intl";
import { fetchCatalog, type CatalogProduct } from "@/lib/catalog";
import { ProductCard } from "@/components/shop/product-card";

type Category = CatalogProduct["category"] | "all";

const CATEGORIES: { key: Category; labelKey: string }[] = [
  { key: "all", labelKey: "all" },
  { key: "source", labelKey: "source" },
  { key: "tutorial", labelKey: "tutorial" },
  { key: "card", labelKey: "card" },
  { key: "resource", labelKey: "resource" }
];

export function ProductGrid() {
  const locale = useLocale();
  const t = useTranslations("shop");
  const [cat, setCat] = useState<Category>("all");
  const [q, setQ] = useState("");
  const [products, setProducts] = useState<CatalogProduct[] | null>(null);
  const [err, setErr] = useState<string | null>(null);

  // 从后端商品目录服务拉取真实商品 (分类筛选/搜索由服务端执行)
  useEffect(() => {
    setErr(null);
    fetchCatalog({ category: cat, search: q.trim() || undefined })
      .then(setProducts)
      .catch((e) => {
        setErr(String(e?.message ?? e));
        setProducts([]);
      });
  }, [cat, q]);

  const list = useMemo(() => products ?? [], [products]);

  return (
    <div>
      <header className="mb-6">
        <h1 className="text-2xl font-bold tracking-tight text-zinc-100">
          {t("title")}
          <span className="ml-3 bg-gradient-to-r from-indigo-400 to-purple-400 bg-clip-text text-xs font-medium text-transparent">
            EduForge Store
          </span>
        </h1>
        <p className="mt-1 text-sm text-zinc-500">{t("subtitle")}</p>
      </header>

      <div className="mb-5 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex flex-wrap gap-2">
          {CATEGORIES.map((c) => (
            <button
              key={c.key}
              type="button"
              onClick={() => setCat(c.key)}
              className={
                "rounded-full border px-3.5 py-1.5 text-xs font-medium transition " +
                (cat === c.key
                  ? "border-indigo-500/60 bg-indigo-500/15 text-indigo-300 shadow-[0_0_12px_rgba(99,102,241,0.25)]"
                  : "border-zinc-800 bg-zinc-900/40 text-zinc-500 hover:border-zinc-700 hover:text-zinc-300")
              }
            >
              {t(c.labelKey)}
            </button>
          ))}
        </div>
        <input
          type="search"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder={t("searchPlaceholder")}
          className="w-full rounded-xl border border-zinc-800 bg-zinc-900/50 px-4 py-2 text-sm text-zinc-200 outline-none transition placeholder:text-zinc-600 focus:border-indigo-500/60 focus:shadow-[0_0_15px_rgba(99,102,241,0.2)] sm:w-56"
        />
      </div>

      {products === null ? (
        <div className="space-y-4">
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className="forge-skeleton h-40 rounded-2xl" />
          ))}
        </div>
      ) : err ? (
        <div className="rounded-2xl border border-rose-500/30 bg-rose-500/10 py-16 text-center text-sm text-rose-400">
          {t("unavailable")}
        </div>
      ) : list.length === 0 ? (
        <div className="rounded-2xl border border-zinc-800/80 bg-zinc-900/40 py-16 text-center text-sm text-zinc-500">
          {t("noMatch")}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {list.map((p) => (
            <ProductCard key={p.sku} p={p} locale={locale} />
          ))}
        </div>
      )}
    </div>
  );
}
