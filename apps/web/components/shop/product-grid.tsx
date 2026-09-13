"use client";

import { useMemo, useState } from "react";
import { useLocale } from "next-intl";
import { PRODUCTS, type Product } from "@/lib/products";
import { ProductCard } from "@/components/shop/product-card";

type Category = Product["category"] | "all";

const CATEGORIES: { key: Category; label: string }[] = [
  { key: "all", label: "全部" },
  { key: "source", label: "源码" },
  { key: "tutorial", label: "教程" },
  { key: "card", label: "卡密" },
  { key: "resource", label: "资源包" }
];

export function ProductGrid() {
  const locale = useLocale();
  const [cat, setCat] = useState<Category>("all");
  const [q, setQ] = useState("");

  const list = useMemo(() => {
    return PRODUCTS.filter((p) => {
      const okCat = cat === "all" || p.category === cat;
      const okQ =
        q.trim() === "" ||
        p.name.toLowerCase().includes(q.toLowerCase()) ||
        p.desc.toLowerCase().includes(q.toLowerCase());
      return okCat && okQ;
    });
  }, [cat, q]);

  return (
    <div>
      <header className="mb-6">
        <h1 className="text-2xl font-bold tracking-tight text-zinc-100">
          商品橱窗
          <span className="ml-3 bg-gradient-to-r from-indigo-400 to-purple-400 bg-clip-text text-xs font-medium text-transparent">
            EduForge Store
          </span>
        </h1>
        <p className="mt-1 text-sm text-zinc-500">
          源码 / 教程 / 卡密 / 资源包 · 支付成功后自动发货
        </p>
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
              {c.label}
            </button>
          ))}
        </div>
        <input
          type="search"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="搜索商品…"
          className="w-full rounded-xl border border-zinc-800 bg-zinc-900/50 px-4 py-2 text-sm text-zinc-200 outline-none transition placeholder:text-zinc-600 focus:border-indigo-500/60 focus:shadow-[0_0_15px_rgba(99,102,241,0.2)] sm:w-56"
        />
      </div>

      {list.length === 0 ? (
        <div className="rounded-2xl border border-zinc-800/80 bg-zinc-900/40 py-16 text-center text-sm text-zinc-500">
          没有匹配的商品
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {list.map((p) => (
            <ProductCard key={p.id} p={p} locale={locale} />
          ))}
        </div>
      )}
    </div>
  );
}
