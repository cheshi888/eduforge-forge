# 代码审查报告 (Round 2) — 2026-09-11

> 上轮报告: `docs/CODE_REVIEW_20260910.md` (14 issues, 13 fixed, H-09 open)。
> 本轮为**全仓库复审 + 全文件覆盖审查**, 编号 R2-* 从上轮未覆盖区域与新增代码出发。
> 所有 file:line 均基于 2026-09-11 实读; 未实跑任何服务, 行为结论以代码路径推断为准。
> 本轮仅审查, 修复 diff 为**建议**, 未落盘。

## 0. 审查范围与覆盖

| 模块 | 文件数 | 覆盖 | 备注 |
|---|---|---|---|
| apps/rust-core/src | 19 | 19/19 | webhook/internal/consumer/main/domain/db/events/config + 6 网关 + cardacq + gateway/mod |
| apps/go-plugins | 6 | 6/6 | main/loader/types/builtins + 2 个 cmd |
| apps/web | 19 | 16/19 | 未审低价值文件: tailwind.config.ts / tsconfig.json / package-lock.json / globals.css |
| schema | 1 | 1/1 | 001_init.sql 全文 |
| k8s | 4 | 4/4 | core-deployment / hpa / mutating-webhook / envoy-ingress-hmac |
| docker-compose.yml | 1 | 1/1 | |
| scripts | 2 | 2/2 | check-security-policy.sh / e2e-webhook.sh (后者上轮已建) |
| docs/architecture.md | 1 | 1/1 | 作为 6 专项依据 |

**47/50 文件实读**; 3 个未覆盖均为纯样式/锁文件, 不影响安全结论。

---

## 1. BLOCKER

### B2-01 币种不一致分支写非法 status, 触发 DB CHECK 违例 → 回调 5xx 循环 + 风控告警盲区

- 证据 1: `apps/rust-core/src/api/webhook.rs:220` `mark_txn(&mut db, gateway, "CURRENCY_MISMATCH", txn)`
- 证据 2: `schema/001_init.sql:60` `CHECK (status IN ('RECEIVED','MISMATCH','DUPLICATE','SETTLED','REJECTED'))` — 无 `CURRENCY_MISMATCH`
- 证据 3: `webhook.rs:211-219` 冻结订单的 UPDATE 与 `webhook.rs:370-379` mark_txn 的 UPDATE 均为 `.ok()` 吞错
- 攻击/故障场景: 任意币种不一致的合法或恶意回调 (如 CNY 订单收到 USD PayPal 回调) → `UPDATE ... SET status='CURRENCY_MISMATCH'` 抛 CHECK 违例 → 事务 aborted → `webhook.rs:221` commit 失败 → 提前 `return ProcessOutcome::Failed` → **`webhook.rs:224-237` 的 fraud_alert 永远不会发布** → 网关收到 5xx 指数退避重推, 每轮都重演同一失败。结果: 资金安全 (未入账), 但 (a) 该攻击/故障在风控流上零告警, (b) 网关侧持续重试风暴, (c) 订单实际冻结与否取决于 `webhook.rs:211` 的 UPDATE 是否先于 mark_txn 成功——两者都在同一 aborted 事务里, **冻结也随 commit 失败一起回滚**, 订单停留在 PENDING。
- 影响: 币种攻击/故障路径完全失效, 观测盲区 + 订单既未冻结也未拒收, 等待网关重试直到放弃。
- 建议修复 diff (二选一, 推荐 a):
  ```diff
  # a) schema/001_init.sql:60
  -         CHECK (status IN ('RECEIVED','MISMATCH','DUPLICATE','SETTLED','REJECTED')),
  +         CHECK (status IN ('RECEIVED','MISMATCH','CURRENCY_MISMATCH','DUPLICATE','SETTLED','REJECTED')),
  ```
  ```diff
  # b) webhook.rs:220 复用既有词表
  -         mark_txn(&mut db, gateway, "CURRENCY_MISMATCH", txn).await;
  +         mark_txn(&mut db, gateway, "MISMATCH", txn).await; // reason 以 fraud_alert.reason 区分
  ```
  无论哪种, 建议同时把 `webhook.rs:211-219` 的 `.ok()` 改为显式 `?` 语义 (至少 `err!` 日志 + 保留 commit 失败前发布告警的兜底队列, 见 M2-02)。
- 回归测试: 单测 (需 DB 或集成测试) 构造 CNY 订单 + USD 回调, 断言 (1) 响应 200 + ACK, (2) orders.status='FRAUD_SUSPENDED', (3) `payment_transactions.status='CURRENCY_MISMATCH'`, (4) NATS `fraud.events.alert` 出现 1 条 `reason="currency_mismatch"`。

---

## 2. CRITICAL / HIGH

### C2-01 Braintree 槽位缺事件类型白名单 (Alipay/Wechat/Stripe/Coinbase/PayPal/Adyen 均已有)

