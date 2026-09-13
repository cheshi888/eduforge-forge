// 金额以 6 位小数微单位 (int64) 传输, 与 Rust/Go 侧对齐; 汇率以十进制字符串传输 (M2-05, 全程零浮点)
export interface OrderSummary {
  outTradeNo: string;
  quantity: number;
  amountMicro: number;
  currency: string;
  lockedRateUsd: string | null;
  gatewayPayload?: string;
}

const API_BASE = process.env.FORGE_API_BASE ?? "http://127.0.0.1:8080";

// C-08: uid 仅由服务端环境变量解析 (FORGE_DEMO_USER, 缺省 u-demo),
// 客户端请求头不再参与 uid 决策 (旧版 x-forge-user 头可伪造 → 越权读任意用户订单)
export function resolveUid(): string {
  return process.env.FORGE_DEMO_USER ?? "u-demo";
}

export async function fetchOrderSummary(): Promise<OrderSummary | null> {
  const uid = resolveUid();
  try {
    const res = await fetch(`${API_BASE}/internal/orders/preview?uid=${uid}`, {
      cache: "no-store"
    });
    if (res.ok) {
      const data = (await res.json()) as OrderSummary;
      return data;
    }
  } catch {
    // 后端未就绪时按开关决定是否回退 mock
  }
  // M2-06: 生产故障显式渲染不可用, 不再静默展示演示订单号 (演示态需显式开启 FORGE_MOCK_FALLBACK=1)
  if (process.env.FORGE_MOCK_FALLBACK !== "1") {
    return null;
  }
  return {
    outTradeNo: "FT-20260909-8f3a2c",
    quantity: 3,
    amountMicro: 3_000_000_000,
    currency: "CNY",
    lockedRateUsd: "0.1385"
  };
}
