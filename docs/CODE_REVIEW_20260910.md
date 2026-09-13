# EduForge-Neo 严格代码审查 + 模拟真人无死角测试报告

- 日期: 2026-09-10
- 范围: rust-core (Rust/axum/sqlx/nats)、go-plugins (Go 热插拔)、web (Next.js)、schema、k8s、docker-compose
- 方法: 全仓静态取证(行号级)+ 可执行项实测(cargo test / go build 热切换 / clippy-fmt) + 未执行项给精确复现命令
- 原则: 失败优先;金额全程 Decimal/整数微单位,禁止 f32/f64;回调金额只信本地 expected_amount;幂等 (gateway_txn_id, gateway_type) 联合唯一 + SELECT FOR UPDATE + 网关原生验签 + 入口 HMAC

---

## 0. !!! CRITICAL 风险速览(先于常规章节) !!!

| 排序 | ID | 一句话风险 | 证据(文件:行) |
|---|---|---|---|
| 1 | C-04 | cardacq 默认槽位 Braintree 验签 = 无密钥 sha256(公知数据),攻击者可自行计算伪造,且 k8s 默认无商户号核对【已修复: 官方 keyed HMAC(BT-Signature) + 空密钥 fail-closed】 | `apps/rust-core/src/gateway/cardacq/braintree.rs:19-23,60-63`(修复) + `k8s/core-deployment.yaml:67-75`(接线) |
| 2 | B-03 | go-plugins `/admin/plugins` 无任何鉴权 + `plugin_name` 路径穿越 → 任意文件写 + 热切换任意二进制 → RCE【已修复: token 鉴权 fail-closed + 名字白名单 + Abs 前缀校验】 | `apps/go-plugins/main.go:44,56,78`(修复) |
| 3 | C-09 | 支付宝 sign 先按 base64(STANDARD)解码:512 字符 hex 恰好是合法 base64 → 静默解出 384 字节垃圾 → 全部合法回调验签失败(业务 DoS)【已修复+回归】 | `src/gateway/mod.rs:137`(根因) + `src/gateway/alipay.rs:44-49`(修复) |
| 4 | C-01 | 金额比对无币种核对:同数值跨币种回调(FX 网关 USD 结算 vs CNY 订单)直接入账【已修复: webhook.rs 币种核对分支】 | `src/api/webhook.rs:182-215`(修复) |
| 5 | B-02 | `crypto.events.poll` 未达标不重排 + 失败即丢 + 重启悬挂 + Stub 固定点 → BTC 订单停在 PENDING_BALANCE(原判"零生产者"已更正: webhook 落 PENDING_BALANCE 已触发首轮 poll)【已修复闭环: 300s backoff 重排 + 30s 失败重试 + 启动回填; 真实链节点 Provider 列遗留】 | `src/consumer.rs:57-84,142-164`(修复) + `src/api/webhook.rs:279-282`(首轮生产者) |
| 6 | C-05 | PayPal webhook secret k8s 未接线 → 空密钥可计算 HMAC → 回调可伪造;且无 resource.type 过滤,退款事件也入账【已修复: 空密钥 fail-closed + 事件白名单 + k8s 接线】 | `src/gateway/paypal.rs:14-16,41-47`(修复) + `k8s/core-deployment.yaml`(接线) |
| 7 | C-10 | 支付宝仅接受 PKCS#1 PEM,官方下发公钥为 PKCS#8 → 官方密钥验签全失败【已修复+回归】 | `src/gateway/alipay.rs:35-43`(修复) |
| 8 | B-01 | rust-core 无 `/internal/checkout`、`/internal/orders/preview`,前端两接口恒 404 → 静默回退 mock(3000.00 CNY,单号库内不存在)【已修复: 补 internal 路由 preview/checkout, 真实 PENDING 单数据, mock 回退仅在不可达时触发】 | `src/main.rs:50-55` + `web/lib/api.ts:18,26-34` |
| 9 | C-02 | stripe/adyen/coinbase/paypal 均无事件类型/状态白名单 → 失败单、退款、pending 单全部入账【已修复: 三网关事件白名单(EventRejected 400); paypal 已随 C-05 修复】 | `stripe.rs:48-51`、`adyen.rs:93-100`、`coinbase.rs:28-31`(修复) |
| 10 | C-08 | 前端 `x-forge-user` 头直接取 uid 查订单(B-01 修复后 IDOR 立即生效)【已修复: uid 改服务端解析 + checkout 归属校验, 跨用户单号 404】 | `web/lib/api.ts:16`(修复前) |

C-09/C-10 修复的回归证据链:
- 修复前: `cargo test alipay` → `officially_signed_callback_is_accepted` panic `SignatureInvalid("alipay rsa2")`(日志 term_32)
- 修复后: `cargo test alipay` 2/2 全绿,`TEST_EXIT=0`(term_41/term_43);`cargo clippy --all-targets` 0 警告;`cargo fmt --check` 通过

C-01/B-03/C-04/C-05 修复的回归证据链:
- C-01: webhook.rs 币种核对分支 + events.rs FraudAlert 扩展;`cargo check`/`clippy`/`fmt` 全绿(term_44/term_46)
- B-03: main.go adminAuth + pluginNameRe + Abs 前缀校验;`go vet ./...` 通过 + 热插拔 demo 复跑 DEMO_EXIT=0(term_45)
- C-04: braintree.rs 改官方 keyed HMAC(BT-Signature) + 空密钥 fail-closed;config.rs 加 `FORGE_BRAINTREE_WEBHOOK_SECRET`;k8s 接线 `braintree-webhook-secret`(optional);`cargo check`/`clippy`/`fmt` 全绿(term_47/term_48)
- C-05: paypal.rs 空密钥 fail-closed + `event_type` 白名单(仅 PAYMENT.CAPTURE.COMPLETED, 新增 GwError::EventRejected);k8s 接线 `paypal-webhook-secret`(optional);`cargo check`/`clippy`/`fmt` 全绿(term_49)
- B-02: consumer.rs poll 闭环(未达标 300s backoff 重排 + 失败 30s 重试 + start() 启动回填 PENDING_BALANCE 悬挂单);`cargo check`/`clippy`/`fmt` 全绿(term_51);真实链节点 Provider 需凭据, 列遗留
- C-02: stripe/adyen/coinbase 事件白名单(EventRejected→400: stripe payment_intent.succeeded|charge.succeeded; adyen AUTHORISATION|CAPTURE+success; coinbase completed);`cargo check`/`clippy`/`fmt` 全绿(term_53/term_54)