- 证据: `apps/rust-core/src/gateway/cardacq/braintree.rs` 全文 — 解析 `BT-Webhook-Id` + keyed HMAC 后直接构造 `GatewayTransaction`, **无 kind 校验**; 同槽位 `cardacq/adyen.rs:95-104` 有 `event_code` 白名单 (AUTHORISATION|CAPTURE)。
- 攻击场景: Braintree webhook 对 `transaction.voided` / `transaction.refunded` / `dispute.opened` 使用同一投递端点。`voided` 事件 (交易从未扣款成功) 被入账 PAID → 白嫖课程; 已退款订单再收到 refund 事件被二次处理 (状态机挡二次入账, 但首次 voided 入账是净损失)。
- 修复 diff:
  ```diff
  + // 仅扣款成功类事件可入账
  + match json["kind"].and_then(Value::as_str) {
  +     Some("transaction.submitted_for_settling") | Some("transaction.settled") | Some("transaction.settling") => {}
  +     Some(other) => return Err(GwError::EventRejected(format!("braintree kind {other} not whitelisted"))),
  +     None => return Err(GwError::EventRejected("braintree kind missing".into())),
  + }
  ```
  (kind 枚举以 Braintree 现行 webhook schema 为准, 需沙箱实测确认字符串, 见 §6 不确定项)
- 回归测试: `POST /webhooks/cardacq` 投递 signed `kind=transaction.voided` 事件, 期望 400/拒收 + 订单不动作; 投递 `kind=transaction.settled` 期望 200 + PAID。

### C2-02 支付宝缺 notify_type 白名单: 退款/改价通知同 URL 送达即入账

- 证据: `apps/rust-core/src/gateway/alipay.rs:61-81` — verify() 校验 RSA2 签名后仅取 `out_trade_no/total_amount/trade_no/currency`, 对 `notify_type`、`trade_status` **零校验**。同 URL 下支付宝会推送 `trade_refund`、`trade_modify` 等 notify_type。
- 攻击场景: 用户支付 100.00 后申请退款, 支付宝推送 `notify_type=trade_refund` → 本代码以 total_amount=100.00 入账 PAID → 钱退了货照解锁 (状态机对已 PAID 订单再迁 PAID 会走 Duplicate, 但首次退款前若订单处于 PENDING 窗口, 退款通知先行即可入账)。
- 修复 diff:
  ```diff
  + let nt = pairs.get("notify_type").map(|s| s.as_str()).unwrap_or("trade_status_sync");
  + if nt != "trade_status_sync" {
  +     return Err(GwError::EventRejected(format!("alipay notify_type {nt} not whitelisted")));
  + }
  ```
- 回归测试: signed 回调 `notify_type=trade_refund` 期望 EventRejected; `notify_type=trade_status_sync&trade_status=TRADE_SUCCESS` 期望 200 + PAID。

### C2-03 Coinbase 未知链网络 → 确认数校验整体旁路, 0 确认直接入账

- 证据 1: `apps/rust-core/src/gateway/coinbase.rs:33-38` 网络字符串只映射已知枚举 (TRC-20/ERC-20/… 白名单), 未知值 → `crypto_network: None`
- 证据 2: `apps/rust-core/src/api/webhook.rs:283` `if let (Some(conf), Some(network)) = (txn.confirmations, txn.crypto_network)` — `None` 时**整段确认数校验被跳过**, 直接走 `webhook.rs:320` PENDING→PAID。
- 攻击场景: 回调声明 `network="SOLANA"` (或任意新链/拼错值) + `confirmations: 0` → 绕过 "BTC≥6 / TRC-20/ERC-20≥1" 阈值, 0 确认入账, 随后被链上重组撤销 (见 TC-22 reorg 场景)。
- 修复 diff (两条都要):
  ```diff
  # coinbase.rs: 未知网络直接拒收
  - _ => ...,            // (现状: 未知 → None)
  + _ => return Err(GwError::EventRejected(format!("coinbase network {net} unsupported"))),
  ```
  ```diff
  # webhook.rs:283 兜底: 加密货币订单 (gateway=coinbase) 若 crypto_network 仍为 None → REJECTED + fraud alert
  ```
- 回归测试: `POST /webhooks/coinbase` 声明 `network="FOO"` 期望拒收; 声明 TRC-20 + confirmations=0 期望 PENDING_BALANCE。

### H2-01 跨境 FX 通道功能性断链: CNY 订单 + USD 回调必被冻结, 且 locked_exchange_rate 无写入路径

- 证据 1: `webhook.rs:205` 币种核对先于金额核对 (C-01), CNY 订单收到 USD 回调 → FRAUD_SUSPENDED。
- 证据 2: 全仓 grep `locked_exchange_rate` 仅 6 处命中, 全部是**读** (webhook.rs:149 / internal.rs:45,59,87 / events.rs:16 / consumer.rs:192), 无任何写路径 → `schema/001_init.sql:40` 该列恒 NULL。
- 证据 3: 前端 `apps/web/components/checkout/checkout-form.tsx:31-37` 的 `rateDisplay` 依赖 `order.lockedRateUsd`, 恒为 null → 汇率展示永不渲染; `lib/amount.ts:27` 的 `lockedUsd` 成为死路径。
- 结论: "下单时锁定美元锚定汇率防汇率波动少付" (schema L40 注释) 的设计目标当前**不可达**: 要么 FX 订单必须以 USD 币种创建 (则 `FX_METHODS` 展示 USD 正确, 但 CNY 订单用户走 PayPal/Stripe 被冻结), 要么结算时按锁定汇率折算 `cb_amount × rate` 再比对 (则需新增写路径 + 校验逻辑)。需产品确认取哪条 (见 §6)。
- 回归测试: 见 TC-23 (预期按现状 = 冻结, 标"暴露 H2-01")。

