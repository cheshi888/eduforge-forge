import type { OrderSummary } from "./api";

export type PaymentMethod = "alipay" | "stripe" | "usdt" | "wechat" | "paypal" | "card";

// 跨境收单通道 (固定美元结算)
export const FX_METHODS: PaymentMethod[] = ["usdt", "paypal", "card"];

const LOCALE_TAGS: Record<string, string> = {
  zh: "zh-CN",
  en: "en-US",
  ja: "ja-JP",
  ko: "ko-KR"
};

// M2-05: 金额换算全程整数/BigInt 运算, 禁用 f64 参与金额计算.
// NUMERIC(20,6) 微单位 (int64) -> 展示层四舍五入到 2 位小数.
// 仅在交给 Intl 渲染前把"整数部分 + 两位小数"拼成 number, 该 number 只做展示, 不再参与金额运算.

// 微单位 -> { 整数部分(含符号), 两位小数(四舍五入) }, 纯 bigint 运算
function round2(micro: bigint): { whole: bigint; frac2: bigint } {
  const neg = micro < 0n;
  const abs = neg ? -micro : micro;
  let whole = abs / 1_000_000n;
  // 6 位微单位小数 → 2 位小数(分): (frac6 + 5000) / 10000 即为四舍五入到分。
  // 修复: 原实现误乘 100n, 导致 frac2 被放大 100 倍, 金额显示错误 (如 199.99 → 298)。
  let frac2 = ((abs % 1_000_000n) + 5_000n) / 10_000n;
  if (frac2 >= 100n) {
    whole += 1n;
    frac2 -= 100n;
  }
  return { whole: neg ? -whole : whole, frac2 };
}

// 把 整数部分+两位小数 渲染为货币展示串. 内部只做一次 展示用 number 拼装, 不做金额运算.
function toDisplayNumber(whole: bigint, frac2: bigint): number {
  const sign = whole < 0n ? -1 : 1;
  const absWhole = whole < 0n ? -whole : whole;
  return sign * (Number(absWhole) + Number(frac2) / 100);
}

// 内部金额统一 6 位小数微单位 (micro-unit), 展示层由 Intl 四舍五入到 2 位:
// zh -> ￥1,000.00 / en -> $1,000.00, 依用户 locale 自动适配。
export function formatMicro(micro: number, locale: string, currency: string): string {
  const localeTag = LOCALE_TAGS[locale] ?? locale;
  const { whole, frac2 } = round2(BigInt(Math.trunc(micro)));
  return new Intl.NumberFormat(localeTag, {
    style: "currency",
    currency,
    minimumFractionDigits: 2,
    maximumFractionDigits: 2
  }).format(toDisplayNumber(whole, frac2));
}

// NUMERIC(20,6) 十进制字符串 (如 "0.1385") -> 微单位 BigInt, 纯字符串解析
function rateToMicro(rate: string): bigint {
  const neg = rate.startsWith("-");
  const [int, frac = ""] = rate.replace("-", "").split(".");
  const padded = (frac + "000000").slice(0, 6);
  const v = BigInt(int || "0") * 1_000_000n + BigInt(padded);
  return neg ? -v : v;
}

// 虚拟货币支付: 展示下单时刻锁定的美元锚定汇率。
// usd微单位 = price微单位 × rate微单位 / 十^6 (两次 10^-6 尺度相乘后归一)
export function lockedUsd(rate: string, micro: number, locale: string): string {
  const localeTag = LOCALE_TAGS[locale] ?? locale;
  const usdMicro = BigInt(Math.trunc(micro)) * rateToMicro(rate);
  const { whole, frac2 } = round2(usdMicro / 1_000_000n);
  const usd = new Intl.NumberFormat(localeTag, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2
  }).format(toDisplayNumber(whole, frac2));
  return `1 ${locale === "zh" ? "人民币" : "CNY"} = ${usd} (locked)`;
}

export function orderCurrency(order: OrderSummary, method: PaymentMethod): string {
  if (method === "usdt") return "USD";
  return order.currency;
}
