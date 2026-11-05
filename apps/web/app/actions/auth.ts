"use server";

// 认证 Server Actions: 登录/注册/登出, 会话存 httpOnly cookie.

import { setSessionCookie, clearSessionCookie } from "@/lib/auth";

// 服务端 fetch 必须用绝对 URL (Node fetch 不接受相对路径), 直接指向后端.
const API_BASE = process.env.FORGE_API_PROXY_TARGET ?? "http://127.0.0.1:8080";

export interface AuthState {
  ok: boolean;
  error?: string;
}

async function post(path: string, body: Record<string, unknown>): Promise<{ token: string } | { error: string }> {
  try {
    const res = await fetch(`${API_BASE}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      cache: "no-store"
    });
    const data = await res.json().catch(() => ({}));
    if (res.ok) {
      return data as { token: string };
    }
    return { error: (data as { error?: string }).error ?? `request failed (${res.status})` };
  } catch {
    return { error: "backend unreachable" };
  }
}

export async function loginAction(
  _prev: AuthState | null,
  formData: FormData
): Promise<AuthState> {
  const email = String(formData.get("email") ?? "");
  const password = String(formData.get("password") ?? "");
  if (!email || !password) {
    return { ok: false, error: "email and password required" };
  }
  const r = await post("/auth/login", { email, password });
  if ("error" in r) {
    return { ok: false, error: r.error };
  }
  await setSessionCookie(r.token);
  return { ok: true };
}

export async function registerAction(
  _prev: AuthState | null,
  formData: FormData
): Promise<AuthState> {
  const email = String(formData.get("email") ?? "");
  const password = String(formData.get("password") ?? "");
  if (!email || !password) {
    return { ok: false, error: "email and password required" };
  }
  const r = await post("/auth/register", { email, password });
  if ("error" in r) {
    return { ok: false, error: r.error };
  }
  await setSessionCookie(r.token);
  return { ok: true };
}

export async function logoutAction(): Promise<void> {
  // 客户端不直接持有 token; 仅清除 cookie (后端会话可由定时清理回收)
  await clearSessionCookie();
}