### H2-02 k8s 核心部署缺 6 组网关/收单 env, wechat 在 k8s 恒 fail-closed

- 证据: `k8s/core-deployment.yaml:37-82` 仅下发 alipay/stripe/coinbase/braintree/paypal; 而 `apps/rust-core/src/config.rs:34-43` 还读取 `FORGE_WECHAT_PLATFORM_CERT_PEM` / `FORGE_WECHAT_API_V3_KEY` / `FORGE_BRAINTREE_MERCHANT_ID` / `FORGE_CARD_ACQUIRER` / `FORGE_ADYEN_HMAC_KEY` / `FORGE_ADYEN_MERCHANT_ACCOUNT`, 全部缺失 → wechat 验签 `from_public_key_pem("")` 失败恒拒 (wechat.rs:33-36); adyen 无 HMAC key → adyen 槽位 fail-closed; `card_acquirer` 缺省 braintree, 若要切 adyen 需补 `FORGE_CARD_ACQUIRER=adyen`。
- 修复 diff: core-deployment.yaml env 段追加 6 个 secretKeyRef (wechat 两 key 来自 `eduforge-gateways`, optional+default 保持 fail-closed 语义)。
- 回归测试: 起 k8s 集群后 `curl -X POST http://…/webhooks/wechat` 合法签名事件, 期望 200 而非恒 400 (需环境)。

### H2-03 Envoy 18080 全路径转发: 外部可达 /internal/*, 越权读任意用户订单 + 恶意结算

- 证据: `k8s/envoy-ingress-hmac.yaml:42-44` `routes: - match: { prefix: "/" } → cluster core:8080` — 无路径限制。
- 信任边界: `rust-core/src/main.rs` 路由面含 `/internal/orders/preview` (internal.rs:16-18, 仅需 `?uid=` 参数) 与 `/internal/checkout` (internal.rs:19, 仅需 uid+out_trade_no)。设计意图 "集群内 /internal/* 直连 8080, 外部必须走 18080" (mutating-webhook.yaml:93-94) 实际**未能隔离**: 外部调用者经 18080 → envoy 自动补 x-forge-signature → core 放行 → `GET /internal/orders/preview?uid=<受害者>` 返回其 PENDING 订单 (单号/金额/币种/汇率, 见 internal.rs:56-89); 攻击者还可替任意 uid 提交 checkout。
- 修复 diff:
  ```diff
  # envoy-ingress-hmac.yaml:42
  -                             - match: { prefix: "/" }
  +                             - match: { prefix: "/webhooks/" }
  ```
  并在 core 侧加 /internal 网络层防护 (仅允许同 Pod 18080 之后的回环/集群内来源, 或要求 sidecar 注入的 source header)。
- 回归测试: 从集群外 `curl http://ingress:18080/internal/orders/preview?uid=u-victim` 期望 404/403 (需 k8s 环境)。

### H2-04 go-plugins /discount/preview 公开无鉴权 + 插件结果无边界校验 + 无 RPC 超时 + int64 溢出

- 证据 1: `apps/go-plugins/main.go:68-70` — `POST /discount/preview` 无 `adminAuth` (对比 L68 上传通道已 fail-closed)。
- 证据 2: `main.go:147-168` — 插件返回的 `FinalMinor/DiscountMinor` **未做 0≤final≤price×qty 校验**直接回显; 一个有 bug 的已上传插件可输出负数/超大折扣价。
- 证据 3: `internal/promo/loader.go:120` `ap.remote.ApplyDiscount(req)` — net/rpc 无客户端超时, 插件挂起 = 每请求一个永久挂起 goroutine, 公开端点可被 DoS 循环放大。
- 证据 4: `internal/promo/builtins.go:14` `req.PriceMinor * int64(req.Qty)` 与 `L32` 同型 — 输入仅校验非负 (`main.go:152`), 无上限; int64 回绕可产生负 FinalMinor。
- 修复 diff:
  ```diff
  # main.go: preview 加 adminAuth 或限流; Apply 结果校验
  + if res.FinalMinor < 0 || res.DiscountMinor < 0 || res.FinalMinor+res.DiscountMinor > req.PriceMinor*int64(req.Qty) {
  +     c.JSON(http.StatusBadRequest, gin.H{"error": "plugin returned out-of-bound price"}); return
  + }
  # loader.go: 客户端超时
  - res, err := ap.remote.ApplyDiscount(req)
  + rc := rpcClientWithTimeout(...); // 或 context 化 gRPC, 5s 超时
  # builtins.go: 溢出检查
  + if req.Qty > 0 && req.PriceMinor > math.MaxInt64/int64(req.Qty) { return 错误 }
  ```
- 回归测试: `curl -X POST :3001/discount/preview -d '{"price_minor":9000000000000000000,"qty":2}'` 期望 400/边界错误而非负数; 上传一个 sleep 插件后并发 100 请求期望 5s 超时不堆积。

