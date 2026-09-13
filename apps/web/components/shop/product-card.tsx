import type { Product } from "@/lib/products";
import Link from "next/link";

const CATEGORY_ICONS: Record<Product["category"], string> = {
  source: "{ }",
  tutorial: "▶",
  card: "◇",
  resource: "▣"
};

export function ProductCard({ p, locale }: { p: Product; locale: string }) {
  const price = Number(BigInt(p.priceMicro) / BigInt(10 ** 6)).toLocaleString("zh-CN", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2
  });

  return (
    <Link
      href={`/${locale}/checkout?item=${p.id}`}
      className="group forge-fade-up relative flex flex-col overflow-hidden rounded-2xl border border-zinc-800/80 bg-zinc-900/50 p-5 backdrop-blur-sm transition-all duration-300 hover:-translate-y-1 hover:border-indigo-500/50 hover:shadow-[0_8px_40px_rgba(99,102,241,0.18)]"
    >
      <div className="pointer-events-none absolute inset-0 rounded-2xl bg-gradient-to-br from-indigo-500/0 via-transparent to-purple-500/0 opacity-0 transition group-hover:from-indigo-500/10 group-hover:opacity-100 group-hover:to-purple-500/10" />

      <div className="relative flex items-start justify-between">
        <div className="flex h-12 w-12 items-center justify-center rounded-xl border border-zinc-700/80 bg-gradient-to-br from-zinc-800 to-zinc-900 text-lg font-mono text-indigo-300 shadow-inner">
          {CATEGORY_ICONS[p.category]}
        </div>
        {p.badge ? (
          <span className="rounded-full border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-0.5 text-[11px] font-medium text-indigo-300">
            {p.badge}
          </span>
        ) : null}
      </div>

      <div className="relative mt-4 flex items-center gap-2">
        <span className="text-[11px] uppercase tracking-wider text-zinc-500">
          {p.categoryLabel}
        </span>
        <span className="h-1 w-1 rounded-full bg-indigo-500/60" />
        <span className="text-[11px] text-zinc-600">★ {p.rating.toFixed(1)}</span>
      </div>

      <h3 className="relative mt-1.5 line-clamp-2 text-sm font-semibold leading-snug text-zinc-100 transition group-hover:text-indigo-300">
        {p.name}
      </h3>
      <p className="relative mt-1.5 line-clamp-2 text-xs leading-relaxed text-zinc-500">
        {p.desc}
      </p>

      <div className="relative mt-4 flex items-baseline justify-between border-t border-zinc-800/80 pt-3.5">
        <div className="flex items-baseline gap-0.5">
          <span className="text-xs font-medium text-zinc-500">¥</span>
          <span className="text-xl font-bold tracking-tight text-zinc-100">
            {price}
          </span>
        </div>
        <span className="text-xs font-medium text-indigo-400 transition group-hover:translate-x-0.5">
          立即购买 →
        </span>
      </div>
    </Link>
  );
}
