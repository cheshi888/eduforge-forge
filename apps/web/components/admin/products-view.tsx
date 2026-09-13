"use client";

import { useCallback, useEffect, useState } from "react";
import { useLocale, useTranslations } from "next-intl";
import {
  listProducts,
  createProduct,
  updateProduct,
  deleteProduct,
  adjustStock,
  AdminApiError,
  type Product,
  type ProductUpsertReq,
  type ProductCategory
} from "@/lib/admin-api";
import { formatMicro } from "@/lib/amount";
import { Card, Skeleton, ErrorBox, EmptyState, FIELD, GHOST } from "./ui";

const CATEGORIES: ProductCategory[] = ["source", "tutorial", "card", "resource"];
const CNY = "CNY";

const EMPTY_FORM: ProductUpsertReq = {
  sku: "",
  title: "",
  category: "tutorial",
  cover: "",
  description: "",
  unitPriceMicro: 0,
  currency: CNY,
  stock: 0,
  active: true
};

export function ProductsView() {
  const locale = useLocale();
  const t = useTranslations("admin");
  const [products, setProducts] = useState<Product[] | null>(null);
  const [error, setError] = useState<AdminApiError | Error | null>(null);
  const [loading, setLoading] = useState(true);
  const [form, setForm] = useState<ProductUpsertReq | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [actionErr, setActionErr] = useState<string | null>(null);
  const [stockBusy, setStockBusy] = useState<number | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setProducts(await listProducts());
    } catch (e) {
      setError(e as AdminApiError | Error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const openCreate = () => {
    setEditingId(null);
    setForm({ ...EMPTY_FORM });
    setActionErr(null);
  };

  const openEdit = (p: Product) => {
    setEditingId(p.id);
    setForm({
      sku: p.sku,
      title: p.title,
      category: p.category,
      cover: p.cover ?? "",
      description: p.description ?? "",
      unitPriceMicro: p.unitPriceMicro,
      currency: p.currency,
      stock: p.stock,
      active: p.active
    });
    setActionErr(null);
  };

  const submit = async () => {
    if (!form || !form.sku.trim() || !form.title.trim()) return;
    setSubmitting(true);
    setActionErr(null);
    const body: ProductUpsertReq = {
      ...form,
      cover: form.cover ? form.cover : undefined,
      description: form.description ? form.description : undefined
    };
    try {
      if (editingId !== null) {
        await updateProduct(editingId, body);
      } else {
        await createProduct(body);
      }
      setForm(null);
      await load();
    } catch (e) {
      setActionErr((e as AdminApiError | Error).message);
    } finally {
      setSubmitting(false);
    }
  };

  const remove = async (p: Product) => {
    setStockBusy(null);
    setActionErr(null);
    if (!window.confirm(t("confirmDelete", { title: p.title }))) return;
    setStockBusy(p.id);
    try {
      await deleteProduct(p.id);
      await load();
    } catch (e) {
      setActionErr((e as AdminApiError | Error).message);
      setStockBusy(null);
    }
  };

  const stock = async (p: Product, delta: number) => {
    setStockBusy(p.id);
    setActionErr(null);
    try {
      await adjustStock(p.id, delta);
      await load();
    } catch (e) {
      setActionErr((e as AdminApiError | Error).message);
    } finally {
      setStockBusy(null);
    }
  };

  return (
    <div>
      {error ? (
        <ErrorBox error={error} onRetry={() => void load()} />
      ) : (
        <Card
          title={t("productsTitle")}
          actions={
            <button type="button" onClick={openCreate} className={GHOST}>
              + {t("newProduct")}
            </button>
          }
        >
          {loading ? (
            <Skeleton rows={6} />
          ) : products && products.length > 0 ? (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead>
                  <tr className="text-[11px] uppercase tracking-wider text-zinc-600">
                    <th className="px-5 py-2.5 font-medium">{t("th.sku")}</th>
                    <th className="px-3 py-2.5 font-medium">{t("th.title")}</th>
                    <th className="px-3 py-2.5 font-medium">{t("th.category")}</th>
                    <th className="px-3 py-2.5 font-medium">{t("th.price")}</th>
                    <th className="px-3 py-2.5 font-medium">{t("th.stock")}</th>
                    <th className="px-3 py-2.5 font-medium">{t("th.active")}</th>
                    <th className="px-5 py-2.5 text-right font-medium">{t("actions")}</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-zinc-800/60">
                  {products.map((p) => (
                    <tr
                      key={p.id}
                      className={`transition hover:bg-zinc-800/30 ${!p.active ? "opacity-50" : ""}`}
                    >
                      <td className="px-5 py-3 font-mono text-[11px] text-zinc-400">{p.sku}</td>
                      <td className="max-w-[180px] truncate px-3 py-3 text-zinc-300">{p.title}</td>
                      <td className="px-3 py-3 text-zinc-500">{p.category}</td>
                      <td className="px-3 py-3 font-mono text-zinc-300">
                        {formatMicro(p.unitPriceMicro, locale, p.currency)}
                      </td>
                      <td className="px-3 py-3">
                        <div className="flex items-center gap-1.5">
                          <button
                            type="button"
                            disabled={stockBusy !== null}
                            onClick={() => void stock(p, -1)}
                            className="rounded-md border border-zinc-700/70 bg-zinc-900/50 px-1.5 py-0.5 text-[11px] text-zinc-400 transition hover:border-rose-500/50 hover:text-rose-400 disabled:opacity-40"
                          >
                            −
                          </button>
                          <span className="w-8 text-center font-mono text-zinc-200">
                            {p.stock}
                          </span>
                          <button
                            type="button"
                            disabled={stockBusy !== null}
                            onClick={() => void stock(p, 1)}
                            className="rounded-md border border-zinc-700/70 bg-zinc-900/50 px-1.5 py-0.5 text-[11px] text-zinc-400 transition hover:border-emerald-500/50 hover:text-emerald-400 disabled:opacity-40"
                          >
                            +
                          </button>
                        </div>
                      </td>
                      <td className="px-3 py-3 text-zinc-500">
                        {p.active ? t("yes") : t("no")}
                      </td>
                      <td className="px-5 py-3 text-right">
                        <div className="flex justify-end gap-2">
                          <button
                            type="button"
                            disabled={stockBusy !== null}
                            onClick={() => openEdit(p)}
                            className={GHOST}
                          >
                            {t("edit")}
                          </button>
                          <button
                            type="button"
                            disabled={stockBusy !== null}
                            onClick={() => void remove(p)}
                            className={`${GHOST} hover:border-rose-500/50 hover:text-rose-400`}
                          >
                            {t("delete")}
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <EmptyState>{t("noProducts")}</EmptyState>
          )}

          {actionErr && (
            <div className="border-t border-zinc-800/80 px-5 py-3 text-[11px] text-rose-400">
              {actionErr}
            </div>
          )}
        </Card>
      )}

      {form && (
        <div
          className="fixed inset-0 z-40 flex justify-end bg-black/50 backdrop-blur-sm"
          onClick={() => setForm(null)}
        >
          <div
            className="h-full w-full max-w-md overflow-y-auto border-l border-zinc-800/80 bg-zinc-950/95 p-5 backdrop-blur-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="mb-4 flex items-center justify-between">
              <span className="text-sm font-semibold text-zinc-200">
                {editingId !== null ? t("editProduct") : t("newProduct")}
              </span>
              <button
                type="button"
                className="text-xs text-zinc-500 transition hover:text-zinc-300"
                onClick={() => setForm(null)}
              >
                ✕
              </button>
            </div>

            <div className="space-y-3">
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">{t("th.sku")}</label>
                <input
                  className={FIELD}
                  value={form.sku}
                  onChange={(e) => setForm({ ...form, sku: e.target.value })}
                  placeholder="SKU-001"
                />
              </div>
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">{t("th.title")}</label>
                <input
                  className={FIELD}
                  value={form.title}
                  onChange={(e) => setForm({ ...form, title: e.target.value })}
                />
              </div>
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">{t("th.category")}</label>
                <select
                  className={FIELD}
                  value={form.category}
                  onChange={(e) =>
                    setForm({ ...form, category: e.target.value as ProductCategory })
                  }
                >
                  {CATEGORIES.map((c) => (
                    <option key={c} value={c}>
                      {t(`category.${c}`)}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">
                  {t("th.price")} ({t("priceCny")})
                </label>
                <input
                  type="number"
                  min={0}
                  step={0.01}
                  className={FIELD}
                  value={form.unitPriceMicro === 0 ? "" : form.unitPriceMicro / 1_000_000}
                  onChange={(e) =>
                    setForm({
                      ...form,
                      unitPriceMicro: Number(
                        (Number(e.target.value) * 1_000_000).toFixed(0)
                      ) || 0
                    })
                  }
                  placeholder="199.00"
                />
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="mb-1 block text-[11px] text-zinc-500">{t("th.currency")}</label>
                  <input
                    className={FIELD}
                    value={form.currency}
                    onChange={(e) => setForm({ ...form, currency: e.target.value })}
                  />
                </div>
                <div>
                  <label className="mb-1 block text-[11px] text-zinc-500">{t("th.stock")}</label>
                  <input
                    type="number"
                    min={0}
                    className={FIELD}
                    value={form.stock}
                    onChange={(e) =>
                      setForm({ ...form, stock: Math.max(0, Number(e.target.value) || 0) })
                    }
                  />
                </div>
              </div>
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">{t("th.cover")}</label>
                <input
                  className={FIELD}
                  value={form.cover ?? ""}
                  onChange={(e) => setForm({ ...form, cover: e.target.value })}
                  placeholder="https://…"
                />
              </div>
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">
                  {t("th.description")}
                </label>
                <textarea
                  rows={2}
                  className={FIELD}
                  value={form.description ?? ""}
                  onChange={(e) => setForm({ ...form, description: e.target.value })}
                />
              </div>
              <label className="flex items-center gap-2 text-xs text-zinc-400">
                <input
                  type="checkbox"
                  className="h-4 w-4 rounded border-zinc-700 bg-zinc-950 accent-indigo-500"
                  checked={form.active}
                  onChange={(e) => setForm({ ...form, active: e.target.checked })}
                />
                {t("th.active")}
              </label>

              {actionErr && (
                <div className="rounded-lg border border-rose-500/40 bg-rose-500/10 p-3 text-[11px] text-rose-400">
                  {actionErr}
                </div>
              )}

              <button
                type="button"
                disabled={submitting || !form.sku.trim() || !form.title.trim()}
                className="w-full rounded-lg border border-indigo-500/50 bg-gradient-to-r from-indigo-500/25 to-purple-500/25 px-3 py-2.5 text-xs font-semibold text-indigo-200 transition hover:from-indigo-500/40 hover:to-purple-500/40 hover:shadow-[0_0_16px_rgba(99,102,241,0.35)] disabled:cursor-not-allowed disabled:opacity-50"
                onClick={() => void submit()}
              >
                {submitting ? t("saving") : t("save")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