### H2-05 并发超卖无任何防护: products.stock 与 Redlock 均为纸面

- 证据 1: `schema/001_init.sql:22` `products.stock BIGINT` 列存在。
- 证据 2: `docker-compose.yml:25` 注释 "Redlock 防并发超卖依赖此实例" + redis 服务 L22-35 存在。
- 证据 3: 全仓 grep `stock|redis|redlock|inventory` 于 apps/rust-core → **0 命中**; 无任何订单创建/扣减库存代码路径 (订单由外部播种, checkout 仅锁 gateway_type, internal.rs:116-128)。
- 结论: "并发超卖" 场景当前**无服务端保护**, TC-20 将暴露 (预期结果 = 超卖发生)。属已知缺口, 需产品定夺实现路径 (Redis Redlock 或 DB `UPDATE products SET stock = stock - n WHERE stock >= n` CAS)。
- 回归测试: 同 SKU stock=1, 并发 50 个下单, 期望 ≤1 成功 (需环境, 预期失败 → 标记 H2-05)。

### M2-09 (升级为 HIGH) B-02 持续开放: 确认数补齐链路无真实 Provider 注入开关

- 证据: `apps/rust-core/src/main.rs:45` 恒装配 `StubConfirmationProvider`; `consumer.rs:14-27` Stub 恒返回 last_stored 值 → PENDING_BALANCE 订单的 backfill **永远不会推进到 PAID**。
- 影响: "未达确认数只入账 PENDING_BALANCE, 由消费者异步轮询补齐后再结算" 的设计 (webhook.rs:280-282 注释) 当前是断头路, 真实链上支付在确认补齐前永久滞留。
- 建议: env 开关 (如 `FORGE_CONFIRMATION_PROVIDER=stub|rpc` + RPC endpoint), 或至少把 Stub 的行为差异以 metric 暴露。修复属新功能, 本轮不实施。

---

## 3. MEDIUM

| ID | 位置 | 问题 | 建议 |
|---|---|---|---|
| M2-01 | stripe.rs:59 / coinbase.rs:40 / paypal.rs:52 / wechat.rs:100 | `gateway_txn_id` 缺失时 `unwrap_or("")` → uk_txn 退化为 ("", gateway) 联合键, 两个不同空 ID 流互斥去重, 合法回调被当重放丢弃 | 对齐 adyen/braintree: 缺失即 `GwError::MissingField` 拒收 |
| M2-02 | events.rs:71-77 | JetStream 发布失败仅 warn, fraud/paid/confirm 事件在 NATS 故障期静默丢失, webhook 层仍返回 ACK | 引入 DB outbox 表或按 subject 落本地持久队列, 恢复后补发 |
| M2-03 | webhook.rs:332-337 | 状态机阻断 (如 Adyen AUTHORISATION 后 CAPTURE) 时把**未结算的重复事件** mark 为 `SETTLED` → 审计语义失真 | 引入 `DUPLICATE` status (schema L60 已有词, 但当前无代码写入, 正好用上) |
| M2-04 | db.rs (pool max 20) + hpa.yaml:13 (maxReplicas 12) | 12 副本 × 20 连接 = 240 > PostgreSQL 默认 max_connections=100, 扩容后连接池饿死 | pg 调参至 ≥300 或 pool max 降至 8 + minidle, 并加 acquire_timeout |
| M2-05 | web/lib/amount.ts:19,32 + scripts/check-security-policy.sh:14 | 展示层 `micro/1e6` 与 `(micro/1e6)*rate` 走 float64; CI 门禁只扫 rust+go, 前端豁免 → "金额全程禁浮点" 策略存在覆盖洞 | 展示换算改 BigInt/字符串十进制; 门禁加 `grep -rnE '1_?000_?000|float' apps/web/lib` 或 eslint 规则 |
| M2-06 | web/lib/api.ts:32-38 | 后端 4xx/5xx/网络故障一律静默回退 mock 演示订单 (FT-20260909-8f3a2c), 用户可能对不存在单号提交支付 | 生产 profile 禁用 mock (env 开关), 故障时显式渲染 "服务不可用" |
| M2-07 | internal.rs:117-121 + schema:42 | `state_version` 只 `+1` 从不作为 CAS 条件 → "乐观并发版本号" 名不副实 (实际并发安全靠 FOR UPDATE + 状态机) | 改为 `WHERE ... AND state_version = $n`, 或删列避免误导 |
| M2-08 | main.rs (无 /readyz) + core-deployment.yaml:90-101 | liveness 与 readiness 同打 /healthz; DB/NATS 故障时 liveness 仍绿 → K8s 永不摘流量, 故障期间回调 5xx 堆积 | 拆 /readyz (DB ping + NATS ping), liveness 仅进程存活 |

---

## 4. LOW / INFO

