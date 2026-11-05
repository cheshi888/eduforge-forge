import { NextRequest, NextResponse } from "next/server";

// 公开 API 代理: 前端"use client"组件调用 /api/<后端路径> 转发到 rust-core 的公开端点
// (仅限 /catalog/* 等无需鉴权的只读端点)。用户侧鉴权走 /api-user/*, 管理侧走 /api-admin/*。
// 本路由不做鉴权注入, 只做透明转发。
const BACKEND = process.env.FORGE_API_PROXY_TARGET ?? "http://127.0.0.1:8080";

export async function GET(
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
  const qs = req.nextUrl.search;
  const upstream = `${BACKEND}/${path.join("/")}${qs}`;

  const headers = new Headers();
  const ct = req.headers.get("content-type");
  if (ct) headers.set("content-type", ct);

  try {
    const upstreamRes = await fetch(upstream, {
      method: req.method,
      headers,
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