---

## 1. 架构说明

```
Next.js web ──/internal/*(缺失,见 B-01)──> rust-core (axum :8080)
   前端 checkout action        │  /webhooks/:gateway (alipay|stripe|coinbase|wechat|paypal|cardacq)
                              │  /healthz
                              ├─ sqlx/Postgres: orders (state_version 乐观锁, NUMERIC(20,6)),
                              │  payment_transactions (uk_txn: gateway_txn_id+gateway_type 联合唯一, 幂等)
                              └─ NATS: orders.events.paid / fraud.events.alert / crypto.events.poll
                              消费者 EventConsumer: run_paid(noop) / run_fraud(留痕) / run_poll(FOR UPDATE 状态机)
                              确认数: ConfirmationProvider (当前为 Stub, 只读库内值)
go-plugins (:3001, gin+OTel): /admin/plugins(上传+热切换) /plugins/active /discount/preview
   PluginHost.Swap: 锁内换指针 + 锁外 Kill 旧进程, 实测 417ms
k8s: core HPA 2→12; mutating-webhook 设计入口层 X-Forge-Signature = hex(HMAC-SHA256(FORGE_HMAC_SECRET, raw_body))
      (Envoy filter 段为注释预留, 未实际下发); 网关密钥 Secret 仅含 alipay/stripe/coinbase 三项
docker-compose: postgres + nats(-js JetStream 已开) + rust-core + go-plugins
```

三道死锁(已实现部分):
1. 悲观锁: `webhook.rs:147-151` SELECT FOR UPDATE 串行化同单并发回调 ✓
2. 本地金额比对: `webhook.rs:172-216` expected_amount 严格相等, 0.01 偏差即 FRAUD_SUSPENDED + 告警 ✓(但无币种核对 = C-01)
3. 链上确认数: `consumer.rs:167-223` BTC≥6 / TRC-20、ERC-20≥1, 未达标仅 PENDING_BALANCE ✓(但无人触发 poll = B-02)

## 2. 核心实体定义

| 实体 | 位置 | 关键点 |
|---|---|---|
| `GatewayTransaction` | `gateway/mod.rs:104-113` | amount 为十进制字符串, 处理层 `Decimal::from_str`, 全程不经浮点; `parse_and_verify` 出口再做一次 Decimal 可解析校验(mod.rs:167-169) ✓ |
| `orders` | `schema/001_init.sql` | NUMERIC(20,6) 六位小数 ✓; `state_version` 乐观锁; `locked_exchange_rate` FX 锁价(未被使用 = C-01 的一半) |
| `uk_txn` | 同上 | (gateway_txn_id, gateway_type) 联合唯一 → 跨网关同单号可分账, 幂等正确 ✓ |
| `OrderStatus` 状态机 | `domain/order.rs` | `transition_to` 白名单: PENDING_BALANCE→PAID 唯一合法链上迁移(consumer.rs:174-176 显式断言) ✓ |
| `Blockchain` | `domain/order.rs` | Btc=6 / Trc20=Erc20=1 确认数阈值 ✓ |
| `FraudAlert` | `events.rs` | 金额不一致时推送 ✓; 孤儿回调分支未推送 = H-02 |
| 金额微单位 | `web/lib/api.ts:7` + go `NewDecimalMinor` | 6 位小数 int64 微单位对齐 ✓(JS 侧为 Number, 见 5.2) |

## 3. 支付防攻击代码审查

问题表(指定格式):

