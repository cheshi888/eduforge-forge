# EduForge-Neo

下一代知识付费平台。热插拔模块 + 零信任支付 + 全球化多语言。

## Monorepo 目录结构

```
edu-forge-neo/
├── apps/
│   ├── rust-core/            # 核心支付与资产域 (Rust, Axum + sqlx + rust_decimal)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs               # 入口 + tracing 初始化
│   │       ├── config.rs             # 环境配置
│   │       ├── db.rs                 # 连接池
│   │       ├── events.rs             # NATS JetStream 发布
│   │       ├── domain/
│   │       │   ├── mod.rs
│   │       │   └── order.rs          # 订单状态机 (PENDING/PAID/PENDING_BALANCE/FRAUD_SUSPENDED/...)
│   │       └── api/
│   │           ├── mod.rs
│   │           └── webhook.rs        # !!! 三道校验的 Webhook Handler !!!
│   ├── go-plugins/           # 业务插件与热插拔域 (Go, Gin + GORM + go-plugin)
│   │   ├── go.mod
│   │   ├── main.go             # Host 进程 + /admin/plugins 热插拔 API
│   │   ├── cmd/plugin-second-half/main.go   # 内置插件: 第二件半价
│   │   └── internal/promo/
│   │       ├── types.go          # PromotionInterface + Decimal (int64 最小单位, 6 位内部精度)
│   │       ├── loader.go         # 插件加载器: 健康检查 + 原子热切换, 不重启 Pod
│   │       └── builtins.go       # 进程内默认插件 (无外部插件时的回退)
│   └── web/                  # 前端 (Next.js 15, React 19, Tailwind, next-intl)
│       ├── package.json
│       ├── middleware.ts             # 国际化路由 /zh /en /ja
│       ├── i18n/                     # next-intl 配置
│       ├── locales/                  # 前后端共用语言包 zh/en/ja/ko
│       ├── app/
│       │   ├── [locale]/checkout/page.tsx
│       │   └── actions/checkout.ts   # Server Action 下单
│       └── components/checkout/checkout-form.tsx   # 支付方式切换 + 金额本地化
├── schema/
│   └── 001_init.sql          # orders / payment_transactions 等 DDL (含联合唯一索引)
├── docs/
│   └── architecture.md       # 架构说明 + ER 图 + 支付防攻击时序
├── k8s/
│   ├── core-deployment.yaml  # Core Deployment + Sidecar (otel + envoy) 注入示例
│   ├── mutating-webhook.yaml # Sidecar 动态注入的 Webhook 模板
│   └── hpa.yaml              # HPA (P95 < 200ms 的容量规划)
└── docker-compose.yml        # Postgres(pgvector) + Redis + NATS + MinIO
```

## 模块划分 (热插拔)

| 模块 | 职责 | 宿主 |
|---|---|---|
| Core | 订单状态机、金额精度、支付网关通信 | Rust |
| Catalog | 商品目录 | Go |
| Promotion | 优惠券/促销算法 (.wasm 或 go-plugin) | Go, 可热插拔 |
| Community | 评论/问答 | Go |

## 构建与运行

```bash
# 基础设施
docker compose up -d

# 数据库初始化
psql "$DATABASE_URL" -f schema/001_init.sql

# Rust Core (需先设置环境变量, 见 apps/rust-core/src/config.rs)
cd apps/rust-core && cargo run

# Go 插件宿主
cd apps/go-plugins
go build -o bin/plugin-second-half ./cmd/plugin-second-half
go run .

# 前端
cd apps/web && npm install && npm run dev
```

## 关键安全设计 (摘要)

1. 金额一律 `rust_decimal::Decimal` / `NUMERIC(20,6)` / Go `int64` 最小单位, 全程禁用浮点。
2. 回调处理三道校验: 入口 HMAC 头 → 网关验签 → (联合唯一索引幂等 + `SELECT FOR UPDATE` 悲观锁 + 本地 expected_amount 严格比对)。
3. Crypto 回调按确认数阈值 (BTC 6 / TRC-20 1) 决定 `PAID` 或 `PENDING_BALANCE`。
4. 详见 docs/architecture.md。