| ID | 位置 | 说明 |
|---|---|---|
| L2-01 | schema:60 vs webhook.rs:333 | status 词表含 `DUPLICATE` 但代码从不用; 与 M2-03 一并收口 |
| L2-02 | i18n/routing.ts:4 vs locales/ko.json vs schema:11 | FIXED (2026-09-12, 第五批): 根因是 `middleware.ts:7` matcher `/(zh|en|ja)/:path*` 漏 `ko`, 致 /ko* 路由 next-intl locale header 未设置 → 404。修法: matcher 补 `ko` → `/(zh|en|ja|ko)/:path*`。routing.ts 与 locales/ko.json 本身合法, 4 语言全部 200 验证通过 |
| L2-03 | stripe.rs:39 | `String::from_utf8_lossy(body)` 后验签: 非 UTF8 合法 Stripe 事件会被 lossy 转换 → 验签目标字节与 Stripe 原始字节可能不一致 → 拒收 (可用性, 非完整性) |
| L2-04 | mutating-webhook.yaml:17,69 | `matchPolicy: Exact` 在 k8s 1.27+ 弃用 (1.32 移除), 部署目标集群 ≥1.27 时需删除该字段 |
| L2-05 | webhook.rs:241 | 金额比对为**严格相等**: 多付 (overpayment) 同样冻结, 无自动退款路径 → 用户争议场景, 需客服退款流程兜底 (产品决策, 见 §6) |

---

## 5. 六项专项结论

1. **金额路径 (Decimal/整数微单位)**: 服务端达标 — NUMERIC(20,6) + rust_decimal (webhook.rs:193, internal.rs:74-80) + Go int64 micro (types.go:13-58, 全程整数)。缺口: 前端展示层 float64 (M2-05) 且不在 CI 门禁内; 折扣溢出 (H2-04)。
2. **幂等**: 三保险齐备 — `uk_txn(gateway_txn_id, gateway_type)` (schema:64) + `ON CONFLICT DO NOTHING` 早退 (webhook.rs:96-139) + 状态机 transition_to 唯一写入口 (domain/order.rs:42-53)。缺口: 空串 txn_id 退化 (M2-01) 与 B2-01 事务回滚副作用。
3. **HMAC 双层校验**: 入口 `x-forge-signature` 严格模式 (gateway/mod.rs:144-151, 缺失即拒) + 各网关独立验签 (stripe 300s 时间窗 / adyen base64 HMAC key / braintree BT-Signature keyed HMAC / paypal 空密钥 fail-closed 180s / wechat 平台证书 RSA-SHA256 300s / alipay RSA2 双 PEM 格式)。结论: 校验面完整; 弱点全在事件类型白名单 (C2-01/C2-02) 与 18080 路径泄漏 (H2-03)。
4. **唯一索引**: uk_txn + out_trade_no UNIQUE (schema:28) + uk_plugin_active (schema:77) + PENDING_BALANCE 部分索引 (schema:49)。唯一违例点 = B2-01 (CURRENCY_MISMATCH 不在 CHECK 词表)。
5. **区块链确认数**: 阈值 BTC≥6 / TRC-20,ERC-20≥1 (domain/order.rs Blockchain::required_confirmations), 未达标停 PENDING_BALANCE + NATS 轮询 (webhook.rs:283-311)。三处断点: 未知网络旁路 (C2-03)、StubProvider 永不补齐 (M2-09/B-02)、reorg 场景只增不减 (见 TC-22 预期)。
6. **回调成功串**: `ack` 按网关分化 (gateway/mod.rs:58-72) — alipay "success" / stripe "ok" / wechat `{"code":"SUCCESS","message":"成功"}` / coinbase `{"code":"SUCCESS","note":status}` / paypal "ok" / cardacq "ok"。异常: B2-01 分支返回 5xx → 网关无限重试。

## 6. 性能与可观测性缺口

- DB 连接池: connect_lazy + max 20, 无 min/idle/acquire_timeout/health-checks (db.rs); 与 HPA 12 副本冲突 (M2-04)。
- 可观测: rust-core 仅 `TraceLayer` 结构化日志, 无 /metrics 或 OTLP 端点 (go-plugins 有完整 OTel, 不对称); k8s 靠 sidecar otel 采集, 但 sidecar 是否真被注入取决于 H-09 (注入器镜像 `registry.example.com/eduforge/webhook-injector:1.0.0` 为占位, 未部署 → 两个 mutating webhook `failurePolicy: Ignore` → Pod 无 sidecar, 采集与入口签名双双缺席, 且 fail-closed 使全部回调被拒 — 见 TC-28)。
- NATS: publish 失败仅 warn (M2-02); consumer backoff 300s 纯 in-memory (consumer.rs), 重启丢失等待状态, 仅靠 backfill 补齐。
- HPA 仅 cpu/mem 资源指标, 无回调延迟/积压自定义指标; P95 200ms 目标 (hpa.yaml:1 注释) 无 SLO 支撑物。

## 7. 不确定项与需补材料

1. **H2-01 产品决策**: FX 订单按哪条路线 (USD 建单 vs 锁定汇率折算比对)? 需产品确认后才能给修复方案。
2. **C2-01 材料**: Braintree webhook `kind` 枚举全集 (voided/refunded/settled/…) 需沙箱实测或官方文档对齐。
3. **M2-09 材料**: 真实 ConfirmationProvider 的 RPC 端点/协议是否存在于别处仓库? 本轮仅见 Stub。
4. **H2-05 决策**: 超卖防护选 Redlock 还是 DB CAS? Redis 实例已在 compose 但无任何消费者代码。
5. **上轮遗留**: H-09 注入器未部署 (k8s 环境项); 网关沙箱凭据 (alipay 本地测试用临时密钥对, 非沙箱)。
6. **Stripe L2-03**: lossy UTF-8 场景是否可接受 (Stripe 官方 body 恒 UTF-8, 影响极小)。