| ID | 严重级别 | 模块 | 文件:行号 | 证据 | 攻击场景 | 影响 | 修复 diff | 回归测试 | 状态 |
|---|---|---|---|---|---|---|---|---|---|
| C-04 | CRITICAL | cardacq/braintree | `gateway/cardacq/braintree.rs:51-63` | L52 `sha256_hex("{tx_time}.{amount}")` 无共享密钥; L56 merchant_id 空则跳过核对 | 攻击者读订单页得知金额+时间窗内任意 RFC3339, 自行计算 sha256 伪造 `signature-header` | 任意订单伪造支付成功 → 全额资损 | 已修复: 切换官方 Webhook 验签 `BT-Signature = hex(HMAC-SHA256(webhook_secret, hex(SHA256(raw_body))))`(keyed, 恒定时间比较), 空密钥 fail-closed(`braintree webhook secret not configured`), 保留 300s 时间窗 + 商户号核对叠加层; config.rs 新增 `FORGE_BRAINTREE_WEBHOOK_SECRET`; k8s core-deployment.yaml 接线 `braintree-webhook-secret`(optional, 未配置时该槽位 fail-closed); 头名对齐官方 `BT-Signature`/`BT-Webhook-Id` | 伪造头回调必须 400 + FRAUD | 已修复(cargo check/clippy/fmt, term_47/term_48) |
| B-03 | CRITICAL | go-plugins | `go-plugins/main.go:44,56-99` | 已修复前: /admin/plugins 无鉴权(仅 Logger/Recovery/OTel 中间件); L78 `filepath.Join(pluginDir, name)` name 来自表单 | ①任意可达 :3001 者上传恶意二进制并热切换=RCE; ②`plugin_name=../../x` 路径穿越任意文件写 | 供应链根因 | 修复已提交: `adminAuth` 中间件(FORGE_ADMIN_TOKEN 未配置则 503 fail-closed, Bearer token 恒定时间比较)+ `pluginNameRe` 白名单 `^[a-z0-9_-]{1,64}$` + Abs/前缀纵深校验 | 无 token 503/401; `../` 400; 合法上传 200 | 已修复(go vet+demo 复跑 DEMO_EXIT=0) |
| C-09 | CRITICAL→已修复 | alipay | `gateway/mod.rs:137`(根因) `gateway/alipay.rs:44-49`(修复) | 原顺序 `base64_decode`(STANDARD) 优先: 512 字符小写 hex ⊂ [A-Za-z0-9+/] 且 len%4==0 → 静默成功解出 384 字节 | 所有支付宝合法 hex 签名回调被拒 → 业务全停(DoS) | 全部合法回调验签失败 | 修复后 hex 优先、base64 兜底(已提交于 alipay.rs:44-49) | `officially_signed_callback_is_accepted` PASS(term_43) | 已修复+回归通过 |
| C-10 | HIGH→已修复 | alipay | `gateway/alipay.rs:35-43` | 原 `from_pkcs1_pem` 仅收 "BEGIN RSA PUBLIC KEY"; 官方公钥为 PKCS#8 "BEGIN PUBLIC KEY" | 配置官方密钥即全量验签失败 | 同上(拒绝服务) | PKCS#1 失败回退 `from_public_key_pem`(spki 0.7 `rsa::pkcs8::DecodePublicKey`)(已提交) | 双格式断言 + `pem_formats_yield_same_key` PASS | 已修复+回归通过 |
| C-01 | CRITICAL | webhook | `api/webhook.rs:182-215` | 修复前: 仅 `cb_amount != order.expected_amount`, `order.currency` 未参与比对; 修复后: 金额核对前新增币种核对分支(`txn.currency` vs `order.currency`, 忽略大小写), 不匹配即 FRAUD_SUSPENDED + txn 标 CURRENCY_MISMATCH + FraudAlert(含 order_currency/callback_currency 证据字段) | FX 网关 USD 结算 100.00 vs CNY 订单 expected 100.00 → 跨币种同额入账 | 资损 | 已提交: webhook.rs 币种分支 + events.rs FraudAlert 扩展 2 字段 | TC-04: CNY 订单 + USD 同额回调 → FRAUD(代码路径静态取证, e2e 未执行) | 已修复(cargo check/clippy/fmt 全绿, term_44/46) |
| C-05 | HIGH | paypal | `gateway/paypal.rs:30-34` + `config.rs:36` + k8s core-deployment(无 paypal key) | 原判"分隔符"撤回: L30 `{ts}.{body}` 即官方规范 ✓; 真风险 = secret 空时 `hmac_sha256_hex("")` 可被任何人计算; 且无 `resource.type` 过滤(L38) | ①空密钥: 伪造任意 paypal 回调; ②退款/失败事件含 invoice_id+amount 也入账 | 资损 | 已修复: ① `verify` 入口 `secret.is_empty()` → `SignatureInvalid("paypal webhook secret not configured")`(fail-closed, 与 adyen/braintree 语义一致); ② 验签后新增事件白名单: 仅 `event_type == "PAYMENT.CAPTURE.COMPLETED"` 入账, 其余事件 `GwError::EventRejected`(mod.rs 新增变体, HTTP 400); ③ k8s core-deployment.yaml 接线 `paypal-webhook-secret`(optional: true, 未配置时该路由 fail-closed) | TC-08: 空密钥伪造 400 + 退款事件 400 | 已修复(cargo check/clippy/fmt, term_49) |
| C-02 | HIGH | stripe/adyen/coinbase | `stripe.rs:45-54` `adyen.rs:69-70` `coinbase.rs:34` | stripe 无 `json["type"]` 过滤; adyen eventCode/success 已读取但未限制; coinbase 无 `data.status` 过滤 | `payment_intent.payment_failed`/`charge.refunded`(stripe)、REFUND 通知(adyen)、PENDING/CANCELED(coinbase)均走入账路径 | 失败/退款单入账 → 资损 | 已修复(统一用 C-05 新增的 `GwError::EventRejected`, HTTP 400): stripe 仅 `payment_intent.succeeded`/`charge.succeeded`; adyen `eventCode∈{AUTHORISATION,CAPTURE} && success==true`(验签后过滤, 不破坏 HMAC 规范串); coinbase 仅 `data.status=="completed"`; paypal 已随 C-05 修复 | TC-09/13: 退款/失败事件 400 | 已修复(cargo check/clippy/fmt, term_53) |
| B-02 | HIGH | 消费者 | `consumer.rs:121-167` + `webhook.rs:279-282` | 原判"零生产者"更正: 首轮触发已存在(webhook 落 PENDING_BALANCE 即 publish_confirmation_poll); 真缺口 = ①未达标不重排(Stub 固定点: 观测值恒=存储值, 永远到不了阈值) ②处理失败即丢(L121 旧版) ③进程重启后悬挂订单无人再触发 | BTC 订单 1 确认后进入 PENDING_BALANCE, 无后续 poll → 订单永久悬挂 | 资金流死锁(非资损, 可用性) | 已修复(闭环机制): ① `process_poll` 返回 `Result<bool,String>`(true=未达标); 未达标 → 300s backoff 重排重发(consumer.rs:142-150, BTC 平均 10min/块); ② 处理失败 → 30s 重试重发(L152-164), 不再即丢; ③ `start()` 启动回填: 重扫 `status='PENDING_BALANCE'` 订单逐一补发 poll(L57-59, L74-84), 覆盖重启悬挂。剩余(需凭据, 未修): 真实链节点 ConfirmationProvider — 注入点已就绪(trait + main.rs:45 默认 Stub, 绝不伪造确认数), 接入真实节点需 RPC 凭据 + 新增 HTTP 依赖 | TC-06: 修复后 PENDING_BALANCE → backoff 轮询 → PAID(需真实确认数递增, e2e 未执行: 本机无 NATS/DB) | 已修复(闭环机制, cargo check/clippy/fmt, term_51; 真实 Provider 列遗留) |
| B-01 | HIGH | rust-core API | `main.rs:50-55` | 路由仅 `/webhooks/:gateway` + `/healthz`; 前端 `api.ts:18` 调 `/internal/orders/preview`、checkout action 调 `/internal/checkout` | 前端恒 404 → L26 静默回退 mock(FT-20260909-8f3a2c, 3000.00 CNY), 该单号库内不存在 | 生产预览金额恒为 mock; 后续支付走 webhook 孤儿分支 REJECTED + FRAUD | 已修复(2026-09-10 review 补代码): 新增 `src/api/internal.rs` — `GET /internal/orders/preview?uid=` 查该用户最新 PENDING 订单(JOIN users, expected_amount NUMERIC(20,6) ×10^6 → int64 微单位纯 Decimal 换算), 无单 404; `POST /internal/checkout` 经 `GatewayType::from_str_v` 校验 6 通道(alipay/stripe/coinbase/wechat/paypal/cardacq)后锁定 gateway_type(state_version+1), 无单/非 PENDING 404; gateway_payload 骨架期返回 {payment_method, out_trade_no}(真实支付链接待网关 SDK 接入, 边界同 B-02); 经 main.rs `api::internal::routes()` merge 注册; 前端 mock 回退保留但仅在后端不可达时触发; 遗留: uid 鉴权(C-08, 路由存在后 IDOR 风险生效)+ 入口 HMAC 未接线(k8s mutating-webhook 段仍注释) | TC-11(实库 live 需环境) | 已修复(cargo check/clippy/fmt 全绿, term_55; live 需 Postgres, 本环境无 psql/docker) |
| C-08 | HIGH | web + rust-core API | `web/lib/api.ts:16`(修复前) | 修复前: `uid = headers().get("x-forge-user") ?? "u-demo"` 客户端可控; 且 checkout 表单 `out_trade_no` 亦客户端可改, 后端仅按单号锁定无归属校验 | 改请求头遍历任意用户订单预览; 改表单单号越权结算他人订单 | 越权读(IDOR) + 越权结算, B-01 修复后全面生效 | 已修复(review, 2026-09-10): ① 前端 uid 改服务端解析 — `resolveUid()` = `process.env.FORGE_DEMO_USER ?? "u-demo"`, 不再读客户端 `x-forge-user` 头(web/lib/api.ts); ② `/internal/checkout` 请求体新增 uid, SQL 加归属校验 `user_id = (SELECT id FROM users WHERE uid = $3)`, 未知 uid/跨用户单号 → 0 行 → 404(不泄露用户枚举, src/api/internal.rs); ③ Server Action `createOrder` 透传服务端 uid(app/actions/checkout.ts)。遗留: uid 当前经环境变量单用户 demo 解析, 多用户模式开启时升级 session/JWT | TC-12 | 已修复(cargo check/clippy/fmt + tsc --noEmit, term_57; live 需 Postgres) |
| H-02 | MEDIUM | webhook | `webhook.rs` 孤儿分支(修复前 L157-163) | 修复前: 孤儿回调 order not found → txn REJECTED 但无 FraudAlert(仅 mismatch 分支推告警) | 攻击者批量打不存在单号, 后端静默无告警 | 风控盲区 | 已修复(review, 2026-09-10): 孤儿分支追加 `publish_fraud_alert` — FraudAlert 扩展 `reason` 归类(currency_mismatch/amount_mismatch/orphan_callback)与 Option 订单字段(无来源置 None, 不伪造数据); commit 失败改返 Failed(与其他分支一致) | TC-13 | 已修复(cargo check/clippy/fmt, term_58; live 需 Postgres+NATS) |
| H-09 | MEDIUM | 部署 | k8s HPA 2-12 + `go-plugins/main.go:89` | 插件热切换为 per-副本本地行为 | 灰度切换窗口内不同副本计价不一致 → 同单两价 | 计价漂移 | 配置走全局(ConfigMap+reload 或 leader 协调) | 需环境 | 未修复 |
| H-10 | MEDIUM | 交付 | `docs/architecture.md` + `src/gateway/mod.rs` float_policy + `scripts/check-security-policy.sh` | 修复前: "禁 f32/f64"等安全策略仅存文档, 无 CI/runtime 断言, 策略随时间腐化 | 质量退化 | 已修复(review, 2026-09-10): ① runtime 断言 — `gateway::float_policy::no_floating_point_in_any_source` 单测 (include_str! 读取全 19 个 .rs 源码, 剥离注释行防策略字样误报, 令牌运行时拼接避免检查器自命中); ② CI 门禁脚本 `scripts/check-security-policy.sh` (cargo test float_policy + go-plugins float64 grep, OTel 指标埋点 attribute.Float64 显式白名单为非金额范围), 脚本实测 PASS(term_61) | - | 已修复(float_policy PASS + 门禁脚本 PASS, term_61; 遗留: 仓库无 CI 系统, 脚本就绪待接入门禁) |
| L-01 | LOW | wechat | `wechat.rs`(修复前 L88-102) | 修复前: 平台证书验签已保真, 但 resource.object 类型(transaction vs refund)未过滤, 退款通知同样入账(需平台证书私钥, 实际不可达) | 纵深防御缺口 | 已修复(review, 2026-09-10): 顶层 `event_type` 白名单 — 仅 `transaction.success` 可入账, 其余类型(含 `refund.*`)一律 `GwError::EventRejected`(HTTP 400), 与 C-02/C-05 语义对齐; 缺 event_type 亦拒收 | - | 已修复(cargo check/clippy/fmt, term_59; live 需微信沙箱) |

