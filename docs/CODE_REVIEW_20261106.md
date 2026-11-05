# EduForge-Neo 严格代码审查 + 真人仿真测试报告（Round 3）

- 日期：2026-11-06
- 仓库：https://github.com/cheshi888/eduforge-forge
- 方法：全仓静态取证（行号级）+ **真实编译/单测/构建实测** + 前端逻辑真人仿真验证
- 原则：失败优先；**凡未实跑的一律如实标注，绝不声称通过**

---

## 0. 真实执行证据（本轮全部实测，非声称）

| 项 | 命令 | 真实结果 |
|---|---|---|
| rust-core 编译 | `cargo check` | ✅ 0 错误（安装 Rust 1.99.1 msvc 工具链后真实编译） |
| rust-core 单测 | `cargo test --jobs 1` | ✅ **25 passed / 0 failed**（0 warning） |
| go-plugins 构建 | `go build ./...` + `go vet ./...` | ✅ 通过 |
| go-plugins 单测 | `go test ./...` | ✅ `internal/promo ok`（含 4 条 H2-04 溢出回归） |
| web 类型检查 | `npx tsc --noEmit` | ✅ 0 错误 |
| web 生产构建 | `npx next build` | ✅ exit 0，全部路由生成（含 /ko、admin、membership、my-files、api-admin 代理） |
| 安全门禁 | `bash scripts/check-security-policy.sh` | ✅ PASS |
| i18n key 一致性 | Python diff 4 个 locales/*.json | ✅ 4 语言 checkout 段 14 key 全一致、顶层 7 namespace 一致 |
| 金额逻辑 | Node 真实执行 amount.ts 核心逻辑 | ⚠️ 发现 1 个真实显示 bug（已修复） |

---

## 1. 本轮新发现并修复的真实缺陷

### B3-01（CRITICAL）前端金额显示错误 —— round2 多乘 100

- 位置：`apps/web/lib/amount.ts` `round2()`（原 L25）
- 证据：`let frac2 = ((abs % 1_000_000n) * 100n + 5_000n) / 10_000n;` 多了 `* 100n`。
- 现象（Node 实测）：`199990000` 微单位（199.99 元）被渲染成 **298**；`123456789`（123.46 元）渲染成 168.68。
- 根因：M2-05「全量 BigInt 重写」时引入的数学错误，`frac2` 被放大 100 倍后与 `toDisplayNumber` 的 `/100` 错位。
- 影响：**所有金额展示错误**（结算页应付金额、会员累计消费、管理后台金额列、卡密定价全部错）。
- 修复：`(abs % 1_000_000n + 5_000n) / 10_000n`（去掉多余 `* 100n`）。
- 回归：Node 实测 8 组边界用例（299 / 199.99 / 123.46 / 0.01 / 0.99 / 1 / 1.005 进位 / 大额）全对，`lockedUsd` 299×0.1385=41.41 正确。

### B3-02（HIGH）管理后台 token 泄露到浏览器 bundle

- 位置：`apps/web/lib/admin-api.ts:146`、`apps/web/lib/membership-api.ts:6`
- 证据：`const ADMIN_TOKEN = process.env.NEXT_PUBLIC_FORGE_ADMIN_TOKEN` —— `NEXT_PUBLIC_` 前缀将 token 内联进前端 JS，任何访客可直接以 Bearer 调 `/api/admin/*`（改商品/发货/吊销卡密/改等级），后台鉴权被击穿。
- 修复：新增服务端代理 `app/api-admin/[...path]/route.ts`，在服务端读 `FORGE_ADMIN_TOKEN`（非 NEXT_PUBLIC）注入 Authorization 转发；前端 admin 请求改走 `/api-admin/*`，不再持有 token。

### B3-03（HIGH）客户端 uid 恒为 "u-demo"，会员/交付/升级失效

- 位置：`lib/membership-api.ts` 调 `resolveUid()`，而 `resolveUid()` 读 `process.env.FORGE_DEMO_USER`（无 NEXT_PUBLIC，客户端为 undefined）→ 恒回落 "u-demo"。
- 影响：部署时若设了 `FORGE_DEMO_USER=张三`，服务端结算用"张三"，客户端会员/我的文件/升级却永远操作 demo 用户，两端不一致。
- 修复：`membership`/`my-files`/`checkout` 三个 page 改为服务端解析 uid 后作为 prop 传给 client 组件；`fetchProfile/fetchDeliveries/upgradeTier/markDownloaded` 改为显式接收 uid。

### B3-04（HIGH）文件下载失效

- 位置：`components/deliveries/deliveries-view.tsx:36-38`
- 证据：`window.open(`${d.fileUrl}&dl=1`)`，而 `d.fileUrl` 是 `uploaded_files.storage_key`（`local://`/`s3://`），非可下载 URL。
- 修复：文件/内容类改走真实下载端点 `GET /api/files/{id}?uid=`（后端校验属主并流式返回）；卡密类不再误触发下载。

### B3-05（MEDIUM）`/admin/memberships/tiers` 缺鉴权

- 位置：`apps/rust-core/src/api/membership.rs` 原 `list_tiers`（admin 侧）无 `guard_admin`。
- 修复：拆分出 `list_tiers_admin`（带鉴权）+ 复用 `tiers_payload`；`/admin/memberships/tiers` 改指向带鉴权版本。

### B3-06（MEDIUM）外币支付金额币种显示错误

- 位置：`components/checkout/checkout-form.tsx:52`
- 证据：`FX_METHODS.includes(method) ? "USD"` 把 paypal/card 的 CNY 订单金额直接标成 USD 且未按锁定汇率换算。
- 修复：改用 `amount.ts` 的 `orderCurrency()`（仅 usdt → USD），paypal/card 仍以订单币种显示 + 锁定汇率单独提示。

### B3-07（MEDIUM）会员折扣只展示不结算

- 位置：`components/checkout/checkout-form.tsx` `discountedMicro` 用 `Math.round` 浮点算折扣价并展示，但表单不提交金额，后端仍按 `expected_amount` 原价结算 → 用户看到折扣价却按原价扣款。
- 修复：移除误导性「折后应付价」展示，改为「下单时自动应用」提示（折扣实际结算需后端下单链路实现，本轮不动资金核心逻辑，避免引入新资金风险）。

### B3-08（LOW）订单渠道筛选缺 cardacq/coinbase

- 位置：`components/admin/orders-view.tsx:23` `GATEWAYS` 缺 `cardacq`/`coinbase`（`usdt` 是 UI 值非落库值）。
- 修复：改为 `["alipay","wechat","stripe","paypal","cardacq","coinbase"]`。

### B3-09（LOW）步骤条 activeStep 逻辑异常

- 位置：`checkout-form.tsx:82` `state?.ok ? 2 : method === "alipay" ? 2 : 1` 默认 alipay 直接显示第 3 步。
- 修复：`state?.ok ? 2 : 1`。

---

## 2. 已核实为「真实落地」的历史修复（未发现回退）

本轮逐文件核对，以下历史报告声称的修复**均真实存在于代码中**：

- C2-01 Braintree kind 白名单（braintree.rs:54-68）✅
- C2-02 Alipay notify_type 白名单（alipay.rs:63-75）✅
- C2-03 Coinbase 未知网络拒收（coinbase.rs:33-44）✅
- C-04 Braintree keyed HMAC + 空密钥 fail-closed（braintree.rs:19-23,77-82）✅
- C-05 PayPal 空密钥 fail-closed + event_type 白名单（paypal.rs:15-19,44-49）✅
- C-09/C-10 Alipay hex 优先 + PKCS#8 支持（alipay.rs:35-50）✅
- M2-01 各网关 gateway_txn_id 缺失拒收（stripe/coinbase/paypal/wechat）✅
- B2-01 CURRENCY_MISMATCH 词表（schema/001_init.sql:60）✅
- H2-01 fx_comparable 折算（webhook.rs:422-434）✅
- H2-05 库存 CAS 扣减（webhook.rs:355-363）✅
- M2-02 event_outbox（schema:90-100 + events.rs）✅
- M2-03 DUPLICATE 词（webhook.rs:346）✅
- H2-04 go-plugins 鉴权/溢出/超时（main.go + builtins.go + loader.go）✅
- L2-02 ko 路由（middleware.ts:7 matcher 已含 ko）✅
- M2-04/M2-07/M2-08（db.rs 连接池、internal.rs CAS、main.rs /readyz）✅

---

## 3. 未执行项（如实标注，需环境）

本机**无 docker / psql / NATS**，以下依赖真实基础设施的 live 用例**未实跑**，与前两轮审查一致：

| 用例 | 阻塞原因 |
|---|---|
| webhook 各网关 live 端到端（`scripts/e2e-webhook.sh` 的 TC-03/04/05/11/12/13） | 需 Postgres + NATS + docker |
| 卡密发货/风控落库/统计/会员升级的 DB 实跑（TEST_MATRIX §5.7/5.9） | 需 PG + NATS |
| 并发超卖 H2-05 CAS、幂等重放、确认数轮询 | 需 PG 环境 |
| k8s 层（sidecar 注入、envoy 18080、HPA） | 需 k8s 集群 |

> 这些用例的**代码路径已静态核验正确**，但「代码路径正确」≠「已通过测试」。我不声称它们已通过。

---

## 4. 遗留项（本轮未改，记录在案）

> ⚠️ 本节所列 4 项中，第 1/2/4 项已在后续「阶段 1-4」工作（见 §6-§9）中解决，第 3 项部分解决。状态如下：

1. ~~会员折扣未真正接入结算链路~~ → **已解决**（§6 阶段1 下单端点 + `discount_pct` 联查，纯 Decimal 计算）。
2. ~~多用户 uid 会话化~~ → **已解决**（§6 阶段1 引入 Argon2id 注册/登录 + Bearer token 会话，`/internal/*`、`/user/*` 改为 token 鉴权，消除 IDOR）。
3. **低危 i18n 硬编码** → **部分解决**：`product-detail-view.tsx` 已接入 `useTranslations`；`product-grid.tsx`/`product-card.tsx`/`deliveries-view.tsx`/`membership-view.tsx` 仍有硬编码中文（演示数据层，非资金安全，未阻塞）。
4. ~~checkout-form 折扣曾用浮点~~ → **已解决**（§7 阶段2/§8 阶段3 折扣与优惠券全程纯 Decimal/BigInt，零浮点）。

---

## 6. 阶段1：用户认证体系（新增，已实测）

- `schema/004_auth.sql`：users 表加 `password_hash`；新增 `auth_sessions` 表（token 存 SHA-256 哈希）。
- `apps/rust-core/src/api/auth.rs`：注册 / 登录 / 登出 / `GET /me`；Argon2id 密码哈希；256-bit 随机 token；`user_id_from_token` 提取用户。
- 前端 `lib/auth.ts`、`app/actions/auth.ts`、`/auth` 页面、`components/auth/auth-form.tsx`；会话走 httpOnly cookie（`forge_session`）。
- **关键改造**：`/internal/*`、`/user/*` 从 `?uid=` 查询参数改为 Bearer token 鉴权，消除越权（IDOR）。
- 实测：`cargo test` 含 `api::auth::tests::password_hash_roundtrip` 等，全量 36 passed。

## 7. 阶段2：商品目录 + 购物车（新增，已实测）

- `schema/005_catalog_cart.sql`：`order_items` 表；`api/catalog.rs`：`/catalog/products`、`/catalog/products/:sku`、`/catalog/categories` 公开端点。
- 删除 `lib/products.ts` 硬编码商品清单，新增 `lib/catalog.ts`（真实从后端拉取）+ `schema/seed_demo_products.sql`。
- `CreateOrderReq` 改为 `items: Vec<CartItem>` 多商品下单。
- 前端 `/api-user/[...path]/route.ts` 用户侧代理（服务端注入 Bearer token）。

## 8. 阶段3：营销体系（优惠券/满减，新增，已实测）

- `schema/006_promotion.sql`：`coupons`、`user_coupons` 表；`api/promotion.rs`：优惠券校验 + `apply_coupon()`（fixed/percent 纯 Decimal）。
- 下单链路叠加优惠券（在会员折扣之后），券使用后标记 `LOCKED`。
- 前端 `lib/coupons.ts`、checkout 页支持 `coupon` 参数。
- 实测：`cargo test` 含优惠券用例 `percent_coupon_ten_percent` 等。

## 9. 阶段4：社区 + 通知（新增，已实测）

- `schema/007_community.sql`：`reviews`、`questions`、`answers`、`favorites`、`notifications` 表。
- `api/community.rs`：评论 / 评分 / 问答 / 收藏 / 通知端点（公开读走 `/catalog/*`，用户写走 `/user/*`）。
- `consumer.rs` `ship_order` 增加发货通知落库。
- 前端 `lib/community.ts`、`components/shop/product-detail-view.tsx`（评论/问答/收藏）、`components/nav/notifications-menu.tsx`（通知中心）、`app/[locale]/product/[sku]/page.tsx`。
- **修复关键 bug**：新增 `app/api/[...path]/route.ts` 公开代理 —— 此前 `lib/catalog.ts`/`lib/community.ts` 调用 `/api/catalog/*` 但项目只有 `/api-admin`、`/api-user` 两个代理，公开读端点完全无法触达后端。补上后商品目录/评论/问答列表可正常加载。
- 前端接入 i18n：`product-detail-view.tsx` 走 `useTranslations("shop")`，4 语言补齐 `shop.*` 与 `nav.*`（通知）key。
- 实测：`cargo build`（0 错误）、`cargo test` 36 passed、`npx tsc --noEmit` 0 错误、`npx next build` 成功（含 `/api/[...path]`、`/product/[sku]` 路由）。

---

## 5. 结论

- 历史报告声称的 25 条 rust 单测、go 单测、float_policy 门禁、tsc、next build **均真实通过**（本轮逐一实跑复现）。
- 但历史报告「M2-05 全量 BigInt 重写、金额全程零浮点」的**结论被推翻**：BigInt 重写本身引入了 `round2` 多乘 100 的显示 bug，且 `checkout-form.tsx` 折扣计算仍用 `Math.round` 浮点。这印证了「静态断言通过 ≠ 功能正确」。
- 本轮修复 9 项真实缺陷（1 CRITICAL + 3 HIGH + 3 MEDIUM + 2 LOW），全部重新编译/单测/构建验证通过。
- 后续阶段 1-4 新增功能（认证/目录/购物车/优惠券/社区/通知）全部实测：`cargo test` 36 passed、`cargo build` 0 错误、`tsc --noEmit` 0 错误、`next build` 成功，并修复公开代理缺失导致的目录/社区读端点失效 bug。

---

## 10. 真实基础设施端到端实测（Round 4，本轮全部实跑）

> 背景：本机无 Docker / PostgreSQL / NATS / psql。为做到「百分百真实验证」，
> 用 **PGlite（WASM 真 PostgreSQL）+ 官方 nats-server 二进制** 搭起完整基础设施，
> 再以 **Playwright + 系统 Chrome** 真实浏览器跑通前后端全链路。

### 10.1 基础设施搭建（真实，非 mock）

| 组件 | 方案 | 结果 |
|---|---|---|
| PostgreSQL | PGlite 0.5.15（WASM PG18.3）+ pglite-socket 暴露 TCP:5432 + pgvector 扩展 | ✅ 21 张表全建，8 个 schema 文件全应用 |
| NATS | 官方 nats-server v2.10.26 Windows 二进制，JetStream 开启 | ✅ :4222 监听，readyz 探活通过 |
| rust-core | cargo run，连 PGlite + NATS | ✅ /healthz、/readyz 200 |
| web | next dev :3000，rewrites → 后端 8080 | ✅ 首页/代理连通 |

### 10.2 本轮新发现并修复的真实缺陷（浏览器实测暴露）

| 编号 | 缺陷 | 根因 | 修复 |
|---|---|---|---|
| **B4-01 (CRITICAL)** | 注册/登录/下单全部失败 | 服务端代码（Server Action / Server Component）用 `fetch("/api/...")` **相对路径**，Node 服务端 fetch 报 `Failed to parse URL` | `lib/auth.ts`、`actions/auth.ts`、`actions/checkout.ts`、`lib/api.ts` 改用绝对 URL `FORGE_API_PROXY_TARGET` |
| **B4-02 (CRITICAL)** | 商品详情页 `/product/[sku]` 404 | `lib/catalog.ts` 被服务端组件调用但用相对路径 `/api`，服务端 fetch 失败 → 返回 null → notFound() | `catalog.ts`/`community.ts`/`coupons.ts` 用 `typeof window` 区分环境：服务端绝对 URL、客户端相对路径 |
| **B4-03 (HIGH)** | 非 zh 语言下首页仍显示中文「商品橱窗/立即购买」 | `product-grid.tsx`/`product-card.tsx` 硬编码中文，未接 i18n | 接入 `useTranslations("shop")`，4 语言补齐 `all/source/tutorial/card/resource/searchPlaceholder/unavailable/noMatch` key |
| **B4-04 (MEDIUM)** | 移动端 375px 下商品详情页横向溢出 4px | 评论/问答输入区 `flex` 无 `flex-wrap`，星星+输入框+按钮挤爆 | 加 `flex-wrap` + `min-w-0` |
| **B4-05 (环境)** | sqlx 连 PGlite 报 `prepared statement already exists` | PGlite(WASM PG) 是单 backend 多路复用，prepared statement 命名空间全局共享，且连接断开不清理残留；sqlx 每连接 `next_statement_id` 从 `sqlx_s_1` 重新计数，撞上旧残留 | ①去掉错误的 `?statement-cache-capacity=0`（该参数反而让 sqlx 从不 Close statement，每 10s 泄漏一个名字）；②`db.rs` 加 `after_connect` 用 `Executor::execute("DEALLOCATE ALL")` 走 simple protocol 清空残留；③支持 `FORGE_DB_MAX_CONNS=1`（本地） |

### 10.3 浏览器真实 e2e 测试结果（Playwright + Chrome，24 项全过）

| 功能模块 | 验证点 | 结果 |
|---|---|---|
| 商品目录 | 首页渲染、8 商品、分类筛选 | ✅ |
| 认证 | 注册、登录、登录态导航显示邮箱 | ✅ |
| 商品详情 | 商品名、评价区、问答区 | ✅ |
| 社区-提问 | 提交提问并显示 | ✅ |
| 社区-评论 | 已购用户提交评论并显示 | ✅ |
| 社区-收藏 | 收藏按钮态切换 | ✅ |
| 会员中心 | 等级卡片渲染 | ✅ |
| 结算 | 金额 ¥199.00、6 种支付方式 | ✅ |
| 优惠券 | 领券 → 结算页显示 WELCOME20 | ✅ |
| 通知中心 | 下拉面板打开 | ✅ |
| 我的文件 | 页面渲染 | ✅ |
| 多语言 | zh/en/ja/ko 四语正确 | ✅ |
| 鉴权拦截 | 未登录访问结算页被拦截 | ✅ |
| 移动端 | 无横向溢出 | ✅ |

**结果：24 PASS / 0 FAIL，控制台 0 业务错误。**

### 10.4 后端完整链路（curl 实测）

- 注册 → 登录 → `/auth/me`（Bearer token）：✅
- 下单 `/internal/orders`（free 无折扣）：`amountMicro=199000000` ✅
- 升级 pro → 下单（20% 折扣）：`amountMicro=159200000, originalAmountMicro=199000000, discountPct=20` ✅
- 领券 → 带券下单：`amountMicro=179000000, couponDiscountMicro=20000000` ✅
- admin 鉴权：无 token 401 / 正确 token 200 ✅

### 10.5 遗留项（如实标注）

- **支付回调 webhook 端到端**（支付宝 RSA2 验签等）需真实网关密钥，本地无法模拟，未实跑。代码路径已静态核验，不声称通过。
- **next-intl 3.26.0 在 Next 15 dev 模式的 `headers()` 同步访问警告**：第三方库兼容性警告，不影响功能（所有页面 200），生产构建不出现。
- **favicon 404**：项目无 favicon.ico，属资源缺失，非功能缺陷。

---

## 11. 安全专项审查与修复（Round 5，本轮全盘实测）

> 背景：上一轮完成功能验收后，本轮聚焦**安全**。逐文件静态审查全部 8 个 API 模块 +
> main.rs/config.rs，并编写 `e2e/security-audit.mjs`（44 项攻击用例）与
> `e2e/xss-browser.mjs`（6 项浏览器 XSS 实测）真实攻击验证。

### 11.1 静态审查结论（安全基线良好）

| 维度 | 结论 |
|---|---|
| 鉴权 | 用户侧统一 `user_id_from_token`（SHA-256 存 token）；admin 侧 `guard_admin` 恒定时间比较；无 IDOR（越权结算/下载均按归属校验） |
| 密码学 | Argon2id 哈希；256-bit 随机 token；防用户枚举（统一 401） |
| 注入 | 全部 SQL 参数化绑定（含动态 where 拼接也是 `$n` 占位符）；SKU/搜索无注入 |
| 金额 | 全程 Decimal/BigInt 零浮点；下单不信任客户端传价 |

### 11.2 本轮新发现并修复的安全缺陷（真实攻击复现）

| 编号 | 严重度 | 漏洞 | 根因 | 修复 |
|---|---|---|---|---|
| **B4-06** | 🔴 CRITICAL | **路径穿越写文件**：admin 上传文件名 `../../../evil.txt` 逃逸 uploads 目录，实测写到了 `apps/evil-escape.txt` | `upload_file_meta` 直接 `dir.join(file_name)` 未过滤 `../` | 新增 `sanitize_file_name()` 取 basename + 拒绝 `:`/`/`/`\`/`..`，`membership.rs` |
| **B4-07** | 🔴 HIGH | **下单数量溢出**：`quantity=2147483647` 下单成功，`amountMicro=4.27 亿`（i32 累加溢出 + 金额无上限） | `create_order` 无数量上限校验 | 加 `MAX_ITEMS=50`、`MAX_QTY_PER_SKU=999` 上限，`internal.rs` |
| **B4-08** | 🔴 HIGH | **并发领券竞态超领**：同用户并发 3 次领券全部成功（应限 1 次） | `claim_coupon` 的 `count(*)` 检查与 `INSERT` 无锁；且 `uk_user_coupon UNIQUE(user_id,coupon_id,id)` 因含自增 `id` 形同虚设 | 改事务 + `SELECT ... FOR UPDATE` 锁券行串行化，`promotion.rs` |
| **B4-09** | 🔴 HIGH | **免费升级会员**：`POST /user/profile` 无需支付即可升级 pro（20% 折扣） | `upgrade_tier` 直接改 `tier_code`，前端「升级」按钮直接调用 | 端点改返回 403 引导走支付；前端按钮改跳商品橱窗；升级走 webhook 自动晋升（`total_spent>=100` 升 plus）或 admin 通道 |
| **B4-10** | 🟡 MEDIUM | **评论/提问无长度限制**（DoS 刷库） | `create_review`/`create_question` 未限长度 | 加 2000 字符上限，`community.rs` |
| B4-11 | 🟡 记录 | **CORS 中间件缺失**：后端无 `CorsLayer` | 架构上前端经 Next 代理，浏览器不直连后端，当前影响有限；但若后端直暴露公网需补 | 记录在案，未改（避免引入配置复杂度） |

### 11.3 安全验收测试结果

**后端攻击面测试（`security-audit.mjs`）：44 PASS / 0 FAIL**

覆盖：
- 鉴权越权：未认证 401 ×4、伪造 token 401、admin 无/错/用户 token 均 401、越权提权 401、越权结算 404
- 金额/业务：传价攻击忽略、负/零/超大数量拒绝、空车/坏 SKU 拒绝、免费升级 403、满减券 199→179 正确
- 注入：SQL 注入参数化、DROP TABLE 尝试不破坏库、SKU 路径穿越 404
- 会话：token 256-bit、登出失效、防枚举、不泄露密码哈希
- 路径穿越：`../` 净化、Windows 盘符净化（修复后）
- 竞态：并发领券成功数=1（修复后）
- 信息泄露：公开商品不泄露库存、评论不泄露 email

**前端 XSS 实测（`xss-browser.mjs`）：6 PASS / 0 FAIL**

注入 `<img onerror>` / `<script>alert()` 恶意评论与提问 → React 正确转义，脚本未执行、无弹窗、DOM 无注入元素。

### 11.4 回归验证

- `cargo test`：36 passed（含 float_policy 门禁 PASS）
- `cargo build`：0 错误
- `npx tsc --noEmit`：0 错误
- 后端注册/登录/下单/领券/评论/提问全部实测 200/400/403 符合预期

### 11.5 剩余遗留项（如实标注）

- **支付回调 webhook 端到端**（支付宝 RSA2 验签）需真实网关密钥，本地无法模拟。
- **会员付费购买流程**：升级已改为支付流程，但「会员商品 SKU + 下单支付 + 自动晋升」的完整闭环尚未接入商品目录（当前 webhook 仅按累计消费自动升 plus，无独立 pro 商品）。属产品功能扩展，非安全缺陷。
- **CORS**：后端直暴露公网前需补 `CorsLayer`。

---

## §12 后台管理 500 崩溃根治（prepared statement 冲突）

### 12.1 现象

后台管理所有页面点开均报 `加载失败 请求失败 (500)`，后端日志：

```
error returned from database: prepared statement "sqlx_s_85" already exists
```

### 12.2 根因（三层递进，最终定位）

1. **表层**：`.env` 的 `FORGE_DATABASE_URL` / `FORGE_DB_MAX_CONNS` 未生效 —— 因为 `config.rs` 用 `env::var()` 直接读系统环境变量，而 `config` 无 dotenv 加载；当前运行的后端进程是直接 `target\debug\eduforge-core.exe` 启动，未经过 `start-rust-core.sh` 的 `source .env`。

2. **中层**：即使补上 dotenv，`?statement-cache-capacity=0` 这个"修复"本身是**错误的**。该参数让 sqlx 的 `StatementCache.is_enabled()=false`，导致 `get_or_prepare` 里的 Close 分支永不执行 —— sqlx 每次执行同一 SQL 都**新建命名 statement 且从不关闭**，在 PGlite 端无限泄漏名字（实测 outbox flush 每 10s 泄漏一个 `sqlx_s_N`，数小时累积数百个）。

3. **深层（真正根因）**：PGlite 是**单用户/单连接**的 WASM PostgreSQL 构建，`pglite-socket` 只是把多个逻辑客户端多路复用到同一个 backend（所有连接共享同一 `pg_stat_activity` 里的一个 backend）。sqlx 每个新连接的 `next_statement_id` 都从 `sqlx_s_1` 重新计数，而 PGlite 端因连接断开**不清理**残留的 prepared statement，导致新连接的 `sqlx_s_8` 撞上旧连接的 `sqlx_s_8`。

### 12.3 修复

| 文件 | 改动 |
|---|---|
| `apps/rust-core/Cargo.toml` | 加 `dotenvy = "0.15"` 直接依赖 |
| `apps/rust-core/src/main.rs` | `from_env()` 前加 `dotenvy::dotenv()`，并打印 `db config loaded` 日志 |
| `apps/rust-core/src/db.rs` | 加 `after_connect` 回调，用 `Executor::execute("DEALLOCATE ALL")` 走 simple query protocol 清空每个新连接建立的 PGlite 残留 statement |
| `apps/rust-core/.env` | **去掉错误的 `?statement-cache-capacity=0`**，恢复默认 statement cache |
| `apps/rust-core/.env.example` | 补 `FORGE_DB_MAX_CONNS` 说明 |

### 12.4 验证（全部真实实测）

- 后端 admin 接口连续 3 轮 × 9 接口 = 27 次全部 200，无 prepared statement 冲突
- 前端 `/api-admin/admin/*` 代理链路 6 接口全部 200
- 前端 7 个 admin 页面（overview/products/orders/card-keys/files/fraud/stats）全部 200
- **60 秒稳定性压测**：100 次请求 0 失败
- **炸弹场景验证**：停后端保留 17 个残留 statement → 重启后端 → `after_connect` 自动 `DEALLOCATE ALL` 清空，新连接从 `sqlx_s_1` 干净起步，不再撞车
- prepared statement 数量稳定在 17（对应 17 种不同 SQL），不再无限增长
- `cargo test`：36 passed / 0 failed（改动无回归）

### 12.5 结论

这是 PGlite 作为"真 Postgres 替代品"的**已知边界**（Firebase Data Connect 团队亦有相同结论："pglite has numerous limitations & bugs with prepared statement protocol"）。本修复通过「正确 statement cache + 连接建立时 DEALLOCATE ALL」双保险，让本地 PGlite 环境稳定运行。**生产环境必须使用真 PostgreSQL**（无此问题）。

---

## §13 会员中心/我的文件 401 修复 + 套餐价显示 bug

### 13.1 现象

未登录时点开「会员中心」「我的文件」报 `请求失败 (401) {"error":"unauthorized"}`；会员中心套餐价显示错误（¥99 → ¥99,000,000.00）。

### 13.2 根因

1. **401**：前端 `/api-user/*` 代理对**所有**用户侧请求强制要求 `forge_session` cookie，未登录一律 401。但后端 `/user/tier`（套餐列表）是**公开端点**（无需鉴权），却被代理错误地拦下。同时前端 `MembershipView`/`DeliveriesView` 组件对未登录态**无优雅降级**，直接把 401 错误文案渲染成红色报错条。

2. **价格错误**：`membership.rs` 的 `micros()` 函数对 `price_micro`（库内已是微单位，如 99000000 = ¥99）又乘 1,000,000，导致价格放大 100 万倍。

### 13.3 修复

| 文件 | 改动 |
|---|---|
| `app/api-user/[...path]/route.ts` | 加公开端点白名单 `PUBLIC_PATHS`（`/user/tier`），无 token 也放行转发 |
| `lib/membership-api.ts` | `userRequest` 对 401 抛 `status=401` 的明确错误（供组件判断） |
| `components/membership/membership-view.tsx` | 未登录时 `fetchProfile` 静默降级为 null，显示「登录后可查看」引导；`fetchTiers` 公开可看 |
| `components/deliveries/deliveries-view.tsx` | 未登录时显示「登录后可查看」引导，不再报 401 |
| `locales/{zh,en,ja,ko}.json` | membership/deliveries 各加 `loginToView`、`login` key |
| `apps/rust-core/src/api/membership.rs` | `micros()` 去掉 ×1,000,000（字段已是微单位），修复套餐价显示 |

### 13.4 验证（真实实测）

- 未登录访问 `/api-user/user/tier` → 200（公开端点放行）；`/api-user/user/profile` → 仍 401（需登录）
- 浏览器未登录访问会员中心：显示套餐列表（Free/Plus/Pro）+ 登录引导，**无 401 错误**
- 浏览器未登录访问我的文件：显示登录引导，**无 401 错误**
- 登录后（带 cookie）：会员中心显示「当前等级 Free / 累计消费 ¥0.00」，套餐价 ¥99.00/¥299.00 正确
- `cargo test` 36 passed / 0 failed；`tsc --noEmit` 0 错误

### 13.5 遗留项（如实标注）

- `total_spent_micro` 字段单位与 `price_micro` 不一致：webhook 累加 `cb_amount`（元）到 `total_spent_micro`（字段名 micro），阈值 `100000000` 注释写"100 元 = 100000000 微单位"存在矛盾。当前库内无消费数据（均为 0）无法实测，属历史单位 bug，需在接入真实支付回调时一并核对（涉及 webhook.rs 累加逻辑 + 晋升阈值）。