## 8. 测试矩阵

见 `docs/TEST_MATRIX_20260911.md` (TC-16 起, 13 必含场景 + 6 网关 × 5 场景类 + 全系统覆盖表 + 需环境执行清单)。

## 9. 修复记录 (同日实施, 仅最高优先级批次)

| ID | 状态 | 改动 | 说明 |
|---|---|---|---|
| B2-01 | FIXED | `schema/001_init.sql:60` CHECK 词表补 `CURRENCY_MISMATCH` | 既有库需跑 `ALTER TABLE payment_transactions DROP CONSTRAINT payment_transactions_status_check; ALTER TABLE ... ADD CONSTRAINT ... CHECK (...同上...)` 同步词表 |
| C2-01 | FIXED | `gateway/cardacq/braintree.rs` 新增 kind 白名单 (transaction.settled/settling/submitted_for_settling) | kind 枚举全集待 Braintree 沙箱实测确认 (§7-2) |
| C2-02 | FIXED | `gateway/alipay.rs` 新增 notify_type 白名单 (仅 trade_status_sync, 缺失也拒) | 回归测试 L128 报文含 notify_type, 兼容; e2e-webhook.sh 三个报文均带该字段 |
| C2-03 | FIXED | `gateway/coinbase.rs` 未知网络改 `EventRejected` (不再映射 None) | webhook 层确认数校验的旁路随之关闭 |
| M2-01 | FIXED | stripe/coinbase/paypal/wechat 的 `gateway_txn_id` 缺失改 `MissingField` 拒收 | 4 处 diff, 与 adyen/braintree/alipay 的 required 语义对齐 |
| H2-03 | FIXED (配置层) | `k8s/envoy-ingress-hmac.yaml` 路由 `prefix: "/"` → `"/webhooks/"` | /internal/* 不再经 18080 可达; 网络层纵深防护 (source 限制) 未做, 遗留 |

未实施 (后续批次): H2-01 (需产品决策)/H2-05 (超卖防护路径选型)/M2-09 (真实 ConfirmationProvider 注入)。批次 2/3 见 §9.1/§9.2。编译与单测结果见 §10。

### 9.3 第四批修复 (同日, 决策已拍板)

| ID | 状态 | 改动 | 说明 |
|---|---|---|---|
| H2-01 | FIXED (方案 B) | `webhook.rs` 新增 `fx_comparable` 纯函数 + 折算比对分支; 跨币种且锁定 `locked_exchange_rate` 时按 `expected × rate` 折算到回调币种再比对, 无锁汇/锁汇=0 仍冻结 | FX 订单 (CNY 计价 + USD 网关) 不再误冻; 折算语义与架构"锁定汇率"一致。4 条单测覆盖 (同币种/跨币种有锁汇/无锁汇/锁汇 0) |
| H2-05 | FIXED (方案 A) | `webhook.rs` 结算前 `UPDATE products SET stock = stock - qty WHERE stock >= qty` CAS, 命中 0 行回滚冻结 | DB 级防超卖, 无新依赖; TC-52 期望从"全 PAID"改为"≤1 PAID"。库存列约束既有库需保证 stock≥0 |

**回归单测 (本批次新增 4 条, 全绿)**: `fx_comparable_*` 4 条 (H2-01 折算判定)。H2-05 CAS 为 SQL 运行时行为, 需 PG 环境实跑 TC-52。

### 9.1 第二批修复 (同日)

| ID | 状态 | 改动 | 说明 |
|---|---|---|---|
| H2-04 | FIXED | `go-plugins`: preview 加 adminAuth; `promo.SafeMul` 溢出检查; NoDiscount/SecondHalf 用 SafeMul; `host.Apply` RPC 客户端 5s 超时 + 回退 builtin; preview 结果边界校验 (final≥0, discount≥0, final+discount==原价, qty≤10000) | 公开端点 DoS 面 + 负价/超额折扣 + int64 回绕全部收口 |
| M2-04 | FIXED | `db.rs` max 20→8 + min 1 + acquire_timeout 5s | 12 副本 × 8 = 96 ≤ pg 默认 max_connections=100 |
| M2-07 | FIXED | `internal.rs` CheckoutReq 新增可选 `state_version`, 提供时 UPDATE 带 CAS 条件 | 向后兼容 (缺省 None 行为不变); web 前端暂不传, 后续可由 preview 下发 |
| M2-08 | FIXED | `main.rs` 新增 `/readyz` (pg SELECT 1 + NATS 裸 publish 探活), k8s readiness 打 /readyz, liveness 保留 /healthz | DB/NATS 故障时摘流量不重启 |

**回归单测 (本批次新增 12 条, 全绿)**:
- alipay: `refund_notify_type_rejected` / `missing_notify_type_rejected` (C2-02)
- coinbase: `unknown_network_rejected` / `missing_txn_id_rejected` / `known_network_accepted` (C2-03 + M2-01)
- braintree: `voided_kind_rejected` / `missing_kind_rejected` / `settled_kind_accepted` (C2-01)
- go-plugins `promo_test.go`: `SafeMul` 溢出 / NoDiscount 溢出 / SecondHalf 溢出 / 折扣守恒 4 条 (H2-04)

### 9.2 第三批修复 (同日)

| ID | 状态 | 改动 | 说明 |
|---|---|---|---|
| H2-02 | FIXED | `k8s/core-deployment.yaml` 补 6 组 env: WECHAT_PLATFORM_CERT_PEM / WECHAT_API_V3_KEY / BRAINTREE_MERCHANT_ID / CARD_ACQUIRER(缺省 braintree) / ADYEN_HMAC_KEY / ADYEN_MERCHANT_ACCOUNT, 全 optional + fail-closed | 网关配置面完整; 缺证书/密钥的网关在代码层仍拒收 (C2/M2-01 语义) |
| M2-05 | FIXED | `internal.rs` locked_rate_usd → `Option<String>` (Decimal 十进制串); `web/lib/amount.ts` 全量 BigInt 重写 (round2 整数舍入 + rateToMicro 字符串解析 + Intl 仅展示); `tsconfig` target → ES2022; `check-security-policy.sh` 加第 3 步 web 浮点门禁 | 金额契约从 f64 number 改十进制字符串, 前端 BigInt 运算, 全链路零浮点 |
| M2-06 | FIXED | `web/lib/api.ts` `fetchOrderSummary` 改返回 `OrderSummary \| null`; 故障显式渲染 "Payment service unavailable"; mock 回退仅 `FORGE_MOCK_FALLBACK=1` 时启用 | 生产不再静默展示演示单号 |
| M2-03 | FIXED | `webhook.rs` 状态机阻断分支 `mark_txn(..., "SETTLED")` → `"DUPLICATE"` | 未结算的重复/被阻断事件审计语义正确; 用上 schema L60 既有 `DUPLICATE` 词 (L2-01 一并收口) |
| M2-02 | FIXED | 新增 `event_outbox` 表 (schema) + `EventPublisher` 落库前置 + `flush_outbox` 周期补发 (main.rs 10s 驱动) | NATS 故障期 fraud/paid/confirm 事件不再静默丢失; at-least-once, 消费端幂等去重 |
| L2-01 | CLOSED | `DUPLICATE` 词由 M2-03 引入写入路径 | 词表与代码对齐 |

**回归与验证 (本批次)**: rust `cargo test` 11/11 PASS (M2-02/M2-03 为运行时/DB 行为, 补发与 flusher 语义需 NATS+PG 环境实跑, 标"需环境"); web `tsc --noEmit` 0 错误 + `check-security-policy.sh` 全 PASS (门禁第 3 步对 amount.ts 无命中); schema `event_outbox` 部分索引 `WHERE status='pending'` 支撑 flusher 扫描。


- 第五批次 (会员等级制度 + 内容/卡密交付 + 文件下载 + admin 上传绑定 + 自动晋升): `cargo test` **25 passed / 0 failed**, 0 warning (新增 membership/deliveries API + domain/membership 状态机 3 单测 + webhook 自动晋升); web `tsc --noEmit` **0 错误** (membership-view / deliveries-view / admin files-view 新增); i18n 4 语言 JSON valid。

## 11. 全链路真人走读复测 (2026-09-11, 第四批后)

全仓按"真人测试"顺序重走一遍 (编译 → 单测 → 门禁 → 逐网关验签/白名单 → webhook 运行时分支 → consumer → web↔rust 契约), 新发现并修复 2 处契约断裂:

| 新发现 | 位置 | 现象 | 修复 |
|---|---|---|---|
| C3-01 | `web/app/actions/checkout.ts` | 前端 UI 值 `card` 直发 `/internal/checkout`, rust `GatewayType::from_str_v` 只认 `cardacq` → 选信用卡 400 "unknown payment method" | web 侧下发前映射 `card→cardacq` (UI 文案不动) |
| C3-02 | `web/app/actions/checkout.ts` | 前端 UI 值 `usdt` 直发, rust 网关词表无 `usdt` (crypto 走 `coinbase` 槽位) → 选 USDT 400 | web 侧映射 `usdt→coinbase` |

**验证**: 修复后 web `tsc --noEmit` 0 错误; 全量 rust `cargo test` 15/15, go `build+vet+test` OK, 门禁 PASS。网关词表 (alipay/stripe/coinbase/wechat/paypal/cardacq) 与 web METHODS (alipay/stripe/usdt/wechat/paypal/card) 经映射后一一对齐, 无其余断裂。

### 11.1 C4-01 管理后台 admin API + 发货接线 + 风控持久化落地

管理后台 `/admin/*` 鉴权 API (商品 CRUD / 订单 / 卡密 / 风控 / 统计) + 发货接线 (consumer.rs `ship_order`: PAID→SHIPPED, card 类卡密池 AVAILABLE→USED 事务内 FOR UPDATE SKIP LOCKED CAS 分配, 耗尽回滚保持 PAID, 非 card 内容类直接 SHIPPED) + 风控事件落库 (`run_fraud` → fraud_events) 全部落地。

纯逻辑已抽为可测函数并补单测 (consumer.rs `decide_ship` 分流决策 + `dec_to_micro` 金额解析), `cargo build` 0 warning, 全量 `cargo test` **22/22 PASS** (新增 7 条: 4×decide_ship + 3×dec_to_micro)。

| 已测 (纯逻辑 ✅) | 需真实 PG/NATS (⏳ 需环境) |
|---|---|
| decide_ship: card 耗尽/分流/钳位 4 条 | 商品 CRUD / 订单发货 / 卡密生成 / 统计 实跑 |
| dec_to_micro: Number/String/拒绝 3 条 | fraud_events INSERT 实跑 |

详细断言见 `docs/TEST_MATRIX_20260911.md §5.6`。

### 11.2 C5-01 会员等级 + 内容交付 + 文件下载 (2026-09-12, 第五批)

- 等级模型 3 档 free/plus/pro, 升级方向 free<plus<pro (domain/membership.rs `Tier::can_upgrade_from`, 仅允许 `cur < to`, 降级/平级 400); 单次回归 3 条 (状态机 + DB 值往返)。
- 内容交付: NATS `run_paid` 路径按商品 category 分流 — 卡密类 `AVAILABLE→USED` 绑定订单 + `product_deliveries` 落库 (delivery_type=card_key); 内容类 source/tutorial/resource → delivery_type=file/content, 真实文件 URL 由 admin `uploaded_files` 登记绑定, 未绑定走占位; 耗尽回滚保持 PAID。
- 自动晋升: webhook 结算处累加 `users.total_spent_micro`, 跨阈值 (100 元=1e8 微单位) 且当前 free → 自动升 plus (tier_expires_at +365d)。
- 下载额度: `/user/deliveries/{id}/download` 按当前等级 `max_downloads` 约束, 超限 403, 下载计数 +1。
- admin: `/admin/deliveries/files` 登记交付文件元数据 (storage_key/checksum 留 MinIO 接口), `/admin/users/{uid}/tier` 手动调级 (续期/降级通道)。
- 前端: 会员中心 (`/membership`, 等级卡/权益/升级) + 我的文件 (`/my-files`, 交付物下载) + admin 交付文件子页; 顶栏加「会员中心」「我的文件」; i18n 4 语言 membership/deliveries/nav/admin 段。
- 金额全程 Decimal/微单位 int64, float_policy 门禁 `no_floating_point_in_any_source` PASS。live 实跑 (等级/升级/交付/下载/自动晋升) 需 PG+NATS 环境, 见 TEST_MATRIX §5.8/§5.9。

## 10. 验证

- 第一批次 (B2-01 等 6 项): `cargo test --jobs 1` **3 passed / 0 failed** (1m21s), EXIT=0。
- 第二批次 (H2-04/M2-04/M2-07/M2-08 + 12 条新回归单测): `cargo test --jobs 1` **11 passed / 0 failed**, EXIT=0; `go build ./...` + `go vet ./...` 通过; `go test ./...` internal/promo **ok** (4 条新单测); `scripts/check-security-policy.sh` **PASS** (float 门禁对新代码无违例)。
- 第三批次 (H2-02/M2-05/M2-06/M2-03/M2-02): `cargo test --jobs 1` **11 passed / 0 failed** (internal.rs String 契约改动 + outbox 落库均通过编译与既有单测); web `tsc --noEmit` **0 错误**; `check-security-policy.sh` **PASS** (第 3 步 web 门禁对 `amount.ts` 无命中)。M2-02/M2-03 为 NATS+PG 运行时行为, 补发与 DUPLICATE 写入需 live 环境实跑, 标"需环境"。
- 第四批次 (H2-01 方案 B / H2-05 方案 A): `cargo test --jobs 1` **15 passed / 0 failed** (新增 fx_comparable 4 条单测)。H2-05 库存 CAS 为 PG 运行时行为, 需 live TC-52 实跑验证 ≤1 单 PAID。
- 第五批次 (会员等级制度 + 内容/卡密交付 + 文件下载 + admin 上传绑定 + 自动晋升): `cargo test` **25 passed / 0 failed**, 0 warning (新增 membership/deliveries API + domain/membership 状态机 3 单测 + webhook 自动晋升); web `tsc --noEmit` **0 错误**; i18n 4 语言 JSON valid。
  - 顺带修复 2 个既有 web 结构缺口:
    - **L2-02 ko 死路由**: 根因 `middleware.ts:7` matcher `/(zh|en|ja)/:path*` 漏 `ko` → 致 /ko* 全部 404 (locale header 未设置)。修法: matcher 补 `ko`。
    - **根 layout 缺口**: `app/page.tsx` (根重定向) 无根 `app/layout.tsx`, `next build` 报 "page.tsx doesn't have a root layout"。修法: 新建根 `app/layout.tsx` 提供 html/body, `app/[locale]/layout.tsx` 去掉自身 html/body 改继承根。`next build` 全绿 (所有路由含 membership/my-files/admin/files 正常生成)。
- schema/envoy/k8s 为数据/配置层, 无编译影响; 既有库的 CHECK 迁移 SQL 见 §9 表内说明; `event_outbox` 表为新增, 既有库需跑 schema 追加段建表。