已验证为正确的防攻击面(防"审查疲劳"误报):
- 支付宝签名基 = 字段 ASCII 排序 + 排除 sign/sign_type + SHA256WithRSA, 回归测试 `officially_signed_callback_is_accepted` 按官方签法构造 ✓
- Stripe 验签 `{t}.{payload}` + 300s 窗口 + constant_time_eq(stripe.rs:39-43) ✓; 分→Decimal `Decimal::new(cents,2)` 无浮点(stripe.rs:52) ✓; wechat 分→Decimal(wechat.rs:94) ✓; adyen 分→两位小数字符串纯整数运算(adyen.rs:95-101) ✓
- adyen 空密钥显式拒绝(adyen.rs:26-30) ✓; wechat 空证书解析失败即拒 ✓
- 幂等 `uk_txn` + FOR UPDATE(webhook.rs:147-151,157-163) ✓; state_version 乐观锁(consumer.rs:181,189) ✓

## 4. 热插拔演示(实测)

- 工具链: go1.25.6, `CGO_ENABLED=0 go build`(纯静态二进制)
- 命令: `./host-demo plugin-second-half`(第二件半价插件)
- 结果(实测, DEMO_EXIT=0):
  - swap elapsed **417.36 ms**(目标 <10s ✓, 3 个订单 300.00×3=900.00 → 折后 750.00, 第二件半价减免 150.00)
  - 旧进程 Kill 在锁外执行, 新插件健康检查通过后才原子换指针
