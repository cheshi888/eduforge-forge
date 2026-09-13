"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocale, useTranslations } from "next-intl";
import Link from "next/link";
import {
  fetchStats,
  listOrders,
  AdminApiError
} from "@/lib/admin-api";
import { formatMicro } from "@/lib/amount";
import {
  Card,
  Skeleton,
  ErrorBox,
  StatusBadge,
  EmptyState,
  BTN
} from "./ui";

export function OverviewView() {
  const locale = useLocale();
  const t = useTranslations("admin");
  const [stats, setStats] = useState<Awaited<ReturnType<typeof fetchStats>> | null>(null);
  const [orders, setOrders] = useState<Awaited<ReturnType<typeof listOrders>> | null>(null);
  const [error, setError] = useState<AdminApiError | Error | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [s, o] = await Promise.all([fetchStats(), listOrders({ limit: 6 })]);
      setStats(s);
      setOrders(o);
    } catch (e) {
      setError(e as AdminApiError | Error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const bars = useMemo(() => {
    if (!stats) return [];
    const src =
      stats.revenueByGateway.length > 0 ? stats.revenueByGateway : stats.ordersByStatus;
    if (src.length === 0) return [];
    const max = Math.max(
      ...src.map((x) =>
        "revenueMicro" in x ? Number(x.revenueMicro) : Number(x.count)
      )
    );
    return src.map((x, i) => ({
      label: ("revenueMicro" in x ? x.gateway : x.status) as string,
      value: ("revenueMicro" in x ? x.revenueMicro : x.count) as number,
      height: max > 0 ? Math.max(8, Math.round((("revenueMicro" in x ? x.revenueMicro : x.count) / max) * 100)) : 8,
      isMicro: "revenueMicro" in x
    }));
  }, [stats]);

  const kpis = [
    {
      label: t("kpi.todayOrders"),
      value: stats ? String(stats.todayOrders) : "—"
    },
    {
      label: t("kpi.todayRevenue"),
      value: stats ? formatMicro(stats.todayRevenueMicro, locale, "CNY") : "—"
    },
    {
      label: t("kpi.pendingShip"),
      value: stats ? String(stats.pendingShip) : "—"
    },
    {
      label: t("kpi.availableKeys"),
      value: stats ? String(stats.availableCardKeys) : "—"
    },
    {
      label: t("kpi.unresolvedFraud"),
      value: stats ? String(stats.unresolvedFraud) : "—"
    }
  ];

  return (
    <div>
      {error ? (
        <ErrorBox error={error} onRetry={() => void load()} />
      ) : (
        <>
          <div className="grid grid-cols-2 gap-3.5 lg:grid-cols-5">
            {kpis.map((k) => (
              <div
                key={k.label}
                className={`${loading ? "forge-skeleton" : "forge-fade-up"} rounded-2xl border border-zinc-800/80 bg-zinc-900/50 p-4 backdrop-blur-sm transition hover:border-indigo-500/30`}
              >
                {loading ? (
                  <div className="space-y-2">
                    <div className="h-3 w-16 rounded bg-zinc-800" />
                    <div className="h-6 w-20 rounded bg-zinc-800" />
                  </div>
                ) : (
                  <>
                    <div className="text-[11px] text-zinc-500">{k.label}</div>
                    <div className="mt-1.5 font-mono text-xl font-bold tracking-tight text-zinc-100">
                      {k.value}
                    </div>
                  </>
                )}
              </div>
            ))}
          </div>

          <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-2">
            <Card
              title={t("revenueTrend")}
              actions={
                <span className="text-[11px] text-zinc-600">
                  {stats && stats.revenueByGateway.length === 0 && t("placeholderNote")}
                </span>
              }
            >
              <div className="p-5">
                {loading ? (
                  <div className="forge-skeleton h-32 rounded-lg" />
                ) : bars.length === 0 ? (
                  <EmptyState>{t("noData")}</EmptyState>
                ) : (
                  <div className="flex h-36 items-end gap-3">
                    {bars.map((b, i) => (
                      <div key={i} className="flex flex-1 flex-col">
                        <div
                          className="rounded-md bg-gradient-to-t from-indigo-600/40 to-purple-500/60 transition hover:from-indigo-500/60 hover:to-purple-400/80"
                          style={{ height: `${b.height}%` }}
                        />
                        <div className="mt-1.5 truncate text-center text-[10px] text-zinc-600">
                          {b.label}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </Card>

            <Card
              title={t("recent")}
              actions={
                <Link
                  href={`/${locale}/admin/orders`}
                  className="text-[11px] text-indigo-400 transition hover:text-indigo-300"
                >
                  {t("viewAll")}
                </Link>
              }
            >
              {loading ? (
                <Skeleton rows={5} />
              ) : orders && orders.length > 0 ? (
                <div className="overflow-x-auto">
                  <table className="w-full text-left text-xs">
                    <thead>
                      <tr className="text-[11px] uppercase tracking-wider text-zinc-600">
                        <th className="px-5 py-2.5 font-medium">{t("th.orderNo")}</th>
                        <th className="px-3 py-2.5 font-medium">{t("th.product")}</th>
                        <th className="px-3 py-2.5 font-medium">{t("th.amount")}</th>
                        <th className="px-3 py-2.5 font-medium">{t("th.status")}</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-zinc-800/60">
                      {orders.map((o) => (
                        <tr key={o.outTradeNo} className="transition hover:bg-zinc-800/30">
                          <td className="px-5 py-3 font-mono text-[11px] text-zinc-400">
                            {o.outTradeNo}
                          </td>
                          <td className="max-w-[160px] truncate px-3 py-3 text-zinc-300">
                            {o.productTitle}
                          </td>
                          <td className="px-3 py-3 font-mono text-zinc-300">
                            {formatMicro(o.expectedAmountMicro, locale, o.currency)}
                          </td>
                          <td className="px-3 py-3">
                            <StatusBadge status={o.status} />
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <EmptyState>{t("noOrders")}</EmptyState>
              )}
            </Card>
          </div>

          <div className="mt-4 flex justify-end">
            <button type="button" onClick={() => void load()} className={BTN}>
              {t("refresh")}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
