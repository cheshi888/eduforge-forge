import { NextRequest, NextResponse } from "next/server";
import { cookies } from "next/headers";

// 用户侧安全代理: 前端"use client"组件无法读取 httpOnly 会话 cookie,
// 本路由在服务端从 cookie 读会话 token, 注入 Authorization: Bearer 后转发到 rust-core.
// 前端调用 /api-user/<后端路径> (如 /api-user/user/profile), 无需携带鉴权头.
// 无有效会话时直接 401 (与后端一致), 不转发请求.
const BACKEND = process.env.FORGE_API_PROXY_TARGET ?? "http://127.0.0.1:8080";
const SESSION_COOKIE = "forge_session";
// 公开端点白名单: 后端本身不鉴权的端点, 无需会话即可访问 (如会员套餐列表)。
// 这些端点即使未登录也应转发到后端, 否则前端会在未登录时误报 401。
const PUBLIC_PATHS = new Set(["/user/tier"]);

export async function GET(
  req: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  return proxy(req, { params });
}

export async function POST(
  req: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  return proxy(req, { params });
}

export async function PUT(
  req: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  return proxy(req, { params });
}

export async function DELETE(
  req: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  return proxy(req, { params });
}

async function proxy(
  req: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  const upstreamPath = `/${path.join("/")}`;

  const store = await cookies();
  const token = store.get(SESSION_COOKIE)?.value;
  // 无会话时: 公开端点放行 (后端本身不鉴权), 其余一律 401 (与后端 fail-closed 一致)
  if (!token && !PUBLIC_PATHS.has(upstreamPath)) {
    return NextResponse.json({ error: "unauthorized" }, { status: 401 });
  }

  const qs = req.nextUrl.search;
  const upstream = `${BACKEND}${upstreamPath}${qs}`;

  const headers = new Headers();
  if (token) headers.set("Authorization", `Bearer ${token}`);
  const ct = req.headers.get("content-type");
  if (ct) headers.set("content-type", ct);

  let body: BodyInit | undefined;
  if (req.method === "POST" || req.method === "PUT") {
    body = await req.text();
  }

  try {
    const upstreamRes = await fetch(upstream, {
      method: req.method,
      headers,
      body,
      cache: "no-store"
    });
    const text = await upstreamRes.text();
    return new NextResponse(text, {
      status: upstreamRes.status,
      headers: {
        "content-type":
          upstreamRes.headers.get("content-type") ?? "application/json"
      }
    });
  } catch {
    return NextResponse.json({ error: "后端不可达" }, { status: 502 });
  }
}