- 风险关联: 切换窗口内多副本不一致 = H-09; 上传通道无鉴权 = B-03(热插拔机制本身是高危入口)

## 5. 前端交互

### 5.1 交互路径
`checkout action(app/actions/checkout.ts)` → `/internal/checkout`(B-01 修复前 404, 修复后携带服务端 uid, 后端按属主校验); `fetchOrderSummary(web/lib/api.ts:15-34)` → `/internal/orders/preview?uid=`(uid 改服务端解析, 仅后端不可达时静默 mock)
`FX_METHODS = ["usdt","paypal","card"]` 漏 "stripe"(用户选 stripe 无 FX 入口, 功能缺口, LOW)

### 5.2 问题
- C-08: `x-forge-user` IDOR(见 §3)【已修复: uid 改 `resolveUid()` 服务端解析(FORGE_DEMO_USER 环境变量, 缺省 u-demo), 客户端头不再参与; checkout 后端加 user_id 属主校验, 跨用户单号 404; 遗留: 多用户模式开启时升级 session/JWT】
- LOW: `lib/amount.ts` 用 JS Number 承载 6 位小数微单位 —— 最大 9,999,999.99 → 99_999_999_990 < 2^53, 精度内无溢出, 可接受但建议在边界加 `Number.isSafeInteger` 断言
- LOW: mock 回退无 toast 提示, 用户不知正使用假数据(与 B-01 叠加放大)

## 6. 部署与可观测

- k8s: HPA cpu70%/mem80% min2 max12; mutating-webhook 的 Envoy HMAC filter 段为注释(mutating-webhook.yaml:130), 入口 HMAC 未实际下发 → 当前只靠网关原生验签, 中间人可伪造"合法 HMAC 缺失"流量(与 C-04/C-05 叠加)【已接线(review, 2026-09-10): 新增 `k8s/envoy-ingress-hmac.yaml` — 完整 Envoy bootstrap (buffer + lua 签名 filter: `x-forge-signature = hex(HMAC-SHA256(secret, body))`, 18080→core:8080; 密钥部署期从 eduforge-core/hmac-secret 渲染, 未渲染即 fail-closed 全拒); `mutating-webhook.yaml` hmac webhook 改动态 2-op 注入 (patchTemplate 单 op 无法覆盖 container+volume); 遗留: k8s 环境 apply 渲染后 ConfigMap + 公网 Ingress 指 18080 + 注入器服务部署(需环境)】
- k8s secret 缺口: 仅 alipay/stripe/coinbase + hmac(db:58-66 段); braintree/paypal 已随 C-04/C-05 补 optional 接线(未配置时 fail-closed); wechat/adyen 仍缺 → L-01 与 B-02 的放大条件
- docker-compose: NATS `-js` JetStream 已启用但无 stream 定义/无 durable, 与 B-02 一致
- OTel: go-plugins otelgin + OTLP exporter ✓; rust-core 未见 OTLP 接线(仅 tracing), 跨服务 trace 断链(LOW)
- 无 NetworkPolicy: go-plugins :3001 与 rust-core :8080 Pod 内网互通无隔离, 放大 B-03(部署加固项)

## 7. i18n 与安全交付

- 前端 locales 仅 zh/en/ja, ko 保留给后端(前后端语言面不对齐, 用户选 ko 前端回退, LOW)
- 金额本地化格式 `promo.NewDecimalMinor`/`final_display` 以字符串下传 ✓ 无浮点
- 安全交付(H-10): 防攻击策略仅存 `docs/architecture.md`, 无 CI 门禁; 建议把 §3 白名单/币种断言落成 rust 单测(本次 alipay 回归测试即范式)

## 8. 测试报告

### 8.1 已执行(实测证据)
| 项 | 结果 | 证据 |
|---|---|---|
| `cargo check` / `cargo clippy --all-targets` / `cargo fmt --check` | 全绿 0 警告 | term_26/term_42 |
| `cargo test alipay`(2 用例: 官方签法双 PEM 格式 + 密钥等价) | 2/2 PASS, TEST_EXIT=0 | term_41/term_43 |
| go 热切换 demo | 417.36ms, DEMO_EXIT=0 | term_29/term_30 |

