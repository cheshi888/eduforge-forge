"use client";

import { useCallback, useEffect, useState } from "react";
import { useLocale, useTranslations } from "next-intl";
import {
  listFraud,
  resolveFraud,
  AdminApiError,
  type FraudEvent
} from "@/lib/admin-api";
import { formatMicro } from "@/lib/amount";
import { Card, Skeleton, ErrorBox, EmptyState, GHOST } from "./ui";

export function FraudView() {
  const locale = useLocale();
  const t = useTranslations("admin");
  const [events, setEvents] = useState<FraudEvent[] | null>(null);
  const [error, setError] = useState<AdminApiError | Error | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<number | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setEvents(await listFraud());
    } catch (e) {
      setError(e as AdminApiError | Error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const resolve = async (ev: FraudEvent) => {
    setBusy(ev.id);
    setMsg(null);
    try {
      await resolveFraud(ev.id);
      setMsg(t("resolvedOk"));
      await load();
    } catch (e) {
      setMsg((e as AdminApiError | Error).message);
    } finally {
      setBusy(null);
    }
  };

  const unresolved = (events ?? []).filter((e) => !e.resolved).length;

  return (
    <div>
      {error ? (
        <ErrorBox error={error} onRetry={() => void load()} />
      ) : (
        <Card
          title={t("fraudTitle")}
          actions={
            <span className="text-[11px] text-zinc-500">
              {t("unresolvedCount", { n: unresolved })}
            </span>
          }
        >
          {loading ? (
            <Skeleton rows={6} />
          ) : events && events.length > 0 ? (
            <>
              <div className="overflow-x-auto">
                <table className="w-full text-left text-xs">
                  <thead>
                    <tr className="text-[11px] uppercase tracking-wider text-zinc-600">
                      <th className="px-5 py-2.5 font-medium">{t("th.orderNo")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.reason")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.received")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.expected")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.gateway")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.resolved")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.time")}</th>
                      <th className="px-5 py-2.5 text-right font-medium">{t("actions")}</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-zinc-800/60">
                    {events.map((ev) => (
                      <tr
                        key={ev.id}
                        className={`transition ${
                          ev.resolved
                            ? "opacity-60"
                            : "bg-rose-500/[0.04] hover:bg-rose-500/10"
                        }`}
                      >
                        <td className="px-5 py-3 font-mono text-[11px] text-zinc-400">
                          {ev.outTradeNo}
                        </td>
                        <td className="px-3 py-3">
                          <span
                            className={`rounded-md border px-2 py-0.5 text-[11px] font-medium ${
                              ev.resolved
                                ? "border-zinc-700/70 bg-zinc-700/20 text-zinc-400"
                                : "border-rose-500/40 bg-rose-500/12 text-rose-400"
                            }`}
                          >
                            {t(`reason.${ev.reason}`, {
                              defaultValue: ev.reason
                            })}
                          </span>
                        </td>
                        <td className="px-3 py-3 font-mono text-zinc-300">
                          {ev.receivedAmountMicro !== null && ev.currency
                            ? formatMicro(ev.receivedAmountMicro, locale, ev.currency)
                            : "—"}
                        </td>
                        <td className="px-3 py-3 font-mono text-zinc-500">
                          {ev.expectedAmountMicro !== null && ev.currency
                            ? formatMicro(ev.expectedAmountMicro, locale, ev.currency)
                            : "—"}
                        </td>
                        <td className="px-3 py-3 text-zinc-500">
                          {ev.gatewayType ?? "—"}
                        </td>
                        <td className="px-3 py-3">
                          {ev.resolved ? (
                            <span className="text-[11px] text-zinc-500">
                              {t("resolvedLabel", { by: ev.resolvedBy ?? "admin" })}
                            </span>
                          ) : (
                            <span className="font-mono text-[11px] text-rose-400">
                              {t("open")}
                            </span>
                          )}
                        </td>
                        <td className="px-3 py-3 text-zinc-600">
                          {new Date(ev.createdAt).toLocaleString(locale)}
                        </td>
                        <td className="px-5 py-3 text-right">
                          {!ev.resolved && (
                            <button
                              type="button"
                              disabled={busy !== null}
                              onClick={() => void resolve(ev)}
                              className={GHOST}
                            >
                              {t("resolve")}
                            </button>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {msg && (
                <div
                  className={`border-t border-zinc-800/80 px-5 py-3 text-[11px] ${
                    msg === t("resolvedOk") ? "text-emerald-400" : "text-rose-400"
                  }`}
                >
                  {msg}
                </div>
              )}
            </>
          ) : (
            <EmptyState>{t("noFraud")}</EmptyState>
          )}
        </Card>
      )}
    </div>
  );
}
