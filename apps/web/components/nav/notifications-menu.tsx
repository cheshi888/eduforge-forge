"use client";

import { useEffect, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import {
  fetchNotifications,
  markAllNotificationsRead,
  type Notification
} from "@/lib/community";

export function NotificationsMenu() {
  const t = useTranslations("nav");
  const [open, setOpen] = useState(false);
  const [items, setItems] = useState<Notification[]>([]);
  const [loading, setLoading] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const unread = items.filter((n) => !n.read).length;

  const load = async () => {
    setLoading(true);
    try {
      setItems(await fetchNotifications());
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (open) load();
  }, [open]);

  useEffect(() => {
    const onClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onClick);
    return () => document.removeEventListener("mousedown", onClick);
  }, []);

  const markAll = async () => {
    await markAllNotificationsRead();
    setItems((prev) => prev.map((n) => ({ ...n, read: true })));
  };

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        className="relative hidden rounded-lg border border-zinc-700/80 bg-zinc-900/50 px-3 py-1.5 text-xs font-medium text-zinc-300 transition hover:border-indigo-500/50 hover:text-indigo-300 sm:block"
        aria-label={t("notifications")}
      >
        {t("notifications")}
        {unread > 0 ? (
          <span className="absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full bg-rose-500 px-1 text-[10px] font-bold text-white">
            {unread}
          </span>
        ) : null}
      </button>

      {open ? (
        <div className="absolute right-0 top-11 z-30 w-80 overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900/95 shadow-2xl backdrop-blur-xl">
          <div className="flex items-center justify-between border-b border-zinc-800 px-4 py-3">
            <span className="text-sm font-semibold text-zinc-100">
              {t("notifications")}
            </span>
            {unread > 0 ? (
              <button
                onClick={markAll}
                className="text-xs text-indigo-400 transition hover:text-indigo-300"
              >
                {t("markAllRead")}
              </button>
            ) : null}
          </div>
          <div className="max-h-80 overflow-y-auto">
            {loading ? (
              <p className="px-4 py-8 text-center text-xs text-zinc-600">{t("loading")}</p>
            ) : items.length === 0 ? (
              <p className="px-4 py-8 text-center text-xs text-zinc-600">{t("noNotifications")}</p>
            ) : (
              items.map((n) => (
                <div
                  key={n.id}
                  className={
                    "border-b border-zinc-800/60 px-4 py-3 " +
                    (n.read ? "bg-transparent" : "bg-indigo-500/5")
                  }
                >
                  <div className="flex items-start justify-between gap-2">
                    <span className="text-xs font-medium text-zinc-200">{n.title}</span>
                    {!n.read ? (
                      <span className="mt-1 h-1.5 w-1.5 shrink-0 rounded-full bg-indigo-400" />
                    ) : null}
                  </div>
                  {n.body ? (
                    <p className="mt-1 text-xs leading-relaxed text-zinc-500">{n.body}</p>
                  ) : null}
                  <p className="mt-1 text-[10px] text-zinc-600">{n.createdAt}</p>
                </div>
              ))
            )}
          </div>
        </div>
      ) : null}
    </div>
  );
}
