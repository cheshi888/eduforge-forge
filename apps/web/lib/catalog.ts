// 商品目录客户端: 从 rust-core /catalog/* 拉取真实商品, 替代硬编码 PRODUCTS.
// 金额 6 位小数微单位 int64, 展示走 lib/amount.ts (BigInt).

export interface CatalogProduct {
  id: number;
  sku: string;
  name: string;
  category: "source" | "tutorial" | "card" | "resource";
  cover: string | null;
  desc: string | null;
  priceMicro: number;
  currency: string;
  rating: number;
  sold: number;
  badge: string | null;
}

const API_BASE =
  typeof window === "undefined"
    ? process.env.FORGE_API_PROXY_TARGET ?? "http://127.0.0.1:8080"
    : "/api";

export async function fetchCatalog(
  opts: { category?: string; search?: string } = {}
): Promise<CatalogProduct[]> {
  const params = new URLSearchParams();
  if (opts.category && opts.category !== "all") params.set("category", opts.category);
  if (opts.search) params.set("search", opts.search);
  const qs = params.toString();
  try {
    const res = await fetch(`${API_BASE}/catalog/products${qs ? `?${qs}` : ""}`, {
      cache: "no-store"
    });
    if (res.ok) {
      return (await res.json()) as CatalogProduct[];
    }
  } catch {
    // 后端不可达
  }
  return [];
}

export async function fetchCatalogProduct(sku: string): Promise<CatalogProduct | null> {
  try {
    const res = await fetch(`${API_BASE}/catalog/products/${encodeURIComponent(sku)}`, {
      cache: "no-store"
    });
    if (res.ok) {
      return (await res.json()) as CatalogProduct;
    }
  } catch {
    // 后端不可达
  }
  return null;
}
