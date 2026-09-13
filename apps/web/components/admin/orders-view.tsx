"use client";

import { useCallback, useEffect, useState } from "react";
import { useLocale, useTranslations } from "next-intl";
import {
  listOrders,
  getOrder,
  shipOrder,
  AdminApiError,
  type Order,
  type OrderDetail
} from "@/lib/admin-api";
import { formatMicro, lockedUsd } from "@/lib/amount";
import {
  Card,
  Skeleton,
  ErrorBox,
  StatusBadge,
  EmptyState,
  GHOST
} from "./ui";

const GATEWAYS = ["alipay", "wechat", "stripe", "paypal", "usdt"];
const STATUSES = ["PENDING", "PAID", "SHIPPED", "CANCELLED", "FAILED"];
const PAGE = 50;

export function OrdersView() {
  const locale = useLocale();
  const t = useTranslations("admin");
  const [status, setStatus] = useState("");
  const [gateway, setGateway] = useState("");
  const [offset, setOffset] = useState(0);
  const [orders, setOrders] = useState<Order[] | null>(null);
  const [error, setError] = useState<AdminApiError | Error | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [actionMsg, setActionMsg] = useState<string | null>(null);
  const [detail, setDetail] = useState<OrderDetail | null>(null);
  const [drawerFor, setDrawerFor] = useState<string | null>(null);
  const [drawerErr, setDrawerErr] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setOrders(
        await listOrders({
          status: status || undefined,
          gatewayType: gateway || undefined,
          limit: PAGE,
          offset
        })
      );
    } catch (e) {
      setError(e as AdminApiError | Error);
    } finally {
      setLoading(false);
    }
  }, [status, gateway, offset]);

  useEffect(() => {
    setOffset(0);
  }, [status, gateway]);

  useEffect(() => {
    void load();
  }, [load]);

  const openDrawer = async (no: string) => {
    setDrawerFor(no);
    setDrawerErr(null);
    setDetail(null);
    try {
      setDetail(await getOrder(no));
    } catch (e) {
      setDrawerErr((e as AdminApiError | Error).message);
    }
  };

  const ship = async (no: string) => {
    setBusy(true);
    setActionMsg(null);
    try {
      await shipOrder(no);
      setActionMsg(t("shippedOk"));
      await load();
    } catch (e) {
      setActionMsg((e as AdminApiError | Error).message);
    } finally {
      setBusy(false);
    }
  };

  const hasNext = orders !== null && orders.length === PAGE;

  return (
    <div>
      {error ? (
        <ErrorBox error={error} onRetry={() => void load()} />
      ) : (
        <>
          <Card>
            <div className="flex flex-wrap items-center gap-3 border-b border-zinc-800/80 px-5 py-3.5">
              <select
                value={status}
                onChange={(e) => setStatus(e.target.value)}
                className="rounded-lg border border-zinc-700/80 bg-zinc-950/60 px-2.5 py-1.5 text-xs text-zinc-300 focus:border-indigo-500/60 focus:outline-none"
              >
                <option value="">{t("allStatus")}</option>
                {STATUSES.map((s) => (
                  <option key={s} value={s}>
                    {s}
                  </option>
                ))}
              </select>
              <select
                value={gateway}
                onChange={(e) => setGateway(e.target.value)}
                className="rounded-lg border border-zinc-700/80 bg-zinc-950/60 px-2.5 py-1.5 text-xs text-zinc-300 focus:border-indigo-500/60 focus:outline-none"
              >
                <option value="">{t("allGateways")}</option>
                {GATEWAYS.map((g) => (
                  <option key={g} value={g}>
                    {g}
                  </option>
                ))}
              </select>
              <span className="ml-auto text-[11px] text-zinc-600">
                {t("offsetPage", { offset })}
              </span>
            </div>

            {loading ? (
              <Skeleton rows={8} />
            ) : orders && orders.length > 0 ? (
              <div className="overflow-x-auto">
                <table className="w-full text-left text-xs">
                  <thead>
                    <tr className="text-[11px] uppercase tracking-wider text-zinc-600">
                      <th className="px-5 py-2.5 font-medium">{t("th.orderNo")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.product")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.amount")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.gateway")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.status")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.time")}</th>
                      <th className="px-5 py-2.5 font-medium">{t("actions")}</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-zinc-800/60">
                    {orders.map((o) => (
                      <tr key={o.outTradeNo} className="transition hover:bg-zinc-800/30">
                        <td className="px-5 py-3">
                          <button
                            type="button"
                            onClick={() => void openDrawer(o.outTradeNo)}
                            className="font-mono text-[11px] text-indigo-300 transition hover:text-indigo-200"
                          >
                            {o.outTradeNo}
                          </button>
                        </td>
                        <td className="max-w-[180px] truncate px-3 py-3 text-zinc-300">
                          {o.productTitle}
                        </td>
                        <td className="px-3 py-3 font-mono text-zinc-300">
                          {formatMicro(o.expectedAmountMicro, locale, o.currency)}
                        </td>
                        <td className="px-3 py-3 text-zinc-500">
                          {o.gatewayType ?? "—"}
                          {o.cryptoNetwork ? ` · ${o.cryptoNetwork}` : ""}
                        </td>
                        <td className="px-3 py-3">
                          <StatusBadge status={o.status} />
                        </td>
                        <td className="px-3 py-3 text-zinc-600">
                          {new Date(o.createdAt).toLocaleString(locale)}
                        </td>
                        <td className="px-5 py-3 text-right">
                          {o.status === "PAID" && (
                            <button
                              type="button"
                              disabled={busy}
                              onClick={() => void ship(o.outTradeNo)}
                              className={GHOST}
                            >
                              {t("ship")}
                            </button>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <EmptyState>{t("noOrders")}</EmptyState>
            )}

            {(loading || !error) && orders && orders.length > 0 && (
              <div className="flex items-center justify-between border-t border-zinc-800/80 px-5 py-3">
                <div className="flex gap-2">
                  <button
                    type="button"
                    className={GHOST}
                    disabled={offset === 0}
                    onClick={() => setOffset(Math.max(0, offset - PAGE))}
                  >
                    ← {t("prevPage")}
                  </button>
                  <button
                    type="button"
                    className={GHOST}
                    disabled={!hasNext}
                    onClick={() => setOffset(offset + PAGE)}
                  >
                    {t("nextPage")} →
                  </button>
                </div>
                {actionMsg && (
                  <span
                    className={`text-[11px] ${actionMsg === t("shippedOk") ? "text-emerald-400" : "text-rose-400"}`}
                  >
                    {actionMsg}
                  </span>
                )}
              </div>
            )}
          </Card>

          {drawerFor && (
            <div
              className="fixed inset-0 z-40 flex justify-end bg-black/50 backdrop-blur-sm"
              onClick={() => setDrawerFor(null)}
            >
              <div
                className="h-full w-full max-w-md overflow-y-auto border-l border-zinc-800/80 bg-zinc-950/95 p-5 backdrop-blur-xl"
                onClick={(e) => e.stopPropagation()}
              >
                <div className="mb-4 flex items-center justify-between">
                  <span className="font-mono text-sm text-indigo-300">{drawerFor}</span>
                  <button
                    type="button"
                    className="text-xs text-zinc-500 transition hover:text-zinc-300"
                    onClick={() => setDrawerFor(null)}
                  >
                    ✕
                  </button>
                </div>

                {drawerErr ? (
                  <div className="rounded-xl border border-rose-500/40 bg-rose-500/10 p-4 text-xs text-rose-300">
                    {drawerErr}
                  </div>
                ) : detail ? (
                  <div className="space-y-4">
                    <div className="rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-4">
                      <div className="mb-2 text-xs font-semibold text-zinc-300">
                        {t("orderInfo")}
                      </div>
                      <dl className="space-y-1.5 text-xs text-zinc-500">
                        <div className="flex justify-between gap-4">
                          <dt>{t("th.product")}</dt>
                          <dd className="text-right text-zinc-300">
                            {detail.order.productTitle} × {detail.order.quantity}
                          </dd>
                        </div>
                        <div className="flex justify-between gap-4">
                          <dt>{t("th.amount")}</dt>
                          <dd className="text-right font-mono text-zinc-300">
                            {formatMicro(
                              detail.order.expectedAmountMicro,
                              locale,
                              detail.order.currency
                            )}
                          </dd>
                        </div>
                        <div className="flex justify-between gap-4">
                          <dt>{t("th.status")}</dt>
                          <dd className="text-right">
                            <StatusBadge status={detail.order.status} />
                          </dd>
                        </div>
                        {detail.order.lockedExchangeRate && (
                          <div>
                            <dt>{t("lockedRate")}</dt>
                            <dd className="mt-1 font-mono text-[11px] text-emerald-400/90">
                              {lockedUsd(
                                detail.order.lockedExchangeRate,
                                detail.order.expectedAmountMicro,
                                locale
                              )}
                            </dd>
                          </div>
                        )}
                      </dl>
                    </div>

                    <div>
                      <div className="mb-2 text-xs font-semibold text-zinc-300">
                        {t("transactions")}
                      </div>
                      {detail.transactions.length === 0 ? (
                        <div className="text-xs text-zinc-600">{t("noTransactions")}</div>
                      ) : (
                        <div className="space-y-2">
                          {detail.transactions.map((tx) => (
                            <div
                              key={tx.gatewayTxnId}
                              className="rounded-lg border border-zinc-800/80 bg-zinc-900/40 p-3 text-xs"
                            >
                              <div className="flex items-center justify-between">
                                <span className="font-mono text-[11px] text-zinc-400">
                                  {tx.gatewayTxnId}
                                </span>
                                <StatusBadge status={tx.status} />
                              </div>
                              <div className="mt-1 flex items-center justify-between text-zinc-500">
                                <span>
                                  {tx.gatewayType} ·{" "}
                                  {formatMicro(tx.amountMicro, locale, tx.currency)}
                                </span>
                                {tx.confirmations !== null && (
                                  <span className="font-mono text-[10px]">
                                    {t("confirmations", { n: tx.confirmations })}
                                  </span>
                                )}
                              </div>
                              <div className="mt-1 text-[10px] text-zinc-600">
                                {new Date(tx.receivedAt).toLocaleString(locale)}
                              </div>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>

                    <div>
                      <div className="mb-2 text-xs font-semibold text-zinc-300">
                        {t("orderCardKeys")}
                      </div>
                      {detail.cardKeys.length === 0 ? (
                        <div className="text-xs text-zinc-600">{t("noCardKeys")}</div>
                      ) : (
                        <div className="space-y-2">
                          {detail.cardKeys.map((k) => (
                            <div
                              key={k.id}
                              className="flex items-center justify-between rounded-lg border border-zinc-800/80 bg-zinc-900/40 p-3"
                            >
                              <span className="truncate font-mono text-[11px] text-zinc-300">
                                {k.keyCode}
                              </span>
                              <StatusBadge status={k.status} />
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  </div>
                ) : (
                  <div className="forge-skeleton h-48 rounded-xl" />
                )}
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}
