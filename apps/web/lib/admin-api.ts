// 管理后台 API 客户端: 对接 rust-core /admin/* 接口.
// 金额一律 6 位微单位 int64 (JS number 传输, 展示走 lib/amount.ts 的 BigInt 运算),
// 汇率为十进制字符串. 鉴权: Bearer <FORGE_ADMIN_TOKEN>, 未配置 token 后端一律 401.

export type ProductCategory = "source" | "tutorial" | "card" | "resource";

export interface Product {
  id: number;
  sku: string;
  title: string;
  category: ProductCategory;
  cover: string | null;
  description: string | null;
  unitPriceMicro: number;
  currency: string;
  stock: number;
  active: boolean;
  createdAt: string;
}

export interface ProductUpsertReq {
  sku: string;
  title: string;
  category: ProductCategory;
  cover?: string;
  description?: string;
  unitPriceMicro: number;
  currency: string;
  stock: number;
  active: boolean;
}

export interface Order {
  id: number;
  outTradeNo: string;
  userUid: string;
  productSku: string;
  productTitle: string;
  quantity: number;
  expectedAmountMicro: number;
  paidAmountMicro: number | null;
  currency: string;
  status: string;
  gatewayType: string | null;
  cryptoNetwork: string | null;
  blockchainConfirmations: number;
  lockedExchangeRate: string | null;
  createdAt: string;
}

export interface OrderDetail {
  order: Order;
  transactions: {
    gatewayTxnId: string;
    gatewayType: string;
    amountMicro: number;
    currency: string;
    status: string;
    confirmations: number | null;
    receivedAt: string;
  }[];
  cardKeys: {
    id: number;
    keyCode: string;
    status: string;
    usedAt: string | null;
  }[];
}

export type CardKeyStatus = "AVAILABLE" | "LOCKED" | "USED" | "REVERSED";

export interface CardKey {
  id: number;
  productSku: string;
  keyCode: string;
  status: CardKeyStatus;
  orderOutTradeNo: string | null;
  createdAt: string;
}

export type FraudReason =
  | "currency_mismatch"
  | "amount_mismatch"
  | "orphan_callback"
  | "confirmation_insufficient"
  | "other";

export interface FraudEvent {
  id: number;
  outTradeNo: string;
  orderId: number | null;
  reason: FraudReason;
  receivedAmountMicro: number | null;
  expectedAmountMicro: number | null;
  currency: string | null;
  gatewayType: string | null;
  resolved: boolean;
  resolvedBy: string | null;
  createdAt: string;
}

export interface Stats {
  todayOrders: number;
  todayRevenueMicro: number;
  pendingShip: number;
  availableCardKeys: number;
  unresolvedFraud: number;
  ordersByStatus: { status: string; count: number }[];
  revenueByGateway: { gateway: string; revenueMicro: number }[];
}

export interface GenerateKeysResult {
  productId: number;
  count: number;
  ids: number[];
}

export class AdminApiError extends Error {
  readonly status: number | null;
  constructor(message: string, status: number | null) {
    super(message);
    this.name = "AdminApiError";
    this.status = status;
  }
}

export const API_BASE = process.env.FORGE_API_BASE ?? "http://127.0.0.1:8080";
const ADMIN_TOKEN = process.env.FORGE_ADMIN_TOKEN ?? "";

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
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
    throw new AdminApiError("后端不可达", null);
  }
  if (res.status === 401) {
    throw new AdminApiError("未授权 (401)", 401);
  }
  if (res.status === 404) {
    throw new AdminApiError("资源不存在 (404)", 404);
  }
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new AdminApiError(`请求失败 (${res.status}) ${text}`.trim(), res.status);
  }
  if (res.status === 204) {
    return undefined as T;
  }
  return (await res.json()) as T;
}

function qs(params: Record<string, string | number | undefined>): string {
  const u = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== "") u.set(k, String(v));
  }
  const s = u.toString();
  return s ? `?${s}` : "";
}

export function fetchStats(): Promise<Stats> {
  return request<Stats>("/admin/stats");
}

export function listProducts(): Promise<Product[]> {
  return request<Product[]>("/admin/products");
}

export function createProduct(body: ProductUpsertReq): Promise<{ id: number }> {
  return request<{ id: number }>("/admin/products", {
    method: "POST",
    body: JSON.stringify(body)
  });
}

export function updateProduct(id: number, body: ProductUpsertReq): Promise<void> {
  return request<void>(`/admin/products/${id}`, {
    method: "PUT",
    body: JSON.stringify(body)
  });
}

export function deleteProduct(id: number): Promise<void> {
  return request<void>(`/admin/products/${id}`, { method: "DELETE" });
}

export function adjustStock(id: number, delta: number): Promise<void> {
  return request<void>(`/admin/products/${id}/stock`, {
    method: "POST",
    body: JSON.stringify({ delta })
  });
}

export function listOrders(params?: {
  status?: string;
  gatewayType?: string;
  limit?: number;
  offset?: number;
}): Promise<Order[]> {
  const q = qs({
    status: params?.status,
    gateway_type: params?.gatewayType,
    limit: params?.limit,
    offset: params?.offset
  });
  return request<Order[]>(`/admin/orders${q}`);
}

export function getOrder(outTradeNo: string): Promise<OrderDetail> {
  return request<OrderDetail>(`/admin/orders/${encodeURIComponent(outTradeNo)}`);
}

export function shipOrder(outTradeNo: string): Promise<{ shipped: string }> {
  return request<{ shipped: string }>(
    `/admin/orders/${encodeURIComponent(outTradeNo)}/ship`,
    { method: "POST", body: "{}" }
  );
}

export function listCardKeys(): Promise<CardKey[]> {
  return request<CardKey[]>("/admin/card-keys");
}

export function generateCardKeys(productId: number, count: number): Promise<GenerateKeysResult> {
  return request<GenerateKeysResult>("/admin/card-keys", {
    method: "POST",
    body: JSON.stringify({ productId, count })
  });
}

export function revokeCardKey(id: number): Promise<void> {
  return request<void>(`/admin/card-keys/${id}`, { method: "PUT" });
}

export function listFraud(): Promise<FraudEvent[]> {
  return request<FraudEvent[]>("/admin/fraud");
}

export function resolveFraud(id: number): Promise<void> {
  return request<void>(`/admin/fraud/${id}`, { method: "POST", body: "{}" });
}

export interface UploadedFile {
  id: number;
  productId: number;
  fileName: string;
  storageKey: string;
  sizeBytes: number | null;
  active: boolean;
}

export function adminListFiles(productId?: number): Promise<UploadedFile[]> {
  const q = productId ? `?product_id=${productId}` : "";
  return request<UploadedFile[]>(`/admin/deliveries/files${q}`);
}

export function adminUploadFileMeta(body: {
  productId: number;
  fileName: string;
  storageKey: string;
  sizeBytes?: number;
  mimeType?: string;
  checksumSha256?: string;
}): Promise<{ id: number }> {
  return request<{ id: number }>("/admin/deliveries/files", {
    method: "POST",
    body: JSON.stringify({
      product_id: body.productId,
      file_name: body.fileName,
      storage_key: body.storageKey,
      size_bytes: body.sizeBytes ?? null,
      mime_type: body.mimeType ?? null,
      checksum_sha256: body.checksumSha256 ?? null
    })
  });
}
