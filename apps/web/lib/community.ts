// 社区客户端: 评论 / 问答 / 收藏 / 通知.
// 公开读 (评论/问答列表) 走 /api/catalog/*; 用户侧写 (评论/提问/收藏/通知) 走 /api-user/*.

export interface Review {
  id: number;
  rating: number;
  content: string | null;
  userUid: string;
  createdAt: string;
}

export interface Question {
  id: number;
  question: string;
  answer: string | null;
  userUid: string;
  createdAt: string;
}

export interface Favorite {
  id: number;
  sku: string;
  title: string;
  category: string;
  priceMicro: number;
  currency: string;
  createdAt: string;
}

export interface Notification {
  id: number;
  kind: string;
  title: string;
  body: string | null;
  read: boolean;
  createdAt: string;
}

const API_BASE =
  typeof window === "undefined"
    ? process.env.FORGE_API_PROXY_TARGET ?? "http://127.0.0.1:8080"
    : "/api";
const USER_BASE =
  typeof window === "undefined"
    ? process.env.FORGE_API_PROXY_TARGET ?? "http://127.0.0.1:8080"
    : "/api-user";

export async function fetchReviews(sku: string): Promise<Review[]> {
  try {
    const res = await fetch(`${API_BASE}/catalog/products/${encodeURIComponent(sku)}/reviews`, {
      cache: "no-store"
    });
    if (res.ok) return (await res.json()) as Review[];
  } catch {
    // 后端不可达
  }
  return [];
}

export async function fetchQuestions(sku: string): Promise<Question[]> {
  try {
    const res = await fetch(`${API_BASE}/catalog/products/${encodeURIComponent(sku)}/questions`, {
      cache: "no-store"
    });
    if (res.ok) return (await res.json()) as Question[];
  } catch {
    // 后端不可达
  }
  return [];
}

export async function submitReview(sku: string, rating: number, content: string): Promise<void> {
  const res = await fetch(`${USER_BASE}/user/reviews`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ sku, rating, content: content || null })
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`评论失败 (${res.status}) ${text}`.trim());
  }
}

export async function submitQuestion(sku: string, question: string): Promise<void> {
  const res = await fetch(`${USER_BASE}/user/questions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ sku, question })
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`提问失败 (${res.status}) ${text}`.trim());
  }
}

export async function fetchFavorites(): Promise<Favorite[]> {
  try {
    const res = await fetch(`${USER_BASE}/user/favorites`, { cache: "no-store" });
    if (res.ok) return (await res.json()) as Favorite[];
  } catch {
    // 后端不可达
  }
  return [];
}

export async function toggleFavorite(sku: string): Promise<boolean> {
  const res = await fetch(`${USER_BASE}/user/favorites`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ sku })
  });
  if (!res.ok) return false;
  const data = (await res.json()) as { favorited: boolean };
  return data.favorited;
}

export async function fetchNotifications(): Promise<Notification[]> {
  try {
    const res = await fetch(`${USER_BASE}/user/notifications`, { cache: "no-store" });
    if (res.ok) return (await res.json()) as Notification[];
  } catch {
    // 后端不可达
  }
  return [];
}

export async function markAllNotificationsRead(): Promise<void> {
  await fetch(`${USER_BASE}/user/notifications/read`, { method: "POST" });
}
