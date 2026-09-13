"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";

const TABS = ["概览", "订单管理", "商品管理", "卡密库", "风控规则", "设置"] as const;

const KPI = [
  { label: "今日订单", value: "128", delta: "+12%", up: true },
  { label: "今日收入", value: "¥ 46,320", delta: "+8.4%", up: true },
  { label: "待发货", value: "23", delta: "-3", up: false },
  { label: "卡密余额", value: "1,842", delta: "-156", up: false }
];

const ORDERS = [
  { no: "FT-20260911-9a2b", item: "Figma 专业版年卡密", amount: "¥120.00", method: "支付宝", status: "PAID", t: "2m ago" },
  { no: "FT-20260911-77e1", item: "Rust 系统级编程 30 讲", amount: "¥199.00", method: "微信支付", status: "PENDING", t: "8m ago" },
  { no: "FT-20260911-4c0f", item: "Spring Cloud 微服务实战源码", amount: "¥299.00", method: "Stripe", status: "PAID", t: "15m ago" },
  { no: "FT-20260911-2d8a", item: "Midjourney 月卡密", amount: "¥89.00", method: "USDT", status: "FRAUD", t: "22m ago" },
  { no: "FT-20260911-0b3c", item: "AI 提示词工程资源包", amount: "¥59.00", method: "PayPal", status: "PAID", t: "31m ago" },
  { no: "FT-20260911-6e9d", item: "Kubernetes 生产落地实战", amount: "¥329.00", method: "支付宝", status: "PAID", t: "40m ago" }
];

const STATUS_STYLE: Record<string, string> = {
  PAID: "border-emerald-500/40 bg-emerald-500/12 text-emerald-400",
  PENDING: "border-amber-500/40 bg-amber-500/12 text-amber-400",
  FRAUD: "border-rose-500/40 bg-rose-500/12 text-rose-400"
};

export function AdminDashboard() {
  const t = useTranslations("admin");
  const [tab, setTab] = useState<(typeof TABS)[number]>("概览");

  return (
    <div className="grid grid-cols-1 gap-5 lg:grid-cols-[220px_1fr]">
      {/* 侧边栏 */}
      <aside className="rounded-2xl border border-zinc-800/80 bg-zinc-900/50 p-4 backdrop-blur-sm">
        <div className="mb-3 text-[11px] font-medium uppercase tracking-wider text-zinc-600">
          模块
        </div>
        <nav className="space-y-1">
          {TABS.map((x) => (
            <button
              key={x}
              type="button"
              onClick={() => setTab(x)}
              className={
                "block w-full rounded-lg px-3 py-2 text-left text-sm transition " +
                (tab === x
                  ? "bg-indigo-500/15 font-medium text-indigo-300 shadow-[inset_0_0_0_1px_rgba(99,102,241,0.35)]"
                  : "text-zinc-500 hover:bg-zinc-800/50 hover:text-zinc-300")
              }
            >
              {x}
            </button>
          ))}
        </nav>
      </aside>

      {/* 主区 */}
      <section>
        <header className="mb-5">
          <h1 className="text-xl font-bold tracking-tight text-zinc-100">{t("title")}</h1>
          <p className="mt-0.5 text-xs text-zinc-500">{t("subtitle")}</p>
        </header>

        <div className="grid grid-cols-2 gap-3.5 lg:grid-cols-4">
          {KPI.map((s) => (
            <div
              key={s.label}
              className="rounded-2xl border border-zinc-800/80 bg-zinc-900/50 p-4 backdrop-blur-sm transition hover:border-indigo-500/30"
            >
              <div className="text-[11px] text-zinc-500">{s.label}</div>
              <div className="mt-1.5 text-xl font-bold tracking-tight text-zinc-100">{s.value}</div>
              <div className={`mt-1 text-[11px] ${s.up ? "text-emerald-400" : "text-rose-400"}`}>
                {s.up ? "▲" : "▼"} {s.delta}
              </div>
            </div>
          ))}
        </div>

        {/* 图表占位 */}
        <div className="mt-4 rounded-2xl border border-zinc-800/80 bg-zinc-900/50 p-5 backdrop-blur-sm">
          <div className="mb-3 flex items-center justify-between">
            <span className="text-sm font-semibold text-zinc-200">收入趋势</span>
            <span className="text-[11px] text-zinc-600">近 7 天</span>
          </div>
          <div className="flex h-32 items-end gap-2">
            {[40, 65, 50, 80, 70, 95, 88].map((h, i) => (
              <div key={i} className="flex-1">
                <div
                  className="rounded-md bg-gradient-to-t from-indigo-600/40 to-purple-500/60 transition hover:from-indigo-500/60 hover:to-purple-400/80"
                  style={{ height: `${h}%` }}
                />
                <div className="mt-1 text-center text-[10px] text-zinc-600">
                  {["一", "二", "三", "四", "五", "六", "日"][i]}
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* 订单表 */}
        <div className="mt-4 rounded-2xl border border-zinc-800/80 bg-zinc-900/50 backdrop-blur-sm">
          <div className="flex items-center justify-between border-b border-zinc-800/80 px-5 py-3.5">
            <span className="text-sm font-semibold text-zinc-200">{t("recent")}</span>
            <span className="text-[11px] text-zinc-600">共 {ORDERS.length} 条</span>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead>
                <tr className="text-[11px] uppercase tracking-wider text-zinc-600">
                  <th className="px-5 py-2.5 font-medium">单号</th>
                  <th className="px-3 py-2.5 font-medium">商品</th>
                  <th className="px-3 py-2.5 font-medium">金额</th>
                  <th className="px-3 py-2.5 font-medium">渠道</th>
                  <th className="px-3 py-2.5 font-medium">状态</th>
                  <th className="px-5 py-2.5 font-medium">时间</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-800/60">
                {ORDERS.map((r) => (
                  <tr key={r.no} className="transition hover:bg-zinc-800/30">
                    <td className="px-5 py-3 font-mono text-[11px] text-zinc-400">{r.no}</td>
                    <td className="max-w-[200px] truncate px-3 py-3 text-zinc-300">{r.item}</td>
                    <td className="px-3 py-3 text-zinc-300">{r.amount}</td>
                    <td className="px-3 py-3 text-zinc-500">{r.method}</td>
                    <td className="px-3 py-3">
                      <span
                        className={`rounded-md border px-2 py-0.5 text-[11px] font-medium ${STATUS_STYLE[r.status]}`}
                      >
                        {r.status}
                      </span>
                    </td>
                    <td className="px-5 py-3 text-zinc-600">{r.t}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </section>
    </div>
  );
}
