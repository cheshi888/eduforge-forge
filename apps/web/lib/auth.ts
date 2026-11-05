// 会话管理: 服务端从 httpOnly cookie 读取会话 token, 调后端 /auth/me 解析用户身份.
// 取代写死的 u-demo 与 FORGE_DEMO_USER 环境变量 (保留 FORGE_DEMO_USER 作为未登录时的降级).

import { cookies } from "next/headers";

// 服务端 fetch 必须用绝对 URL (Node fetch 不接受相对路径 "/api/..."),
// 直接指向后端 FORGE_API_PROXY_TARGET (默认本机 8080), 绕过前端 rewrites.
const API_BASE = process.env.FORGE_API_PROXY_TARGET ?? "http://127.0.0.1:8080";
export const SESSION_COOKIE = "forge_session";

export interface AuthUser {
  uid: string;
  email: string;
  preferredLanguage: string;
  tierCode: string;
}

// 服务端读取会话 cookie 原始 token (不暴露给客户端)
export async function getSessionToken(): Promise<string | null> {
  const store = await cookies();
  return store.get(SESSION_COOKIE)?.value ?? null;
}

// 服务端解析当前登录用户: 有 token 则调 /auth/me, 无则回退 demo 用户 (兼容单用户 demo 模式)
export async function resolveCurrentUser(): Promise<AuthUser | null> {
  const token = await getSessionToken();
  if (token) {
    try {
      const res = await fetch(`${API_BASE}/auth/me`, {
        headers: { Authorization: `Bearer ${token}` },
        cache: "no-store"
      });
      if (res.ok) {
        return (await res.json()) as AuthUser;
      }
    } catch {
      // 后端不可达时走降级
    }
  }
  // 未登录: 回退到 FORGE_DEMO_USER (保留旧 demo 单用户模式, 便于无登录直接体验)
  const demoUid = process.env.FORGE_DEMO_USER;
  if (demoUid) {
    return { uid: demoUid, email: `${demoUid}@demo.local`, preferredLanguage: "zh", tierCode: "free" };
  }
  return null;
}

// 服务端解析 uid (未登录返回 null, 不再写死 u-demo)
export async function resolveUid(): Promise<string | null> {
  const user = await resolveCurrentUser();
  return user?.uid ?? null;
}

// 设置会话 cookie (供 login/register server action 调用)
export async function setSessionCookie(token: string) {
  const store = await cookies();
  store.set(SESSION_COOKIE, token, {
    httpOnly: true,
    sameSite: "lax",
    secure: process.env.NODE_ENV === "production",
    maxAge: 60 * 60 * 24 * 30, // 30 天
    path: "/"
  });
}

export async function clearSessionCookie() {
  const store = await cookies();
  store.delete(SESSION_COOKIE);
}
