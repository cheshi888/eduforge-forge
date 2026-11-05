// 优惠券客户端: 领券 / 我的券 / 可用券列表.
// 用户侧 (领券/我的券) 走 /api-user (服务端代理注入会话 token); 公开可用券走 /api/catalog/coupons.

export interface Coupon {
  id: number;
  code: string;
  title: string;
  couponType: "fixed" | "percent";
  discountValueMicro: number;
  discountPct: number | null;
  thresholdMicro: number;
  validUntil: string | null;
  status?: string;
}

const API_BASE =
  typeof window === "undefined"
    ? process.env.FORGE_API_PROXY_TARGET ?? "http://127.0.0.1:8080"
    : "/api";
const USER_BASE =
  typeof window === "undefined"
    ? process.env.FORGE_API_PROXY_TARGET ?? "http://127.0.0.1:8080"
    : "/api-user";

export async function fetchAvailableCoupons(): Promise<Coupon[]> {
  try {
    const res = await fetch(`${API_BASE}/catalog/coupons`, { cache: "no-store" });
    if (res.ok) return (await res.json()) as Coupon[];
  } catch {
    // 后端不可达
  }
  return [];
}

export async function fetchMyCoupons(): Promise<Coupon[]> {
  try {
    const res = await fetch(`${USER_BASE}/user/coupons`, { cache: "no-store" });
    if (res.ok) return (await res.json()) as Coupon[];
  } catch {
    // 后端不可达
  }
  return [];
}

export async function claimCoupon(code: string): Promise<void> {
  const res = await fetch(`${USER_BASE}/user/coupons`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code })
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`领券失败 (${res.status}) ${text}`.trim());
  }
}
