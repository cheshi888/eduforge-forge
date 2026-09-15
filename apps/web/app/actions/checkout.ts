"use server";

import { resolveUid } from "@/lib/api";

// 结算表单 Server Action: 组装下单请求, 由 Rust Core 统一落库
// (expected_amount 在此刻锁定, 后续网关回调只做本地比对)。
const API_BASE = "/api";

export interface CheckoutState {
  ok: boolean;
  outTradeNo?: string;
  gatewayPayload?: string;
  error?: string;
}

export async function createOrder(
  _prev: CheckoutState | null,
  formData: FormData
): Promise<CheckoutState> {
  const outTradeNo = String(formData.get("out_trade_no") ?? "");
  // 前端 UI 值与 rust GatewayType 词表 (alipay/stripe/coinbase/wechat/paypal/cardacq) 不一致,
  // 下发前统一映射, 避免 400 unknown payment method: card→cardacq, usdt→coinbase
  const rawMethod = String(formData.get("payment_method") ?? "alipay");
  const UI_TO_GATEWAY: Record<string, string> = { card: "cardacq", usdt: "coinbase" };
  const paymentMethod = UI_TO_GATEWAY[rawMethod] ?? rawMethod;
  if (!outTradeNo) {
    return { ok: false, error: "missing out_trade_no" };
  }

  try {
    const res = await fetch(`${API_BASE}/internal/checkout`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        uid: resolveUid(),
        out_trade_no: outTradeNo,
        payment_method: paymentMethod
      }),
      cache: "no-store"
    });
    if (!res.ok) {
      return { ok: false, error: `gateway checkout failed: ${res.status}` };
    }
    const data = (await res.json()) as {
      out_trade_no: string;
      gateway_payload: string;
    };
    return { ok: true, outTradeNo: data.out_trade_no, gatewayPayload: data.gateway_payload };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}
