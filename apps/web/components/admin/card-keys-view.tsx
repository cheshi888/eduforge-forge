"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocale, useTranslations } from "next-intl";
import {
  listCardKeys,
  listProducts,
  generateCardKeys,
  revokeCardKey,
  AdminApiError,
  type CardKey
} from "@/lib/admin-api";
import { Card, Skeleton, ErrorBox, StatusBadge, EmptyState, GHOST } from "./ui";

export function CardKeysView() {
  const t = useTranslations("admin");
  const [keys, setKeys] = useState<CardKey[] | null>(null);
  const [products, setProducts] = useState<Awaited<ReturnType<typeof listProducts>>>([]);
  const [error, setError] = useState<AdminApiError | Error | null>(null);
  const [loading, setLoading] = useState(true);
  const [productId, setProductId] = useState("");
  const [count, setCount] = useState("10");
  const [genBusy, setGenBusy] = useState(false);
  const [genMsg, setGenMsg] = useState<string | null>(null);
  const [revokeBusy, setRevokeBusy] = useState<number | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [ks, ps] = await Promise.all([listCardKeys(), listProducts()]);
      setKeys(ks);
      setProducts(ps);
    } catch (e) {
      setError(e as AdminApiError | Error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const cardProducts = useMemo(
    () => products.filter((p) => p.category === "card"),
    [products]
  );

  const generate = async () => {
    const pid = Number(productId);
    const n = Number(count);
    if (!pid || n < 1 || n > 500) return;
    setGenBusy(true);
    setGenMsg(null);
    try {
      const res = await generateCardKeys(pid, n);
      setGenMsg(t("generated", { n: res.count }));
      await load();
    } catch (e) {
      setGenMsg((e as AdminApiError | Error).message);
    } finally {
      setGenBusy(false);
    }
  };

  const revoke = async (k: CardKey) => {
    setRevokeBusy(k.id);
    setGenMsg(null);
    try {
      await revokeCardKey(k.id);
      setGenMsg(t("revokeOk"));
      await load();
    } catch (e) {
      setGenMsg((e as AdminApiError | Error).message);
    } finally {
      setRevokeBusy(null);
    }
  };

  const counts = useMemo(() => {
    const c: Record<string, number> = {};
    for (const k of keys ?? []) c[k.status] = (c[k.status] ?? 0) + 1;
    return c;
  }, [keys]);

  return (
    <div>
      {error ? (
        <ErrorBox error={error} onRetry={() => void load()} />
      ) : (
        <>
          <div className="mb-4 flex flex-wrap items-center gap-2">
            {(["AVAILABLE", "LOCKED", "USED", "REVERSED"] as const).map((s) => (
              <span
                key={s}
                className="rounded-md border border-zinc-800/80 bg-zinc-900/50 px-2 py-1 font-mono text-[11px] text-zinc-500"
              >
                {s}: <span className="text-zinc-300">{counts[s] ?? 0}</span>
              </span>
            ))}
          </div>

          <Card title={t("generateTitle")}>
            <div className="flex flex-wrap items-end gap-3 p-5">
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">{t("productId")}</label>
                <select
                  value={productId}
                  onChange={(e) => setProductId(e.target.value)}
                  className="rounded-lg border border-zinc-700/80 bg-zinc-950/60 px-2.5 py-2 text-xs text-zinc-300 focus:border-indigo-500/60 focus:outline-none"
                >
                  <option value="">{t("selectProduct")}</option>
                  {cardProducts.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.sku} · {p.title}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">
                  {t("countLabel")} (1-500)
                </label>
                <input
                  type="number"
                  min={1}
                  max={500}
                  value={count}
                  onChange={(e) => setCount(e.target.value)}
                  className="w-28 rounded-lg border border-zinc-700/80 bg-zinc-950/60 px-2.5 py-2 text-xs text-zinc-300 focus:border-indigo-500/60 focus:outline-none"
                />
              </div>
              <button
                type="button"
                disabled={genBusy || !productId}
                onClick={() => void generate()}
                className="rounded-lg border border-indigo-500/50 bg-gradient-to-r from-indigo-500/25 to-purple-500/25 px-4 py-2 text-xs font-semibold text-indigo-200 transition hover:from-indigo-500/40 hover:to-purple-500/40 hover:shadow-[0_0_16px_rgba(99,102,241,0.35)] disabled:cursor-not-allowed disabled:opacity-50"
              >
                {genBusy ? t("generating") : t("generate")}
              </button>
              {genMsg && (
                <span
                  className={`text-[11px] ${
                    genMsg === t("revokeOk") || genMsg.startsWith(t("generatedPrefix"))
                      ? "text-emerald-400"
                      : "text-rose-400"
                  }`}
                >
                  {genMsg}
                </span>
              )}
            </div>
          </Card>

          <Card className="mt-4" title={t("keysTitle")}>
            {loading ? (
              <Skeleton rows={8} />
            ) : keys && keys.length > 0 ? (
              <div className="overflow-x-auto">
                <table className="w-full text-left text-xs">
                  <thead>
                    <tr className="text-[11px] uppercase tracking-wider text-zinc-600">
                      <th className="px-5 py-2.5 font-medium">{t("th.key")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.productSku")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.status")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.orderNo")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.time")}</th>
                      <th className="px-5 py-2.5 text-right font-medium">{t("actions")}</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-zinc-800/60">
                    {keys.map((k) => (
                      <tr key={k.id} className="transition hover:bg-zinc-800/30">
                        <td className="px-5 py-3 font-mono text-[11px] text-zinc-400">
                          {k.keyCode.slice(0, 16)}…
                        </td>
                        <td className="px-3 py-3 font-mono text-[11px] text-zinc-500">
                          {k.productSku}
                        </td>
                        <td className="px-3 py-3">
                          <StatusBadge status={k.status} />
                        </td>
                        <td className="px-3 py-3 font-mono text-[11px] text-zinc-500">
                          {k.orderOutTradeNo ?? "—"}
                        </td>
                        <td className="px-3 py-3 text-zinc-600">
                          {new Date(k.createdAt).toLocaleString()}
                        </td>
                        <td className="px-5 py-3 text-right">
                          {k.status === "USED" && (
                            <button
                              type="button"
                              disabled={revokeBusy !== null}
                              onClick={() => void revoke(k)}
                              className={`${GHOST} hover:border-rose-500/50 hover:text-rose-400`}
                            >
                              {t("revoke")}
                            </button>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <EmptyState>{t("noKeys")}</EmptyState>
            )}
          </Card>
        </>
      )}
    </div>
  );
}
