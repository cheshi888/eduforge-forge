"use client";

import type { AdminApiError } from "@/lib/admin-api";
import type { ReactNode } from "react";

export const CARD =
  "rounded-2xl border border-zinc-800/80 bg-zinc-900/50 backdrop-blur-sm";

export const FIELD =
  "rounded-lg border border-zinc-700/80 bg-zinc-950/60 px-3 py-2 text-sm text-zinc-200 placeholder:text-zinc-600 focus:border-indigo-500/60 focus:outline-none focus:ring-1 focus:ring-indigo-500/40";

export const BTN =
  "inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/50 bg-gradient-to-r from-indigo-500/25 to-purple-500/25 px-3 py-2 text-xs font-semibold text-indigo-200 transition hover:from-indigo-500/40 hover:to-purple-500/40 hover:shadow-[0_0_16px_rgba(99,102,241,0.35)] disabled:cursor-not-allowed disabled:opacity-50";

export const GHOST =
  "inline-flex items-center rounded-md border border-zinc-700/70 bg-zinc-900/50 px-2 py-1 text-[11px] font-medium text-zinc-400 transition hover:border-indigo-500/50 hover:text-indigo-300 disabled:cursor-not-allowed disabled:opacity-40";

const STATUS_STYLE: Record<string, string> = {
  PENDING: "border-amber-500/40 bg-amber-500/12 text-amber-400",
  PAID: "border-emerald-500/40 bg-emerald-500/12 text-emerald-400",
  SHIPPED: "border-indigo-500/40 bg-indigo-500/12 text-indigo-300",
  CANCELLED: "border-zinc-600/50 bg-zinc-600/12 text-zinc-500",
  FAILED: "border-rose-500/40 bg-rose-500/12 text-rose-400",
  FRAUD: "border-rose-500/40 bg-rose-500/12 text-rose-400",
  AVAILABLE: "border-emerald-500/40 bg-emerald-500/12 text-emerald-400",
  LOCKED: "border-amber-500/40 bg-amber-500/12 text-amber-400",
  USED: "border-sky-500/40 bg-sky-500/12 text-sky-400",
  REVERSED: "border-rose-500/40 bg-rose-500/12 text-rose-400"
};

export function StatusBadge({ status }: { status: string }) {
  const cls = STATUS_STYLE[status] ?? "border-zinc-700/70 bg-zinc-700/20 text-zinc-400";
  return (
    <span className={`rounded-md border px-2 py-0.5 text-[11px] font-medium ${cls}`}>
      {status}
    </span>
  );
}

export function Skeleton({ rows = 6 }: { rows?: number }) {
  return (
    <div className="space-y-2.5 p-5">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="forge-skeleton h-9 rounded-lg" />
      ))}
    </div>
  );
}

export function ErrorBox({
  error,
  onRetry
}: {
  error: AdminApiError | Error | null;
  onRetry?: () => void;
}) {
  if (!error) return null;
  return (
    <div
      role="alert"
      className="rounded-2xl border border-rose-500/40 bg-rose-500/10 p-5 text-sm text-rose-300"
    >
      <div className="font-semibold">加载失败</div>
      <div className="mt-1 font-mono text-xs text-rose-400/80">
        {error.message}
      </div>
      {onRetry && (
        <button
          type="button"
          onClick={onRetry}
          className="mt-3 rounded-lg border border-rose-500/40 bg-rose-500/15 px-3 py-1.5 text-xs text-rose-300 transition hover:bg-rose-500/25"
        >
          重试
        </button>
      )}
    </div>
  );
}

export function EmptyState({ children }: { children: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-2 px-5 py-12 text-sm text-zinc-600">
      <div className="font-mono text-xs text-zinc-700">∅</div>
      {children}
    </div>
  );
}

export function Card({
  title,
  actions,
  children,
  className = ""
}: {
  title?: string;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={`${CARD} forge-fade-up ${className}`}>
      {title && (
        <div className="flex items-center justify-between border-b border-zinc-800/80 px-5 py-3.5">
          <span className="text-sm font-semibold text-zinc-200">{title}</span>
          {actions}
        </div>
      )}
      {children}
    </div>
  );
}