### 8.2 用例矩阵(TC)
| TC-ID | 角色 | 语言/地区 | 设备/网络 | 支付方式 | 前置 | 步骤 | 期望 | 实际 | 证据 | 结论 |
|---|---|---|---|---|---|---|---|---|---|---|
| TC-01 | 买家 | zh/CN | 桌面/正常 | 支付宝 | 官方 RSA2 密钥 | 官方签法(ASCII 排序+排除 sign/sign_type)hex 签名回调 | 验签通过, PAID | 通过 | cargo test alipay (term_43) | PASS |
| TC-02 | 买家 | 任意 | 任意 | 支付宝 | PKCS#8 官方公钥 | 同 TC-01 换 PKCS#8 PEM | 双格式均通过 | 通过 | 同上 | PASS |
| TC-03 | 攻击者 | 任意 | 任意 | 支付宝 | 篡改任意字段 | 重放/改单回调 | SignatureInvalid + FRAUD | 覆盖: `scripts/e2e-webhook.sh` step 8 (本地生成密钥对, 无需网关沙箱; 需 docker 实跑) | 未执行 | 未执行 |
| TC-04 | 攻击者 | 任意 | 任意 | 跨币种 | CNY 订单 expected=100.00 | FX 网关回调 100.00 USD | FRAUD_SUSPENDED | 修复后代码路径: webhook.rs 币种分支冻结; live 由 e2e-webhook.sh step 10 覆盖 | 静态取证(term_46 编译通过) | 修复后预期 PASS(未执行 e2e) |
| TC-05 | 攻击者 | 任意 | 任意 | 任意 | uk_txn 已存在 | 同 (gateway_txn_id,gateway_type) 重放 | Duplicate 200, 单号状态不变 | webhook.rs:140 幂等分支存在; live 由 e2e-webhook.sh step 9 覆盖 | 未执行 | 未执行 |
| TC-06 | 买家 | 任意 | 任意 | USDT(BTC) | 回调 1 确认 <6 | 等待第 6 确认 | PENDING_BALANCE→PAID | 修复后闭环: 首轮 poll(webhook 触发)→未达标 300s backoff 重排→达标结算; 失败 30s 重试; 重启启动回填 | consumer.rs:57-84,142-164 | 机制静态取证通过(无 NATS/DB 环境, e2e 未执行); 真实结算需接链节点 Provider | 部分 PASS(机制, cargo check/clippy/fmt, term_51/term_52) |
| TC-07 | 攻击者 | 任意 | 任意 | card | 默认 braintree 槽 | 自算 sha256(ts.amt) 伪造头 | 400 拒签 | 修复后: 验签改为 keyed HMAC-SHA256(BT-Signature), 无 secret 攻击者无法计算; 空密钥直接 fail-closed | braintree.rs:19-23,60-63 | 修复后预期 PASS(静态取证, e2e 未执行) |
| TC-08 | 攻击者 | 任意 | 任意 | paypal | k8s 空 secret | 空密钥 HMAC 伪造回调 | 400 | 修复后: 空密钥直接 fail-closed("paypal webhook secret not configured"); 且仅 PAYMENT.CAPTURE.COMPLETED 入账, 退款/失败事件 400 | paypal.rs:14-16,41-47 | 修复后预期 PASS(静态取证, e2e 未执行) |
| TC-09 | 买家 | 任意 | 任意 | 任意 | stripe refund 事件 | refund.created 回调 | 忽略/拒收 | 修复后: type 不在白名单 → 400 EventRejected(stripe 仅收 payment_intent.succeeded/charge.succeeded; adyen 仅 AUTHORISATION/CAPTURE+success; coinbase 仅 completed) | stripe.rs:48-51 / adyen.rs:93-100 / coinbase.rs:28-31 | 修复后预期 PASS(静态取证, e2e 未执行) |
| TC-10 | 攻击者 | 任意 | 任意 | card | /admin/plugins 可达 | `plugin_name=../../tmp/pwn` + 任意 artifact | 401(无 token)/400(名字白名单) | 修复后: adminAuth 503/401 + 白名单拦截 `../` | main.go adminAuth/pluginNameRe | 修复后预期 PASS(go vet+demo 通过, 未起服务实测) |
| TC-11 | 买家 | 任意 | 任意 | 任意 | 生产环境 | 打开订单预览 | 真实预览 | 修复前恒 mock 3000.00 CNY; 修复后: 后端可达 → 真实最新 PENDING 订单数据(内部路由 preview), mock 回退仅在 backend 不可达时触发 | internal.rs(新) + api.ts:26-34(回退路径收窄) | 部分 PASS(静态取证 + 编译/检查通过, term_55; live 由 `scripts/e2e-webhook.sh` step 6 覆盖, 需 docker 环境实跑) |
| TC-12 | 恶意买家 | 任意 | 任意 | 任意 | C-08 修复后 | 改 x-forge-user 头 / 改表单 out_trade_no | 头无效(不再读取) / 跨用户单号 404 | 修复前 uid 直传后端; 修复后 resolveUid 服务端解析 + checkout user_id 归属校验 | api.ts resolveUid + internal.rs checkout SQL | 部分 PASS(静态取证 + 编译验证, term_57; live 由 `scripts/e2e-webhook.sh` step 7 覆盖, 需 docker 环境实跑) |
| TC-13 | 攻击者 | 任意 | 任意 | 任意 | 批量不存在单号 | 孤儿回调 | 触发风控告警 | 修复前静默 REJECTED 无告警; 修复后孤儿分支推 fraud.events.alert(reason=orphan_callback, 订单字段 None) | webhook.rs 孤儿分支 | 部分 PASS(静态取证 + 编译验证, term_58; live 由 `scripts/e2e-webhook.sh` step 11 覆盖, 需 docker 环境实跑) |
| TC-14 | 买家 | ja/JP | 移动 | 任意 | 全站 | 日语浏览+支付 | 全量日语 | ko 无前端 locale, ja 正常 | locales 目录 | PARTIAL |
| TC-15 | 运营 | zh/CN | 桌面 | - | 热插拔 | 上传第二半价插件 | <10s 切换 | 417ms | term_29/30 | PASS |

