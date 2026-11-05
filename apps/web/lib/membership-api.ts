// 会员中心 + 内容交付 API 客户端: 走 /api-user/* (服务端代理注入会话 token, 身份由后端从 token 解析)
// 与 /api-admin/* (服务端代理注入 admin token). 前端不持有、不传递任何 uid / 鉴权头.
// 金额 6 位小数微单位 int64, 展示走 lib/amount.ts formatMicro (BigInt)
const API_BASE = "/api-user";
const ADMIN_BASE = "/api-admin";

export interface Tier {
  code: string;
  name: string;
  priceMicro: number;
  durationDays: number;
  discountPct: number;
  maxDownloads: number;
  perks: string[];
}

export interface UserTier {
  uid: string;
  tierCode: string;
  tierExpiresAt: string | null;
  totalSpentMicro: number;
  discountPct: number;
  maxDownloads: number;
  perkLabels: string[];
}

export interface Delivery {
  id: number;
  deliveryType: "file" | "card_key" | "content" | "license";
  fileName: string | null;
  fileUrl: string | null;
  cardKeyId: number | null;
  cardKey: string | null;
  downloadCount: number;
  productSku: string;
  productTitle: string;
  productCategory: string;
  createdAt: string;
}

export interface UploadedFile {
  id: number;
  productId: number;
  fileName: string;
  storageKey: string;
  sizeBytes: number | null;
  active: boolean;
}

class ApiErr extends Error {
  status: number | null;
  constructor(msg: string, status: number | null) {
    super(msg);
    this.status = status;
  }
}

async function userRequest<T>(path: string, init: RequestInit = {}): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`${API_BASE}${path}`, {
      ...init,
      cache: "no-store",
      headers: { "Content-Type": "application/json", ...(init.headers ?? {}) }
    });
  } catch {
    throw new ApiErr("后端不可达", null);
  }
  if (res.status === 401) {
    // 未登录: 抛带 status 的错误, 组件据此显示"请先登录"而非原始 401 文案
    throw new ApiErr("unauthorized", 401);
  }
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new ApiErr(`请求失败 (${res.status}) ${text}`.trim(), res.status);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

async function adminRequest<T>(path: string, init: RequestInit = {}): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`${ADMIN_BASE}${path}`, {
      ...init,
      cache: "no-store",
      headers: { "Content-Type": "application/json", ...(init.headers ?? {}) }
    });
  } catch {
    throw new ApiErr("后端不可达", null);
  }
  if (res.status === 401) throw new ApiErr("未授权 (401)", 401);
  if (res.status === 503) throw new ApiErr("管理后台未配置 (503)", 503);
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new ApiErr(`请求失败 (${res.status}) ${text}`.trim(), res.status);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export function fetchTiers(): Promise<Tier[]> {
  return userRequest<Tier[]>("/user/tier");
}

// 用户侧身份由后端从会话 token 解析, 不再传 uid (防 IDOR)
export function fetchProfile(): Promise<UserTier> {
  return userRequest<UserTier>("/user/profile");
}

// 会员升级已改为支付流程: 用户侧不再直接调用此端点 (后端返回 403).
// 升级路径: 购买会员商品下单支付 → webhook 结算自动晋升, 或由 admin 通道人工变更.
// 保留此函数仅为类型兼容, 新代码不应使用.
export function upgradeTier(tierCode: string): Promise<{ uid: string; tierCode: string }> {
  return userRequest(`/user/profile`, {
    method: "POST",
    body: JSON.stringify({ tier_code: tierCode })
  });
}

export function fetchDeliveries(): Promise<Delivery[]> {
  return userRequest<Delivery[]>("/user/deliveries");
}

export function markDownloaded(id: number): Promise<void> {
  return userRequest<void>(`/user/deliveries/${id}/download`, {
    method: "POST",
    body: "{}"
  });
}

export function adminListTiers(): Promise<Tier[]> {
  return adminRequest<Tier[]>("/admin/memberships/tiers");
}

export function adminSetUserTier(uid: string, tierCode: string): Promise<void> {
  return adminRequest<void>(`/admin/users/${encodeURIComponent(uid)}/tier`, {
    method: "POST",
    body: JSON.stringify({ uid, tier_code: tierCode })
  });
}
