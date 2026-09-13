"use client";

import { useCallback, useEffect, useState } from "react";
import { useLocale, useTranslations } from "next-intl";
import { fetchStats, AdminApiError } from "@/lib/admin-api";
import { formatMicro } from "@/lib/amount";
import { Card, Skeleton, ErrorBox, EmptyState, BTN } from "./ui";

type Stats = Awaited<ReturnType<typeof fetchStats>>;

export function StatsView() {
  const locale = useLocale();
  const t = useTranslations("admin");
  const [stats, setStats] = useState<Stats | null>(null);
  const [error, setError] = useState<AdminApiError | Error | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setStats(await fetchStats());
    } catch (e) {
      setError(e as AdminApiError | Error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const kpis = [
    { label: t("kpi.todayOrders"), value: stats?.todayOrders },
    {
      label: t("kpi.todayRevenue"),
      value: stats ? formatMicro(stats.todayRevenueMicro, locale, "CNY") : undefined
    },
    { label: t("kpi.pendingShip"), value: stats?.pendingShip },
    { label: t("kpi.availableKeys"), value: stats?.availableCardKeys },
    { label: t("kpi.unresolvedFraud"), value: stats?.unresolvedFraud }
  ];

  const maxStatus = Math.max(1, ...stats?.ordersByStatus.map((x) => x.count) ?? [1]);
  const maxGw = Math.max(
    1,
    ...stats?.revenueByGateway.map((x) => Number(x.revenueMicro)) ?? [1]
  );

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
                      {k.value ?? "—"}
                    </div>
                  </>
                )}
              </div>
            ))}
          </div>

          <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-2">
            <Card title={t("ordersByStatus")}>
              {loading ? (
                <Skeleton rows={4} />
              ) : stats && stats.ordersByStatus.length > 0 ? (
                <div className="space-y-3 p-5">
                  {stats.ordersByStatus.map((s) => (
                    <div key={s.status}>
                      <div className="mb-1 flex items-center justify-between text-xs">
                        <span className="font-mono text-zinc-400">{s.status}</span>
                        <span className="font-mono text-zinc-300">{s.count}</span>
                      </div>
                      <div className="h-2 overflow-hidden rounded-full bg-zinc-800/80">
                        <div
                          className="h-full rounded-full bg-gradient-to-r from-indigo-500 to-purple-500 shadow-[0_0_10px_rgba(99,102,241,0.5)]"
                          style={{ width: `${Math.max(4, (s.count / maxStatus) * 100)}%` }}
                        />
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <EmptyState>{t("noData")}</EmptyState>
              )}
            </Card>

            <Card title={t("revenueByGateway")}>
              {loading ? (
                <Skeleton rows={4} />
              ) : stats && stats.revenueByGateway.length > 0 ? (
                <div className="p-5">
                  <div className="flex h-40 items-end gap-4">
                    {stats.revenueByGateway.map((g) => (
                      <div key={g.gateway} className="flex flex-1 flex-col">
                        <div
                          className="rounded-md bg-gradient-to-t from-indigo-600/40 to-purple-500/60 transition hover:from-indigo-500/60 hover:to-purple-400/80"
                          style={{
                            height: `${Math.max(8, (Number(g.revenueMicro) / maxGw) * 100)}%`
                          }}
                        />
                        <div className="mt-1.5 truncate text-center text-[10px] text-zinc-600">
                          {g.gateway}
                        </div>
                        <div className="text-center font-mono text-[10px] text-zinc-500">
                          {formatMicro(g.revenueMicro, locale, "CNY")}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              ) : (
                <EmptyState>{t("noData")}</EmptyState>
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