### 8.3 机器可读 JSON 摘要
```json
{
  "report": "eduforge-neo-code-review-2026-09-10",
  "toolchain": { "rustc": "1.98.1", "go": "1.25.6", "node": "next.js" },
  "executed": [
    { "cmd": "cargo check/clippy/fmt --all-targets", "result": "pass", "log": "term_26/term_42" },
    { "cmd": "cargo test alipay", "result": "2/2 pass", "log": "term_41/term_43" },
    { "cmd": "go hot-swap demo (host-demo plugin-second-half)", "result": "417.36ms, exit 0", "log": "term_29/term_30" }
  ],
  "issues": [
    { "id": "C-04", "severity": "CRITICAL", "module": "cardacq/braintree", "file": "src/gateway/cardacq/braintree.rs", "line": "19-23,60-63", "kind": "forgeable_signature", "status": "fixed", "fix": "official BT-Signature = hex(HMAC-SHA256(webhook_secret, hex(SHA256(body)))) keyed + empty-secret fail-closed; FORGE_BRAINTREE_WEBHOOK_SECRET added to config.rs; k8s wires braintree-webhook-secret (optional); headers aligned to BT-Signature/BT-Webhook-Id", "regression": "cargo check/clippy/fmt pass (term_47/term_48)" },
    { "id": "B-03", "severity": "CRITICAL", "module": "go-plugins", "file": "apps/go-plugins/main.go", "line": "44,78", "kind": "rce_path_traversal_no_authn", "status": "fixed", "fix": "adminAuth(Bearer FORGE_ADMIN_TOKEN, fail-closed)+pluginNameRe whitelist+Abs prefix check", "regression": "go vet + host-demo DEMO_EXIT=0" },
    { "id": "C-09", "severity": "CRITICAL", "module": "alipay", "file": "src/gateway/alipay.rs", "line": "44-49", "kind": "hex_misdecoded_as_base64", "status": "fixed+regression_passed", "regression": "cargo test alipay" },
    { "id": "C-01", "severity": "CRITICAL", "module": "webhook", "file": "src/api/webhook.rs", "line": "182-215", "kind": "amount_check_without_currency", "status": "fixed", "fix": "currency-mismatch branch before amount compare; FraudAlert extended with order_currency/callback_currency", "regression": "cargo check/clippy/fmt pass (term_44/46)" },
    { "id": "B-02", "severity": "HIGH", "module": "consumer", "file": "src/consumer.rs", "line": "57-84,142-164", "kind": "poll_no_reschedule_restart_loss_stub_fixed_point", "status": "fixed (mechanism)", "fix": "process_poll returns below-threshold flag; 300s backoff reschedule; 30s failure retry; startup backfill republishes PENDING_BALANCE polls; real chain-node provider remains open (needs RPC creds, Stub is safe default)" },
    { "id": "C-05", "severity": "HIGH", "module": "paypal", "file": "src/gateway/paypal.rs", "line": "14-16,41-47", "kind": "empty_secret_forgeable+no_event_filter", "status": "fixed", "fix": "empty-secret fail-closed + event_type whitelist PAYMENT.CAPTURE.COMPLETED (GwError::EventRejected new) + k8s wires paypal-webhook-secret (optional)", "regression": "cargo check/clippy/fmt pass (term_49)" },
    { "id": "C-10", "severity": "HIGH", "module": "alipay", "file": "src/gateway/alipay.rs", "line": "35-43", "kind": "pkcs8_pem_unsupported", "status": "fixed+regression_passed" },
    { "id": "C-02", "severity": "HIGH", "module": "stripe/adyen/coinbase", "file": "src/gateway/*.rs", "line": "stripe:48-51,adyen:93-100,coinbase:28-31", "kind": "missing_event_type_whitelist", "status": "fixed", "fix": "event-type whitelists via GwError::EventRejected (HTTP 400): stripe payment_intent.succeeded|charge.succeeded; adyen AUTHORISATION|CAPTURE + success==true (post-HMAC, canonical string untouched); coinbase data.status==completed; paypal done in C-05", "regression": "cargo check/clippy/fmt pass (term_53/term_54)" },
    { "id": "B-01", "severity": "HIGH", "module": "rust-core-api", "file": "src/api/internal.rs", "line": "new", "kind": "missing_internal_routes_frontend_mock_fallback", "status": "fixed", "fix": "GET /internal/orders/preview?uid= returns user's latest PENDING order (JOIN users, NUMERIC(20,6) x1e6 -> i64 micro-units, pure Decimal); POST /internal/checkout validates payment_method via GatewayType::from_str_v (6 channels) then locks gateway_type (state_version+1); 404 on no order; frontend mock fallback now only triggers when backend unreachable; gateway_payload is skeleton {payment_method, out_trade_no} until gateway SDK wired; open: uid auth (C-08) + entry HMAC not wired", "regression": "cargo check/clippy/fmt pass (term_55); live needs Postgres (TC-11 db part not executed)" },
    { "id": "C-08", "severity": "HIGH", "module": "web+rust-core-api", "file": "apps/web/lib/api.ts + apps/rust-core/src/api/internal.rs", "line": "resolveUid/checkout", "kind": "idor_x_forge_user", "status": "fixed", "fix": "uid resolved server-side (FORGE_DEMO_USER env, default u-demo), client x-forge-user header no longer read; /internal/checkout now requires uid and enforces order ownership via user_id=(SELECT id FROM users WHERE uid=$3) so cross-user out_trade_no yields 404; server action passes server-resolved uid", "regression": "cargo check/clippy/fmt (term_57) + tsc --noEmit; live needs Postgres", "open_note": "upgrade uid resolution to session/JWT when multi-user mode opens" },
    { "id": "H-02", "severity": "MEDIUM", "module": "webhook", "file": "src/api/webhook.rs", "line": "orphan branch", "kind": "orphan_callback_no_fraud_alert", "status": "fixed", "fix": "orphan branch (order not found -> txn REJECTED) now publishes fraud alert; FraudAlert extended with reason taxonomy (currency_mismatch/amount_mismatch/orphan_callback) and Option order fields (None when source absent, no fabricated data); commit failure returns Failed like sibling branches", "regression": "cargo check/clippy/fmt (term_58); live needs Postgres+NATS" },
    { "id": "H-09", "severity": "MEDIUM", "module": "deploy", "file": "k8s", "line": null, "kind": "multi_replica_plugin_desync", "status": "open" },
    { "id": "H-10", "severity": "MEDIUM", "module": "delivery", "file": "src/gateway/mod.rs float_policy + scripts/check-security-policy.sh", "line": null, "kind": "policy_docs_only_no_ci", "status": "fixed", "fix": "runtime assertion: float_policy unit test scans all 19 rust-core sources via include_str! (comment lines stripped, tokens runtime-joined to avoid self-hit); CI gate script check-security-policy.sh (cargo test float_policy + go-plugins float64 grep, OTel metric attribute.Float64 whitelisted as non-amount); script verified PASS; open: repo has no CI host to wire the script into", "regression": "cargo test float_policy + gate script PASS (term_61)" },
    { "id": "L-01", "severity": "LOW", "module": "wechat", "file": "src/gateway/wechat.rs", "line": "event_type filter", "kind": "object_type_not_filtered", "status": "fixed", "fix": "top-level event_type whitelist: only transaction.success is creditable; refund.* and any other type rejected via GwError::EventRejected (HTTP 400), missing event_type also rejected; aligned with C-02/C-05 semantics", "regression": "cargo check/clippy/fmt (term_59); live needs wechat sandbox" }
  ],
  "testcases": { "total": 15, "pass": 3, "fail": 7, "partial": 4, "not_executed": 1 },
  "fixed_in_review": [
    { "id": "C-09", "regression": "cargo test alipay 2/2 (term_43)" },
    { "id": "C-10", "regression": "cargo test alipay 2/2 (term_43)" },
    { "id": "C-01", "regression": "cargo check/clippy/fmt (term_44/46)" },
    { "id": "B-03", "regression": "go vet + host-demo DEMO_EXIT=0 (term_45)" },
    { "id": "C-04", "regression": "cargo check/clippy/fmt (term_47/term_48)" },
    { "id": "C-05", "regression": "cargo check/clippy/fmt (term_49)" },
    { "id": "B-02", "regression": "cargo check/clippy/fmt (term_51/term_52); real node provider left open (needs RPC creds)" },
    { "id": "C-02", "regression": "cargo check/clippy/fmt (term_53/term_54)" },
    { "id": "B-01", "regression": "cargo check/clippy/fmt (term_55); live needs Postgres (TC-11 db part not executed)" },
    { "id": "C-08", "regression": "cargo check/clippy/fmt + tsc --noEmit (term_57); live needs Postgres; uid via env in demo, session/JWT left open" },
    { "id": "H-02", "regression": "cargo check/clippy/fmt (term_58); live needs Postgres+NATS" },
    { "id": "L-01", "regression": "cargo check/clippy/fmt (term_59); live needs wechat sandbox" },
    { "id": "H-10", "regression": "cargo test float_policy + scripts/check-security-policy.sh PASS (term_61); CI host wiring left open" }
  ],
  "not_executed_reason": "本机无 docker; 网关沙箱账号/证书未提供 (e2e 脚本已补齐, alipay 链路无沙箱依赖)",
  "not_executed_commands": [
    "./scripts/e2e-webhook.sh  # 需 docker+psql+openssl+python3+cargo; 一次复现 TC-03/04/05/11/12/13 (alipay 签本地生成密钥对)",
    "cargo nextest run --profile ci  # JUnit 报告 (需安装 nextest, 仓库当前无 CI 宿主)"
  ]
}
```

