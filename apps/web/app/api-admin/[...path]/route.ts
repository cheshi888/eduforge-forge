import { NextRequest, NextResponse } from "next/server";

// 管理后台安全代理: 前端不持有 admin token (修复 NEXT_PUBLIC token 泄露到浏览器 bundle)。
// 本路由在服务端读取 FORGE_ADMIN_TOKEN (仅服务端可见), 注入 Authorization 后转发到 rust-core。
// 前端调用 /api-admin/<后端路径>, 无需也不得携带任何鉴权头。
const BACKEND = process.env.FORGE_API_PROXY_TARGET ?? "http://127.0.0.1:8080";
const ADMIN_TOKEN = process.env.FORGE_ADMIN_TOKEN ?? "";

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
  // 服务端未配置 token 时直接 503 (与后端 fail-closed 语义一致, 且不泄露任何信息)
  if (!ADMIN_TOKEN) {
    return NextResponse.json(
      { error: "admin token not configured on server" },
      { status: 503 }
    );
  }
  const { path } = await params;
  const qs = req.nextUrl.search;
  const upstream = `${BACKEND}/${path.join("/")}${qs}`;

  const headers = new Headers();
  headers.set("Authorization", `Bearer ${ADMIN_TOKEN}`);
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
