"use client";

import { useEffect, useState } from "react";
import { useTranslations, useLocale } from "next-intl";
import Link from "next/link";
import { fetchDeliveries, markDownloaded, type Delivery } from "@/lib/membership-api";

const TYPE_ICON: Record<Delivery["deliveryType"], string> = {
  file: "▣",
  card_key: "◇",
  content: "▶",
  license: "§"
};

export function DeliveriesView() {
  const locale = useLocale();
  const t = useTranslations("deliveries");
  const [list, setList] = useState<Delivery[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<number | null>(null);
  const [unauthorized, setUnauthorized] = useState(false);

  const load = () => {
    setErr(null);
    setUnauthorized(false);
    fetchDeliveries()
      .then(setList)
      .catch((e) => {
        if ((e as { status?: number })?.status === 401) {
          setUnauthorized(true);
        } else {
          setErr(String(e?.message ?? e));
        }
      });
  };

  useEffect(load, []);

  const download = async (d: Delivery) => {
    setBusy(d.id);
    try {
      await markDownloaded(d.id);
      // 文件/内容类: 走真实下载端点 /api-user/files/:id (服务端代理注入会话 token,
      // 后端校验属主并流式返回), 而非把 storage_key (local:// / s3://) 当 URL 打开.
      if (d.deliveryType !== "card_key") {
        window.open(`/api-user/files/${d.id}`, "_blank");
      }
      load();
    } catch (e) {
      setErr(String((e as { message?: string })?.message ?? e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div>
      <header className="mb-6">
        <h1 className="text-2xl font-bold tracking-tight text-zinc-100">
          我的文件
          <span className="ml-3 bg-gradient-to-r from-indigo-400 to-purple-400 bg-clip-text text-xs font-medium text-transparent">
            Deliveries
          </span>
        </h1>
        <p className="mt-1 text-sm text-zinc-500">{t("subtitle")}</p>
      </header>

      {err ? (
        <p className="mb-4 rounded-lg border border-rose-500/30 bg-rose-500/15 px-4 py-3 text-sm text-rose-400">
          {err}
          <button onClick={load} className="ml-3 text-indigo-400 hover:text-indigo-300">
            {t("retry")}
          </button>
        </p>
      ) : null}

      {unauthorized ? (
        <div className="rounded-2xl border border-zinc-800/80 bg-zinc-900/40 py-16 text-center text-sm text-zinc-400">
          {t("loginToView")}
          <div className="mt-3">
            <Link
              href={`/${locale}/auth`}
              className="text-indigo-400 hover:text-indigo-300"
            >
              {t("login")} →
            </Link>
          </div>
        </div>
      ) : !list ? (
        <div className="space-y-3">
          {[0, 1, 2].map((i) => (
            <div key={i} className="forge-skeleton h-16 rounded-xl" />
          ))}
        </div>
      ) : list.length === 0 ? (
        <div className="rounded-2xl border border-zinc-800/80 bg-zinc-900/40 py-16 text-center text-sm text-zinc-500">
          {t("empty")}
          <div className="mt-3">
            <Link
              href={`/${locale}`}
              className="text-indigo-400 hover:text-indigo-300"
            >
              {t("goStore")} →
            </Link>
          </div>
        </div>
      ) : (
        <div className="space-y-3">
          {list.map((d) => (
            <div
              key={d.id}
              className="forge-fade-up flex items-center justify-between rounded-xl border border-zinc-800/80 bg-zinc-900/50 p-4 backdrop-blur-sm"
            >
              <div className="flex items-center gap-3">
                <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border border-zinc-700/80 bg-gradient-to-br from-zinc-800 to-zinc-900 font-mono text-indigo-300">
                  {TYPE_ICON[d.deliveryType]}
                </div>
                <div className="min-w-0">
                  <div className="truncate text-sm font-semibold text-zinc-100">
                    {d.productTitle}
                  </div>
                  <div className="mt-0.5 flex items-center gap-2 text-[11px] text-zinc-500">
                    <span className="rounded bg-zinc-800 px-1.5 py-0.5 uppercase">
                      {d.productCategory}
                    </span>
                    <span>{d.createdAt}</span>
                    {d.cardKey ? (
                      <span className="font-mono text-emerald-400">
                        {d.cardKey.slice(0, 8)}…
                      </span>
                    ) : null}
                    {d.downloadCount > 0 ? (
                      <span>{t("downloaded")}: {d.downloadCount}</span>
                    ) : null}
                  </div>
                </div>
              </div>
              <button
                onClick={() => download(d)}
                disabled={busy === d.id}
                className="rounded-lg border border-indigo-500/40 bg-indigo-500/12 px-3.5 py-2 text-xs font-medium text-indigo-300 transition hover:bg-indigo-500/20 disabled:opacity-50"
              >
                {busy === d.id ? t("downloading") : t("download")}
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