### 8.4 未执行项与复现命令
1. 端到端 webhook(需 Postgres+NATS, alipay 签用本地生成密钥对, 无需网关沙箱): `./scripts/e2e-webhook.sh` (脚本已补齐, 一次性复现 TC-03/04/05 + TC-11/12/13; 本机 `docker not found` 未实跑, 有 docker 机一键执行)
2. 幂等重放 TC-05: 已由 `scripts/e2e-webhook.sh` step 9 覆盖 (同 body 连发两次, 期望第二次 200 Duplicate 且状态不变); 起服务后同 (gateway_txn_id, gateway_type) 手动连发亦可
3. JUnit 报告: 当前无 cargo-cargo-junit 接线, `cargo nextest run --profile ci`(需安装 nextest)

---
## 下一步建议(优先级)
~~B-03、C-01、C-04、C-05、C-02、B-02(闭环机制)、B-01(internal 路由)、C-08(uid 服务端解析 + 属主校验)、H-02(孤儿回调推风控告警)、L-01(wechat 事件类型白名单)、H-10(浮点策略单测断言 + 门禁脚本)~~ 已修复(见 §3 状态列), 剩余:
1. B-02(遗留): 接真实链节点 ConfirmationProvider(需 RPC 凭据 + HTTP 依赖, 注入点已就绪)
2. C-08(遗留): 多用户模式开启时将 uid 解析从环境变量升级为 session/JWT(当前 demo 单用户); 入口 HMAC 配置侧已接线(`k8s/envoy-ingress-hmac.yaml` + 动态注入说明, 见 §6), 剩余 k8s 环境落地: apply 渲染后 ConfigMap + 公网 Ingress 指 18080 + 部署注入器服务
3. H-09(需环境): 多副本计价配置隔离(k8s ConfigMap/reload 或 leader 协调)
4. H-10(遗留): 将 `scripts/check-security-policy.sh` 接入 CI 主机门禁(脚本与单测断言已就绪, 仓库当前无 CI 系统)
5. 有 docker+psql 机器执行 `./scripts/e2e-webhook.sh` 即完成 TC-03/04/05/11/12/13 全部 live 复现(脚本已补齐, alipay 签名本地生成密钥对, 无需网关沙箱凭据; 本机无 docker 未实跑)
