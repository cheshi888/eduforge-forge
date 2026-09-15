// 会员中心 + 内容交付 API 客户端: 走 /user/* (用户侧, uid 服务端解析) 与 /admin/* (Bearer)
// 金额 6 位小数微单位 int64, 展示走 lib/amount.ts formatMicro (BigInt)
import { resolveUid } from "@/lib/api";

const API_BASE = "/api";
const ADMIN_TOKEN = process.env.NEXT_PUBLIC_FORGE_ADMIN_TOKEN ?? "";

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
    res = await fetch(`${API_BASE}${path}`, {
      ...init,
      cache: "no-store",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${ADMIN_TOKEN}`,
        ...(init.headers ?? {})
      }
    });
  } catch {
    throw new ApiErr("后端不可达", null);
  }
  if (res.status === 401) throw new ApiErr("未授权 (401)", 401);
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

export function fetchProfile(): Promise<UserTier> {
  return userRequest<UserTier>(`/user/profile?uid=${encodeURIComponent(resolveUid())}`);
}

export function upgradeTier(tierCode: string): Promise<{ uid: string; tierCode: string }> {
  return userRequest(`/user/profile`, {
    method: "POST",
    body: JSON.stringify({ uid: resolveUid(), tier_code: tierCode })
  });
}

export function fetchDeliveries(): Promise<Delivery[]> {
  return userRequest<Delivery[]>(`/user/deliveries?uid=${encodeURIComponent(resolveUid())}`);
}

export function markDownloaded(id: number): Promise<void> {
  return userRequest<void>(`/user/deliveries/${id}/download?uid=${encodeURIComponent(resolveUid())}`, {
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
