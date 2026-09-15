"use client";

import { useMemo, useState, useActionState, useEffect } from "react";
import { useTranslations } from "next-intl";
import { createOrder, type CheckoutState } from "@/app/actions/checkout";
import { formatMicro, lockedUsd, FX_METHODS, type PaymentMethod } from "@/lib/amount";
import type { OrderSummary } from "@/lib/api";
import type { Product } from "@/lib/products";
import { fetchProfile, type UserTier } from "@/lib/membership-api";

const METHODS: { key: PaymentMethod; icon: string }[] = [
  { key: "alipay", icon: "¥" },
  { key: "wechat", icon: "✆" },
  { key: "stripe", icon: "S" },
  { key: "card", icon: "▤" },
  { key: "paypal", icon: "P" },
  { key: "usdt", icon: "₮" }
];

const ITEM_ICONS: Record<Product["category"], string> = {
  source: "{ }",
  tutorial: "▶",
  card: "◇",
  resource: "▣"
};

const STEPS = ["选商品", "选支付方式", "完成支付"] as const;

export function CheckoutForm({
  order,
  locale,
  item
}: {
  order: OrderSummary;
  locale: string;
  item?: Product;
}) {
  const t = useTranslations("checkout");
  const [method, setMethod] = useState<PaymentMethod>("alipay");
  const [state, formAction, isPending] = useActionState<CheckoutState | null, FormData>(
    createOrder,
    null
  );
  // 会员折扣: 拉取当前用户等级, 展示 discountPct 权益
  const [member, setMember] = useState<UserTier | null>(null);
  useEffect(() => {
    fetchProfile()
      .then(setMember)
      .catch(() => setMember(null));
  }, []);

  const currency = FX_METHODS.includes(method) ? "USD" : order.currency;
  const totalDisplay = useMemo(
    () => formatMicro(order.amountMicro, locale, currency),
    [order.amountMicro, locale, currency]
  );
  // 会员折扣后金额: 整数微单位 * (100 - pct) / 100, 无浮点
  const discountedMicro = useMemo(() => {
    const pct = member?.discountPct ?? 0;
    if (pct <= 0 || pct >= 100) return order.amountMicro;
    return Math.round((order.amountMicro * (100 - pct)) / 100);
  }, [order.amountMicro, member]);
  const discountedDisplay = useMemo(
    () => formatMicro(discountedMicro, locale, currency),
    [discountedMicro, locale, currency]
  );
  const hasDiscount = (member?.discountPct ?? 0) > 0;
  const rateDisplay = useMemo(
    () =>
      order.lockedRateUsd
        ? lockedUsd(order.lockedRateUsd, order.amountMicro, locale)
        : null,
    [order.lockedRateUsd, order.amountMicro, locale]
  );

  const submit = (fd: FormData) => formAction(fd);

  const activeStep = state?.ok ? 2 : method === "alipay" ? 2 : 1;

  return (
    <section className="forge-fade-up relative overflow-hidden rounded-2xl border border-zinc-800/80 bg-zinc-900/50 p-6 backdrop-blur-md">
      <div className="pointer-events-none absolute inset-0 bg-gradient-to-br from-indigo-500/5 via-transparent to-purple-500/5" />
      <div className="relative">
        {/* 步骤条 */}
        <ol className="mb-5 flex items-center gap-1.5 text-[11px]">
          {STEPS.map((s, i) => (
            <li key={s} className="flex items-center gap-1.5">
              <span
                className={
                  "flex h-5 w-5 items-center justify-center rounded-full border text-[10px] font-bold " +
                  (i < activeStep
                    ? "border-emerald-500/50 bg-emerald-500/15 text-emerald-400"
                    : i === activeStep
                      ? "border-indigo-500/60 bg-indigo-500/15 text-indigo-300"
                      : "border-zinc-700 text-zinc-600")
                }
              >
                {i + 1}
              </span>
              <span className={i <= activeStep ? "text-zinc-300" : "text-zinc-600"}>{s}</span>
              {i < STEPS.length - 1 ? <span className="mx-0.5 h-px w-4 bg-zinc-700" /> : null}
            </li>
          ))}
        </ol>

        <h1 className="text-lg font-semibold tracking-tight text-zinc-100">{t("title")}</h1>

        {item ? (
          <div className="mt-4 rounded-xl border border-zinc-800/80 bg-zinc-950/50 p-4">
            <div className="flex items-center gap-3">
              <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl border border-zinc-700/80 bg-gradient-to-br from-zinc-800 to-zinc-900 text-lg font-mono text-indigo-300">
                {ITEM_ICONS[item.category]}
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-[11px] uppercase tracking-wider text-zinc-500">
                  {item.categoryLabel}
                </div>
                <div className="truncate text-sm font-semibold text-zinc-100">{item.name}</div>
                <div className="mt-0.5 truncate text-xs text-zinc-500">{item.desc}</div>
              </div>
            </div>
          </div>
        ) : null}

        <dl className="mt-4 space-y-2.5 text-sm">
          <div className="flex justify-between">
            <dt className="text-zinc-500">{t("orderNo")}</dt>
            <dd className="font-mono text-xs text-zinc-300">{order.outTradeNo}</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-zinc-500">{t("quantity")}</dt>
            <dd className="text-zinc-300">{order.quantity}</dd>
          </div>
          <div className="flex items-baseline justify-between border-t border-zinc-800/80 pt-2.5">
            <dt className="text-sm text-zinc-400">{t("total")}</dt>
            <dd className="text-xl font-bold tracking-tight text-zinc-100">{totalDisplay}</dd>
          </div>
          {hasDiscount ? (
            <div className="flex items-baseline justify-between text-xs">
              <dt className="text-indigo-400">
                {t("memberDiscount")} ({member!.tierCode} · {member!.discountPct}%)
              </dt>
              <dd className="font-mono font-semibold text-emerald-400">{discountedDisplay}</dd>
            </div>
          ) : null}
          {rateDisplay ? (
            <div className="flex justify-between text-xs text-amber-500/80">
              <dt>{t("lockedRate")}</dt>
              <dd className="font-mono">{rateDisplay}</dd>
            </div>
          ) : null}
        </dl>

        <fieldset className="mt-5">
          <legend className="text-xs font-medium uppercase tracking-wider text-zinc-500">
            {t("payMethod")}
          </legend>
          <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-3">
            {METHODS.map((m) => (
              <button
                key={m.key}
                type="button"
                onClick={() => setMethod(m.key)}
                className={
                  "flex items-center gap-2.5 rounded-xl border px-3 py-2.5 text-left text-xs transition " +
                  (method === m.key
                    ? "border-indigo-500/60 bg-indigo-500/12 font-medium text-indigo-300 shadow-[0_0_14px_rgba(99,102,241,0.22)]"
                    : "border-zinc-800/80 bg-zinc-950/40 text-zinc-400 hover:border-zinc-700")
                }
              >
                <span
                  className={
                    "flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-xs font-mono " +
                    (method === m.key
                      ? "bg-indigo-500/20 text-indigo-300"
                      : "bg-zinc-800/80 text-zinc-500")
                  }
                >
                  {m.icon}
                </span>
                <span className="truncate">{t(`method.${m.key}`)}</span>
              </button>
            ))}
          </div>
        </fieldset>

        <form
          onSubmit={(e) => {
            e.preventDefault();
            submit(new FormData(e.currentTarget));
          }}
          className="mt-5 space-y-3"
        >
          <input type="hidden" name="out_trade_no" value={order.outTradeNo} />
          <input type="hidden" name="payment_method" value={method} />
          <button
            type="submit"
            disabled={isPending}
            className="w-full rounded-xl bg-gradient-to-r from-indigo-600 via-purple-600 to-indigo-600 px-4 py-3 text-sm font-semibold text-white shadow-[0_0_24px_rgba(99,102,241,0.35)] transition hover:shadow-[0_0_32px_rgba(99,102,241,0.5)] disabled:opacity-50 disabled:shadow-none"
          >
            {isPending ? t("submitting") : t("payNow")}
          </button>
        </form>

        {state?.ok ? (
          <p className="mt-3 rounded-lg border border-emerald-500/30 bg-emerald-500/12 px-3 py-2.5 text-xs text-emerald-400">
            {t("created")}
          </p>
        ) : null}
        {state?.error ? (
          <p className="mt-3 rounded-lg border border-rose-500/30 bg-rose-500/12 px-3 py-2.5 text-xs text-rose-400">
            {t("error")}
          </p>
        ) : null}

        <p className="mt-4 text-[11px] leading-relaxed text-zinc-600">{t("securityNote")}</p>
      </div>
    </section>
  );
}
