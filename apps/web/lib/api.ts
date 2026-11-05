// 金额以 6 位小数微单位 (int64) 传输, 与 Rust/Go 侧对齐; 汇率以十进制字符串传输 (M2-05, 全程零浮点)
export interface OrderSummary {
  outTradeNo: string;
  quantity: number;
  amountMicro: number;
  currency: string;
  lockedRateUsd: string | null;
  gatewayPayload?: string;
  // 会员折扣 (下单时后端已应用): 折前原价微单位 + 折扣率 (0 表示无折扣)
  originalAmountMicro?: number;
  discountPct?: number;
  // 优惠券 (阶段3): 命中时返回券码与优惠金额微单位
  couponCode?: string;
  couponDiscountMicro?: number;
}

const API_BASE = process.env.FORGE_API_PROXY_TARGET ?? "http://127.0.0.1:8080";

// 购物车条目 (多商品下单)
export interface CartItemInput {
  sku: string;
  quantity: number;
}

// 下单创建订单 (后端按会员等级折扣 + 优惠券应用, 身份由会话 token 解析): 传购物车条目数组 + 可选优惠券码.
// 返回 OrderSummary (含折后 amountMicro 与折前 originalAmountMicro / discountPct / couponCode).
// 注意: 本函数仅可在服务端调用 (读取 httpOnly 会话 cookie), 客户端下单走 server action.
export async function createOrderSummary(
  items: CartItemInput[],
  token: string,
  couponCode?: string
): Promise<OrderSummary | null> {
  try {
    const res = await fetch(`${API_BASE}/internal/orders`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`
      },
      body: JSON.stringify({ items, coupon_code: couponCode }),
      cache: "no-store"
    });
    if (res.ok) {
      return (await res.json()) as OrderSummary;
    }
  } catch {
    // 下单失败交由调用方处理
  }
  return null;
}

// 读取当前用户最新 PENDING 订单 (身份由会话 token 解析)
export async function fetchOrderSummary(token: string): Promise<OrderSummary | null> {
  try {
    const res = await fetch(`${API_BASE}/internal/orders/preview`, {
      headers: { Authorization: `Bearer ${token}` },
      cache: "no-store"
    });
    if (res.ok) {
      return (await res.json()) as OrderSummary;
    }
  } catch {
    // 后端未就绪
  }
  return null;
}
