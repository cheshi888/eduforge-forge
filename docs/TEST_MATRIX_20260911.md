# 全系统测试矩阵 — 2026-09-11 (Round 2)

> 配套 `docs/CODE_REVIEW_20260911.md`。TC 编号从上轮 TC-15 之后续编 (TC-16 起)。
> 13 必含场景全部落位 (映射见 §4)。每用例含: 角色/语言地区/设备网络/前置/步骤/期望/实际/证据断言/结论。
> "实际"列: 本轮未实跑任何服务, 静态可验证项除外 (标注"已静态验证"); 其余一律标 **[需环境]** 并进入 §6 执行清单。

## 1. 用户画像 (Persona)

| 编号 | 画像 | 语言/地区 | 设备/网络 | 典型路径 |
|---|---|---|---|---|
| P1 | 国内课程买家 (支付宝/微信) | zh / 中国 | Android 移动 4G, 弱网 (3G 级, 2s 延迟) | /zh/checkout → alipay/wechat → 网关回调 → 课程解锁 |
| P2 | 海外买家 (卡/PayPal/USDT) | en / 美国, 或 ja / 日本 | 桌面 Chrome, 跨境 CDN 网络 | /en/checkout → stripe/paypal/usdt → USD 结算 |
| P3 | 恶意攻击者 (黑产/脚本) | — | 服务器批量请求, 可伪造出单号 | 篡改回调、重放、越权 /internal、薅促销 |
| P4 | 平台运维 (SRE) | zh | 跳板机 + k8s 集群 | 插件热切换、sidecar 注入、HPA 扩容、链上轮询 |

## 2. 通用断言工具

```bash
# 支付事务与订单状态
psql -h 127.0.0.1 -U eduforge -d eduforge -c \
  "SELECT out_trade_no, status, paid_amount, blockchain_confirmations FROM orders WHERE out_trade_no='$O';"
psql -h 127.0.0.1 -U eduforge -d eduforge -c \
  "SELECT gateway_txn_id, gateway_type, status, amount, currency FROM payment_transactions WHERE out_trade_no='$O';"

# NATS 风控/支付事件计数 (JetStream)
curl -s http://127.0.0.1:8222/js/api/v1/accounts/stream/fraud.events.alert | jq .
# 或消费一条:
curl -s http://127.0.0.1:4222 -H "Authorization: Bearer $TOKEN" # (nats CLI 首选: nats sub fraud.events.alert --count 1)

# rust-core 日志断言
docker logs eduforge-core 2>&1 | grep -E "amount mismatch|currency mismatch|order not found|idempotent"
# k8s: kubectl logs deploy/eduforge-core -n eduforge | grep -E "..."

# OTel trace 断言 (go-plugins; sidecar 注入时)
kubectl logs deploy/eduforge-webhook -n eduforge-system | grep "plugins.swap"
# 期望 span 属性: plugins.swap.elapsed_ms < 10000 (热切换 <10s), plugins.active=plugin-second-half
```

**入口 HMAC 前置** (rust-core 严格模式, gateway/mod.rs:144-151): 所有 `/webhooks/:gateway` 请求必须带 `x-forge-signature: hex(HMAC-SHA256(FORGE_HMAC_SECRET, raw_body))`, 缺一即 400。本地复现用 `scripts/e2e-webhook.sh` 的签名函数。

## 3. 用例矩阵 (按网关 × 场景类)

图例: 正常 / 异常 / 恶意 / 并发 / 故障。结论列: PASS=符合期望; FAIL=暴露缺陷 (→ R2 编号)。

### 3.1 支付宝 (P1)

| TC | 类 | 步骤 (curl 骨架) | 期望 | 实际 | 证据断言 | 结论 |
|---|---|---|---|---|---|---|
| TC-16 | 正常 | 本地 RSA2 密钥对按官方规范签名 `notify_type=trade_status_sync&trade_status=TRADE_SUCCESS&out_trade_no=FT-01&total_amount=100.00&currency=CNY&trade_no=T01` → POST /webhooks/alipay (带入口 HMAC 头) | 200 body "success"; orders=PAID; txn=SETTLED | [需环境: rust-core+pg+docker] | SQL 断言 status='PAID', paid_amount=100.000000; 日志无 fraud 行 | — |
| TC-17 | 异常 (必含#2 少付) | 同 TC-16 但 total_amount=99.99 (签名合法, 金额<预期) | 200 ACK; orders=FRAUD_SUSPENDED; txn=MISMATCH; fraud 事件 1 条 | [需环境] | NATS fraud.events.alert 计数 +1, reason=amount_mismatch; SQL paid_amount=99.99, status=FRAUD_SUSPENDED | — |
| TC-18 | 恶意 (必含#1 金额篡改) | 篡改后**不重签** (或伪造 sign 字段) 回调 total_amount=0.01 | 4xx SignatureInvalid; 订单不动作; txn 不落 | [需环境] | curl 状态码 4xx; SQL payment_transactions 无该 out_trade_no 行 | — |
| TC-19 | 异常 (必含#3 多付) | 合法签名 total_amount=100.10 (>预期 100.00) | 严格相等比对 → FRAUD_SUSPENDED + MISMATCH (L2-05: 多付同样冻结, 无自动退款) | [需环境] | SQL status=FRAUD_SUSPENDED; fraud reason=amount_mismatch, received>expected | 预期冻结; 产品需补退款路径 |
| TC-20 | 恶意 (必含#4 重放) | TC-16 成功报文原样重发 N=3 次 | 首次 PAID, 后续 200 Duplicate ACK; uk_txn ON CONFLICT 早退; 无二次结算 | [需环境] | SQL txn 仍 1 行 status=SETTLED; orders state_version 仅 +1; 日志 "idempotent skip" | — |
| TC-21 | 异常 (C2-02 退款入账) | 合法签名 `notify_type=trade_refund&trade_status=REFUND_SUCCESS&total_amount=100.00` | **现状**: 被入账 PAID (无 notify_type 白名单, alipay.rs:61-81) → FAIL 暴露 C2-02 | [需环境] | SQL orders status: PENDING→PAID (缺陷); 修复后期望 4xx EventRejected | 预期 FAIL (C2-02) |
| TC-22 | 并发 | 200 并发同 out_trade_no 混合 (正确金额×199 + 正确重放×1) | 恰好 1 次 PENDING→PAID, 其余 Duplicate; 无死锁 | [需环境: ab 或脚本] | SQL paid_amount 唯一值; state_version 增量=2 (冻结前 PAID 1 次); DB 无 deadlock 日志 | — |

### 3.2 Stripe (P2)

| TC | 类 | 步骤 | 期望 | 实际 | 证据断言 | 结论 |
|---|---|---|---|---|---|---|
| TC-23 | 正常 | `stripe_` webhook JSON `type=invoice.payment_succeeded`, header `Stripe-Signature: t=..,v1=HMAC-SHA256(secret, "t.body")` (300s 内) | 200 "ok"; PAID | [需环境] | SQL PAID; txn=SETTLED | — |
| TC-24 | 恶意 (必含#4) | TC-23 报文重放 2 次 | 首次入账, 后续 Duplicate (uk_txn) | [需环境] | 同 TC-20 断言, gateway_type='stripe' | — |
| TC-25 | 恶意 | 将报文 t 戳改为 10 分钟前 (>300s 窗) 重新算 v1 签名 | SignatureInvalid 4xx | [需环境] | 日志 "stripe timestamp drift" | — |
| TC-26 | 并发 | 50 并发同一 invoice 事件 | 1 PAID + 49 Duplicate, P95<200ms (hpa.yaml 目标) | [需环境] | 计时断言; 连接池水位 (pg_stat_activity) | — |
| TC-54 | 异常 (M2-01) | 构造缺失 `data.object.id` 的事件 (签名合法) | **现状**: txn_id="" 落库, 与另一空 ID 流互斥 → FAIL 暴露 M2-01 | [需环境] | SQL gateway_txn_id='' 行数≥2 时第 2 单丢失 | 预期 FAIL (M2-01) |

### 3.3 Coinbase (P2, USDT/crypto)

| TC | 类 | 步骤 | 期望 | 实际 | 证据断言 | 结论 |
|---|---|---|---|---|---|---|
| TC-27 | 正常 | TRC-20 转账事件, confirmations=1 (达标), header 签名 | 200 `{"code":"SUCCESS"}`; PAID | [需环境] | SQL blockchain_confirmations=1, status=PAID | — |
| TC-28 | 异常 (必含#6 确认数不足) | TRC-20 confirmations=0 (<1) | PENDING_BALANCE + confirmations=0; NATS crypto.events.poll 事件; **不发货** | [需环境] | SQL status=PENDING_BALANCE; 日志 "below confirmation threshold"; 消费者 300s backoff 启动 (StubProvider 下永不推进 → B-02) | — |
| TC-29 | 恶意 (必含#7 链上重组) | TC-28 之后模拟 reorg: 同笔交易 confirmations 3→0 (回调推 conf=0, 网络声明 TRC-20) | 订单保持/退回 PENDING_BALANCE, 已发放权益应收回 | **现状**: 确认数只增不减 (webhook.rs:286-317 无"低于已存确认数→降级"分支) → FAIL 暴露 | [需环境] | SQL blockchain_confirmations 3 不被 0 覆盖/或订单滞留 PAID | 预期 FAIL (设计缺口, 入报告 §6) |
| TC-30 | 恶意 (C2-03) | `network="SOLANA"` + confirmations=0 | **现状**: crypto_network=None → 确认数校验旁路 → 直接 PAID → FAIL 暴露 C2-03 | [需环境] | SQL status=PAID 且 blockchain_confirmations=0 (缺陷证据) | 预期 FAIL (C2-03) |
| TC-31 | 并发 | 同笔 conf=1 与 conf=3 事件乱序并发 | 终态 confirmations=3, PAID 唯一 | [需环境] | SQL 终值断言 | — |

### 3.4 微信 (P1)

| TC | 类 | 步骤 | 期望 | 实际 | 证据断言 | 结论 |
|---|---|---|---|---|---|---|
| TC-32 | 正常 | APIv3 平台证书 RSA-SHA256 签名 (`ts\nnonce\nbody\n`), event_type=transaction.success, resource 明文体 | 200; PAID | [需环境] | SQL PAID; txn=SETTLED | — |
| TC-33 | 恶意 | resource 走 AES-256-GCM 密文分支, AAD 改 1 字节 | Decrypt 4xx | [需环境] | 日志 "resource aes-gcm" | — |
| TC-34 | 异常 (L-01 回归) | event_type=refund.success | EventRejected (wechat.rs:48-58 白名单) | [需环境] | 4xx + 日志 "not whitelisted" | 预期 PASS |
| TC-35 | 故障 (H2-02) | k8s 环境 (env 缺 FORGE_WECHAT_PLATFORM_CERT_PEM) 发 TC-32 报文 | **现状**: 证书解析失败恒 400 → wechat 通道在 k8s 恒死 → FAIL 暴露 H2-02 | [需环境: k8s 集群] | 日志 "wechat cert: ..." | 预期 FAIL (H2-02) |

### 3.5 PayPal (P2)

| TC | 类 | 步骤 | 期望 | 实际 | 证据断言 | 结论 |
|---|---|---|---|---|---|---|
| TC-36 | 正常 | PayPal webhook (签名头合法, 180s 窗), CNY 币种订单 | 200 "ok"; PAID | [需环境] | SQL PAID | — |
| TC-37 | 故障 | `FORGE_PAYPAL_WEBHOOK_SECRET` 留空启动 → 任意 PayPal 回调 | 全拒 4xx (fail-closed, paypal.rs 空密钥分支) | [需环境] | 日志 "paypal secret not configured" | 预期 PASS |
| TC-38 | 异常 (必含#8 汇率波动, H2-01) | CNY 订单走 paypal, 回调 currency=USD, 金额=锁定汇率折算值 | **现状**: webhook.rs:205 币种先行 → FRAUD_SUSPENDED (currency_mismatch) → 跨境通道断链, FAIL 暴露 H2-01 | [需环境] | SQL status=FRAUD_SUSPENDED; fraud reason=currency_mismatch; **并断 B2-01 回归**: 响应是否 200 + txn 是否落 CURRENCY_MISMATCH (现状 5xx + 告警丢失 → B2-01) | 预期 FAIL (H2-01 + B2-01) |

### 3.6 卡收单 cardacq (P2, braintree/adyen 可切换)

| TC | 类 | 步骤 | 期望 | 实际 | 证据断言 | 结论 |
|---|---|---|---|---|---|---|
| TC-39 | 正常 | braintree `kind=transaction.settled` + BT-Signature keyed HMAC | 200 "ok"; PAID | [需环境] | SQL PAID, gateway_type='cardacq' | — |
| TC-40 | 恶意 (C2-01) | braintree `kind=transaction.voided` (未扣款) | **现状**: 无 kind 白名单 → 入账 PAID → FAIL 暴露 C2-01 | [需环境] | SQL status: PENDING→PAID (缺陷证据) | 预期 FAIL (C2-01) |
| TC-41 | 异常 | adyen 双事件 AUTHORISATION 后 CAPTURE 同 psp_ref | 首个入账, 第二个 Duplicate ACK; **现状** mark_txn 写 SETTLED (审计失真, M2-03/L2-01) | [需环境] | SQL txn 2 行: SETTLED + SETTLED (应为 SETTLED+DUPLICATE 修复后) | 预期部分 FAIL (M2-03) |
| TC-42 | 故障 | `FORGE_CARD_ACQUIRER=adyen` + adyen_hmac_key 配置, adyen 标准 HMAC 事件 | 通道切换生效, PAID | [需环境] | 日志通道选择断言 | — |

### 3.7 促销插件热切换 (P4 / P3)

| TC | 类 | 步骤 | 期望 | 实际 | 证据断言 | 结论 |
|---|---|---|---|---|---|---|
| TC-43 | 正常 (必含#9 插件热切换) | 1) `POST /admin/plugins` 上传 plugin-second-half 二进制 (Bearer FORGE_ADMIN_TOKEN) 2) `GET /plugins/active` 3) `POST /discount/preview {"price_minor":300000000,"qty":3}` | Swap<10s 不重启宿主; active=plugin-second-half external=true; 900.00 → final=750.00 discount=150.00 (每 2 件 50%) | [需环境: go-plugins 服务] | OTel span plugins.swap: plugins.swap.elapsed_ms<10000, plugins.active=plugin-second-half; 响应 JSON final_minor=750000000 | — |
| TC-44 | 恶意 (H2-04) | preview 输入 `price_minor=9000000000000000000, qty=2` (int64 回绕) 与负数 qty | 边界拒绝/正值结果, 无负 FinalMinor | **现状**: builtins.go:14 溢出可产生负数 → FAIL 暴露 H2-04 | 响应 final_minor<0 即缺陷证据 | 预期 FAIL (H2-04) |
| TC-45 | 恶意 | 未配 `FORGE_ADMIN_TOKEN` 启动 go-plugins → 上传任意二进制 | 503 fail-closed (main.go:33-39, B-03 回归) | [可静态验证: 代码路径 main.go:35-39] | 代码断言 PASS; live 需环境 | 静态 PASS |
| TC-46 | 故障 | 上传 sleep 插件 (ApplyDiscount 阻塞) → 并发 100 个 /discount/preview | 有客户端超时, 不堆积 goroutine | **现状**: net/rpc 无超时 → goroutine 线性堆积 → FAIL 暴露 H2-04 | 服务 goroutine 计数 (debug/pprof) 随并发线性增长 | 预期 FAIL (H2-04) |

### 3.8 内容交付: 预签名 URL / M3U8 (P1/P2)

| TC | 类 | 步骤 | 期望 | 实际 | 证据断言 | 结论 |
|---|---|---|---|---|---|---|
| TC-47 | 正常+恶意 (必含#11 预签名 URL 盗链) | 1) 对已购课程 m3u8/切片生成预签名 URL (MinIO, TTL 5min, docker-compose L56 设计) 2) 6min 后用同 URL 重放 3) 未购用户 P3 直接访问 | TTL 内 200; 过期/无凭证 403 | [需环境: minio + 交付服务; 仓库内无 presign 生成代码 → 材料缺失, 见 §7] | curl -sI 过期 URL 期望 403 (Sig 过期错误码) | — |
| TC-48 | 恶意 (必含#12 M3U8 Key 泄露) | 已购用户 A 的 M3U8 内 TS 分片 URL 被用户 B (非属主) 抓取播放 | 分片 URL 属主校验 → B 403; 泄露面 = 单用户 5min 窗口 | [需环境, 材料缺失同 TC-47] | B 的 curl 期望 403; 日志 403 计数 | — |

### 3.9 i18n (P1 zh / P2 en·ja)

| TC | 类 | 步骤 | 期望 | 实际 | 证据断言 | 结论 |
|---|---|---|---|---|---|---|
| TC-49 | 异常 (必含#10 i18n 缺失) | 1) 访问 `/ko/checkout` 2) 访问 `/xx/checkout` (不存在 locale) 3) 检查 next-intl key 缺失兜底 | /ko 为死路由 (routing 仅 zh/en/ja, i18n/routing.ts:4) → 落 default zh (L2-02); /xx → redirect 默认; 当前 4 语言 key 集合一致 (19 keys) 无缺失 | 静态断言 (key 一致性) 已验证 PASS; live 路由行为 [需环境: next dev] | python diff 4 个 locales/*.json: 19 keys 全一致 (2026-09-11 实跑); 浏览器断言 /ko→/zh 重定向 | 静态 PASS |
| TC-50 | 故障 | 删除某 key (如 checkout.payNow) 后渲染页面 | next-intl 抛 missing message → 组件崩溃 → 期望兜底 | [需环境: next dev] | 页面错误边界断言 | — |

### 3.10 基础设施 (P4 / P3)

| TC | 类 | 步骤 | 期望 | 实际 | 证据断言 | 结论 |
|---|---|---|---|---|---|---|
| TC-51 | 恶意 (H2-03) | 集群外 `curl http://<ingress:18080>/internal/orders/preview?uid=u-victim` | **现状**: envoy prefix "/" 全转发 + 自动补 x-forge-signature → 200 返回受害者 PENDING 订单 → FAIL 暴露 H2-03 | [需环境: k8s + envoy sidecar] | 响应 200 + JSON 断言 (缺陷证据); 修复后期望 404 | 预期 FAIL (H2-03) |
| TC-52 | 恶意 (必含#5 并发超卖) | 同 SKU stock=1, P3 脚本并发 50 个下单+支付回调 | ≤1 单最终 PAID | **现状**: 无 stock 扣减 → 50 单全 PAID → FAIL; 修复后 (H2-05 方案 A): 结算前 `UPDATE products SET stock=stock-1 WHERE stock>=1` CAS, 命中 0 行回滚 → 仅 1 单 PAID, 其余 FRAUD/Failed | 预期: 修复后 PASS (需 PG 环境实跑) |
| TC-53 | 并发 | 同订单双击 2 次 /internal/checkout | gateway_type 锁定 1 次; state_version CAS **现状缺失** (M2-07), 双锁幂等靠 status 条件 | [需环境] | SQL state_version 增量=2 (非 1) 即 CAS 未生效 | — |
| TC-54 | 故障 (必含#13 Sidecar 注入失败) | injector Deployment 置 0 副本 / 不可达 (failurePolicy: Ignore 生效) → 新建 core Pod | Pod 无 hmac-gateway sidecar; 全部外部回调无 x-forge-signature → 后端 fail-closed 全拒 (mutating-webhook.yaml:94 设计意图), 无未签名流量放行 | [需环境: k8s] | kubectl get pod -o yaml 无 hmac 容器; 回调全 400 "bad hmac"; 业务侧 0 放行 | 预期 PASS (fail-closed 设计验证) |
| TC-55 | 故障 | envoy 配置占位符 `__FORGE_HMAC_SECRET_B64__` 未渲染即启动 sidecar | bootstrap 断言失败 → 18080 全 5xx fail-closed, 无明文放行 | [需环境: k8s] | sidecar 容器 crash-loop + 日志 assert | — |
| TC-56 | 故障 | NATS JetStream 停机 → 发金额不一致回调 (TC-17 报文) | 订单仍冻结 (DB 层), 但 fraud/poll 事件丢失 (events.rs:71-77 仅 warn) → 风控盲区 | [需环境: nats down] | 日志 "jetstream publish failed"; NATS 恢复后告警未补发 | 预期部分 FAIL (M2-02) |
| TC-57 | 故障 (H2-04 补充) | 并发超卖 + HPA 12 副本场景压测 | pg 连接不饿死 | **现状**: 12×20=240 > pg max_connections=100 → 连接池等待堆积 → FAIL 暴露 M2-04 | pg_stat_activity count; p95 延迟断言 | [需环境: k8s+pg] 预期 FAIL (M2-04) |

| TC-58 | 异常 (C3-01/C3-02 契约映射) | 前端选 `card` 与 `usdt` 各下单 1 次, 抓 `/internal/checkout` 请求体 | 请求体 `payment_method` 分别为 `cardacq` / `coinbase` (UI 值已映射), rust 侧不再 400 unknown payment method; 两单均正常锁 gateway_type | [需环境: web + rust-core] | 抓包断言 body.payment_method ∈ {cardacq, coinbase}; SQL gateway_type='cardacq'/'coinbase' 各 1 行 | 修复后 PASS (静态 tsc 已验证映射存在) |

## 4. 13 必含场景映射

| # | 必含场景 | TC |
|---|---|---|
| 1 | 金额篡改 | TC-18 |
| 2 | 少付 | TC-17 |
| 3 | 多付 | TC-19 |
| 4 | 重放 | TC-20 / TC-24 / TC-25 |
| 5 | 并发超卖 | TC-52 |
| 6 | 确认数不足 | TC-28 |
| 7 | 链上重组 | TC-29 |
| 8 | 汇率波动 | TC-38 |
| 9 | 插件热切换 | TC-43 |
| 10 | i18n 缺失 | TC-49 |
| 11 | 预签名 URL 盗链 | TC-47 |
| 12 | M3U8 Key 泄露 | TC-48 |
| 13 | Sidecar 注入失败 | TC-54 |

(上轮 TC-16 前编号 1-15 见 CODE_REVIEW_20260910.md §8; 本轮 TC-54/57 与上轮 TC 编号在不同文档内, 已按文档内唯一。)

## 5. 执行清单 (按环境分档)

| 档位 | 环境 | 覆盖 TC | 启动步骤 |
|---|---|---|---|
| A: 单机 docker | `docker compose up -d` + psql 灌 schema + `cargo run --bin rust-core` (env: FORGE_DATABASE_URL/FORGE_NATS_URL/FORGE_HMAC_SECRET/网关密钥) | TC-16~37 (除 TC-35), TC-43~46, TC-49b, TC-53, TC-56, TC-57(压测部分) | `bash scripts/e2e-webhook.sh` 可代跑 alipay 分支 + fraud 消息断言; 其余网关 curl 手工签发 |
| B: go-plugins 服务 | `go run ./cmd/...` 宿主 + 编译 plugin-second-half | TC-43~46 | FORGE_ADMIN_TOKEN 需另行注入 (不写入仓库) |
| C: web 前端 | `npm run dev -p 3000` (FORGE_API_BASE 指向 rust-core) | TC-49/50/52(前端部分) | i18n 渲染断言 |
| D: k8s 集群 | k8s/ 三件套 + pg HPA | TC-35, TC-51, TC-54, TC-55, TC-57 | 注入器镜像 registry.example.com/… 为占位 (H-09 未收口), 先手挂 sidecar 验证 TC-54 设计意图 |
| E: 链上/Media | 真实链节点 RPC + MinIO 交付服务 | TC-28~30, TC-47/48 | **材料缺失**: 预签名/M3U8 生成代码不在本仓库, ConfirmationProvider 真实实现未提供 → 执行前需补材料 |

### 执行前置检查 (全部 [需环境] 用例统一)

1. `psql -c 'SELECT 1'` 通; NATS `nats server info` JetStream=true。
2. rust-core 启动日志无 "stub confirmation provider" 以外的异常。
3. 每个 webhook 用例前记录 `SELECT count(*) FROM payment_transactions` 基线, 用例后 diff。
4. fraud/poll 计数用 JetStream consumer 或 8222 API 抓取前后差值。

## 5.5 修复实施后期望值更新 (2026-09-11, 同日修复批次 B2-01/C2-01/C2-02/C2-03/M2-01/H2-03 + 第三批 H2-02/M2-05/M2-06/M2-03/M2-02)

以下用例在修复落盘后, "期望"列更新为修复后语义 (重跑时以本表为准, §3 原"现状"描述仅作为缺陷证据存档):

| TC | 修复前期望 (缺陷暴露) | 修复后期望 |
|---|---|---|
| TC-21 alipay 退款通知 | PENDING→PAID (FAIL, C2-02) | 4xx EventRejected "notify_type trade_refund not whitelisted", 订单不动作 |
| TC-30 coinbase 未知网络 | 0 确认直接 PAID (FAIL, C2-03) | 4xx EventRejected "network=SOLANA unsupported" |
| TC-38 paypal CNY 订单 USD 回调 | 5xx + 告警丢失 (FAIL, B2-01) | 200 ACK + orders=FRAUD_SUSPENDED + txn=CURRENCY_MISMATCH + fraud 事件 1 条 (H2-01 冻结语义不变, 属 H2-01 产品决策范畴) |
| TC-40 braintree voided | 入账 PAID (FAIL, C2-01) | 4xx EventRejected "kind not whitelisted" |
| TC-41 adyen CAPTURE 重复 | txn 2 行 SETTLED+SETTLED (FAIL, M2-03) | txn 2 行 SETTLED+DUPLICATE; 订单仅 +1 (M2-03 修复) |
| TC-43 FX CNY 订单 + USD 网关回调 | 现状全冻 (H2-01 断链, 无折算) | 锁汇 `locked_exchange_rate` 存在时按 `expected × rate` 折算比对, 相等则 PAID; 无锁汇/锁汇 0 仍冻 FRAUD_SUSPENDED (H2-01 方案 B, 单测 `fx_comparable_*` 4 条覆盖判定) |
| TC-51 /internal 外部可达 | 200 泄漏订单 (FAIL, H2-03) | 404/无路由 (envoy 仅转发 /webhooks/); 网络层防护遗留项仍开放 |
| M2-01 类 (stripe/coinbase/paypal/wechat 空 txn_id) | 空串落库互斥去重 | 4xx MissingField 拒收 |
| M2-02 NATS 故障期 | fraud/paid/poll 事件丢失 (仅 warn) | 事件落 `event_outbox` pending, flusher 10s 补发, 恢复后 at-least-once 送达 (新增 live 断言: 停 NATS → 触发回调 → outbox 出现 pending 行; 重启 NATS → 行转 sent) |
| M2-05 金额展示 | `micro/1e6` f64 + 契约 number | 契约 `locked_rate_usd` 为十进制字符串, 前端 BigInt 运算, 展示仅 `toDisplayNumber` 拼一次 number 交 Intl; 门禁第 3 步对 amount.ts 无命中 |
| M2-06 后端故障 | 静默回退 mock 单号 FT-20260909 | 显式 "Payment service unavailable"; 仅 `FORGE_MOCK_FALLBACK=1` 才回退 mock |
| C3-01/C3-02 前端通道映射 | card/usdt 直发 rust → 400 unknown payment method | web 下发前映射 card→cardacq, usdt→coinbase (checkout.ts), 两通道正常锁 gateway_type (TC-58) |

## 5.6 管理后台 + 发货接线 + 风控落库 (2026-09-11, rust-core 回归单测批次)

### 纯逻辑单测 (已 PASS, 无 DB/网络)

| 用例 | 覆盖点 | 测试名 | 结论 |
|---|---|---|---|
| 发货分流: card 类可用≥需求 | decide_ship(quantity, Some("card"), available≥need) → Ship | `consumer::tests::decide_ship_card_allocates_min_of_need_and_available` | ✅ PASS |
| 发货分流: card 类卡密耗尽 | available < need → CardPoolExhausted{need,available}, 回滚保持 PAID | `consumer::tests::decide_ship_card_exhausted_when_available_below_need` | ✅ PASS |
| 发货分流: 非 card 走内容交付 | None/source/tutorial/resource → Ship(无卡密分配) | `consumer::tests::decide_ship_non_card_goes_content_delivery` | ✅ PASS |
| 发货分流: quantity 下限钳 1 | quantity=0 → need=1 | `consumer::tests::decide_ship_quantity_clamped_to_one` | ✅ PASS |
| 风控金额解析: JSON Number | dec_to_micro 读微单位整数 (Decimal 序列化形态) | `consumer::tests::dec_to_micro_reads_json_number` | ✅ PASS |
| 风控金额解析: Decimal 字符串 | dec_to_micro 兼容字符串形态 | `consumer::tests::dec_to_micro_reads_decimal_string` | ✅ PASS |
| 风控金额解析: 非数值拒绝 | Null/Bool/Array/坏字符串/浮点 → None | `consumer::tests::dec_to_micro_rejects_non_numeric` | ✅ PASS |

### live 需环境项 (需真实 PG + NATS, 本单测环境无 docker)

> 启动路径: `bash scripts/start-rust-core.sh` (自动检测 :4222/:5432, 缺则打印 docker 拉起命令); 前端 `bash scripts/start-web.sh` (读 .env.local 起 next dev :3000 + 自检 /zh), 填 `FORGE_API_BASE=http://127.0.0.1:8080` + `FORGE_ADMIN_TOKEN`(与 rust `.env` 一致)。schema 需 `001_init.sql` + `002_admin.sql` 均应用。

| 项 | 期望 | 状态 |
|---|---|---|
| 管理后台 API (/admin/* 鉴权 + 商品 CRUD + 订单 + 卡密 + 风控 + 统计) | Bearer FORGE_ADMIN_TOKEN 鉴权; 商品/卡密/订单 CRUD 实跑落库 | ⏳ 需环境 |
| 发货接线: PAID→SHIPPED 状态迁移 | 事务内 orders.status 迁移 + state_version+1 | ⏳ 需环境 |
| 发货接线: card 类卡密分配 | AVAILABLE→USED (FOR UPDATE SKIP LOCKED 防并发双发) 实跑 | ⏳ 需环境 |
| 发货接线: 卡密耗尽保持 PAID | 池可用不足 → 回滚不发货 (纯逻辑 ✅ 已测, DB 回滚语义需实跑) | ⏳ 需环境 |
| 发货接线: 非 card 内容类直接 SHIPPED | source/tutorial/resource 推进状态 | ⏳ 需环境 |
| 风控落库 fraud_events | INSERT 全字段实跑 (金额解析纯逻辑 ✅ 已测) | ⏳ 需环境 |
| 统计 /admin/stats | 聚合查询实跑 | ⏳ 需环境 |

### 5.7 live 执行清单 (可直接照抄, 需真实 PG+NATS)

前置 (一次性):

> **一键执行**: 下列 8 步已封装为 `scripts/run-live-tests.sh`, 真实环境直接 `FORGE_ADMIN_TOKEN=xxx bash scripts/run-live-tests.sh` 跑完 7 项, 末尾输出 PASS/FAIL 汇总. 下方手动命令为兜底排查用.

```bash
export BASE=http://127.0.0.1:8080
export ADMIN="Bearer $(grep -oP '(?<=FORGE_ADMIN_TOKEN=).+' apps/rust-core/.env | tr -d '"')"
export PSQL="psql -h 127.0.0.1 -U forge -d forge -tA"

# schema (已含 002_admin: products 扩展列 + card_keys + fraud_events)
psql -h 127.0.0.1 -U forge -d forge -f schema/001_init.sql
psql -h 127.0.0.1 -U forge -d forge -f schema/002_admin.sql

# 造一张测试用户 (orders.user_id 是 FK→users.id, NOT NULL; users.email 也 NOT NULL)
UID=$($PSQL -c "INSERT INTO users (uid,email) VALUES ('u-live','u-live@live.test')
                 ON CONFLICT (uid) DO NOTHING RETURNING id;" \
     -c "SELECT id FROM users WHERE uid='u-live'")
# 触发一次正常结算是前提 (webhook 需合法签名; 用既有 e2e 工具)
bash scripts/e2e-webhook.sh alipay success      # 产生 1 条 PAID 订单, 驱动 run_paid 发货
```

> 注: **两条发货路径, 语义不同** —
> - `POST /admin/orders/{out}/ship` (admin.rs:596): 纯订单端点, 仅 `PAID→SHIPPED` + state_version+1, **不分配卡密**, 非 PAID 返 409。
> - NATS `orders.events.paid` → `consumer.rs run_paid`: 履约接线, 按商品 category 分流 — card 类从卡密池 `AVAILABLE→USED` 绑定订单, 耗尽回滚保持 PAID, 内容类直接 SHIPPED。**只有 core 进程在跑且 NATS 可达时, 订单进入 PAID 后自动触发。**
> 卡密分配验证必须走 NATS 路径 (步骤 4/5/6), 不能用 admin ship 端点替代。
> 清单里的 `nats pub orders.events.paid '{...}'` 是**直发短路**验证消费者逻辑; 生产可靠路径是 `publish_order_paid` → `event_outbox` (M2-02) → flusher 补发, 步骤 4/5/6 的 SQL 断言对两条路径都成立。

1) 鉴权 fail-closed (无 token → 全 401)

```bash
curl -s -o /dev/null -w '%{http_code}\n' "$BASE/admin/products"        # 期望 401
curl -s -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer x" "$BASE/admin/products"  # 期望 401
```

2) 商品 CRUD 实跑

```bash
# create
curl -s -H "$ADMIN" -H 'Content-Type: application/json' \
  -d '{"sku":"KCARD-01","title":"AI 卡密","category":"card","unitPriceMicro":99000000,"currency":"CNY","stock":10,"active":true}' \
  "$BASE/admin/products"
# 期望 201 {"id":N}; SQL 断言:
$PSQL -c "SELECT id,sku,category,unit_price FROM products WHERE sku='KCARD-01'"  # N,KCARD-01,card,99.000000

# list + 过滤
curl -s -H "$ADMIN" "$BASE/admin/products?category=card"               # 数组含 KCARD-01
# update
curl -s -X PUT -H "$ADMIN" -H 'Content-Type: application/json' \
  -d '{"sku":"KCARD-01","title":"AI 卡密 v2","category":"card","unitPriceMicro":99000000,"currency":"CNY","stock":10,"active":true}' \
  "$BASE/admin/products/1"                                              # 200
# 库存调整
curl -s -X POST -H "$ADMIN" -H 'Content-Type: application/json' -d '{"delta":-3}' "$BASE/admin/products/1/stock"  # 204
$PSQL -c "SELECT stock FROM products WHERE id=1"                        # 7
# delete
curl -s -o /dev/null -w '%{http_code}\n' -X DELETE -H "$ADMIN" "$BASE/admin/products/1"  # 204
$PSQL -c "SELECT count(*) FROM products WHERE id=1"                     # 0
```

3) 订单管理实跑 (复用 e2e 产生的订单)

```bash
OUT=$($PSQL -c "SELECT out_trade_no FROM orders WHERE status='PAID' LIMIT 1")
curl -s -H "$ADMIN" "$BASE/admin/orders"                          # 数组
curl -s -H "$ADMIN" "$BASE/admin/orders/$OUT"                     # 详情 {order, transactions[], cardKeys[]}
$PSQL -c "SELECT status FROM orders WHERE out_trade_no='$OUT'"    # SHIPPED (run_paid 已接线)
```

4) 发货接线: card 类卡密分配 (AVAILABLE→USED, 防并发双发) — 走 NATS run_paid

```bash
PID=$($PSQL -c "SELECT id FROM products WHERE category='card' LIMIT 1")
# 生成 3 张 (admin 端点, 独立于发货路径)
curl -s -H "$ADMIN" -H 'Content-Type: application/json' -d "{\"productId\":$PID,\"count\":3}" "$BASE/admin/card-keys"  # {count:3, ids[]}
$PSQL -c "SELECT status,count(*) FROM card_keys WHERE product_id=$PID GROUP BY status"  # AVAILABLE,3
# 驱动发货: 造一张 PAID card 订单, 发 NATS paid 事件 (或由 e2e-webhook 产生)
OID=$($PSQL -c "INSERT INTO orders (out_trade_no,user_id,product_id,quantity,expected_amount,currency,gateway_type,status)
          VALUES ('LIVE-CARD-01',$UID,$PID,2,198.000000,'CNY','alipay','PAID') RETURNING id")
nats pub orders.events.paid '{"order_id":'$OID',"out_trade_no":"LIVE-CARD-01"}'
sleep 2
$PSQL -c "SELECT status FROM orders WHERE out_trade_no='LIVE-CARD-01'"  # SHIPPED
$PSQL -c "SELECT status,count(*) FROM card_keys WHERE product_id=$PID GROUP BY status"  # USED,2 / AVAILABLE,1
$PSQL -c "SELECT count(*) FROM card_keys WHERE order_id=(SELECT id FROM orders WHERE out_trade_no='LIVE-CARD-01') AND status='USED'"  # 2
```

5) 发货接线: 卡密耗尽保持 PAID — 走 NATS run_paid

```bash
PID2=$($PSQL -c "SELECT id FROM products WHERE category='card' LIMIT 1")
$PSQL -c "DELETE FROM card_keys WHERE product_id=$PID2"          # 清空池
OID=$($PSQL -c "INSERT INTO orders (out_trade_no,user_id,product_id,quantity,expected_amount,currency,gateway_type,status)
          VALUES ('LIVE-CARD-EX',$UID,$PID2,1,99.000000,'CNY','alipay','PAID') RETURNING id")
nats pub orders.events.paid '{"order_id":'$OID',"out_trade_no":"LIVE-CARD-EX"}'
sleep 2
$PSQL -c "SELECT status FROM orders WHERE out_trade_no='LIVE-CARD-EX'"  # 期望仍 PAID (耗尽不发货)
$PSQL -c "SELECT count(*) FROM card_keys WHERE product_id=$PID2 AND status='USED'"  # 0
# 反证: admin ship 端点仍会把 PAID→SHIPPED (它不查卡密池), 验证语义差异
curl -s -X POST -H "$ADMIN" "$BASE/admin/orders/LIVE-CARD-EX/ship"  # 200 {"shipped":"LIVE-CARD-EX"}
$PSQL -c "SELECT status FROM orders WHERE out_trade_no='LIVE-CARD-EX'"  # SHIPPED (端点路径)
```

6) 发货接线: 非 card 内容类直接 SHIPPED (无卡密分配) — 走 NATS run_paid

```bash
RPID=$($PSQL -c "SELECT id FROM products WHERE category='resource' OR category='source' LIMIT 1")
OID=$($PSQL -c "INSERT INTO orders (out_trade_no,user_id,product_id,quantity,expected_amount,currency,gateway_type,status)
          VALUES ('LIVE-RES-01',$UID,$RPID,1,49.000000,'CNY','alipay','PAID') RETURNING id")
nats pub orders.events.paid '{"order_id":'$OID',"out_trade_no":"LIVE-RES-01"}'
sleep 2
$PSQL -c "SELECT status FROM orders WHERE out_trade_no='LIVE-RES-01'"  # SHIPPED
$PSQL -c "SELECT count(*) FROM card_keys WHERE order_id=(SELECT id FROM orders WHERE out_trade_no='LIVE-RES-01')"  # 0
```

7) 风控落库 fraud_events

```bash
# 触发: 金额不一致回调 (e2e 有 mismatch 用例), 或手工 INSERT 验证落库
$PSQL -c "INSERT INTO fraud_events (out_trade_no,order_id,reason,received_amount,expected_amount,currency,gateway_type,evidence)
          VALUES ('$OUT',NULL,'amount_mismatch',99.000000,100.000000,'CNY','alipay','{}'::jsonb)
          RETURNING id,resolved"
# 期望 (N,false); SQL 断言:
$PSQL -c "SELECT id,resolved,resolved_by FROM fraud_events WHERE reason='amount_mismatch' ORDER BY id DESC LIMIT 1"  # N,NULL
# 标记解决
$PIDF=$($PSQL -c "SELECT max(id) FROM fraud_events")
curl -s -o /dev/null -w '%{http_code}\n' -X POST -H "$ADMIN" "$BASE/admin/fraud/$PIDF"  # 204
$PSQL -c "SELECT resolved,resolved_by FROM fraud_events WHERE id=$PIDF"  # t,admin
```

8) 统计 /admin/stats

```bash
curl -s -H "$ADMIN" "$BASE/admin/stats"
# 断言: todayOrders/pendingShip/availableCardKeys/unresolvedFraud 为非负 int;
# ordersByStatus[] 行数 = 实际 orders.status 分布; revenueByGateway[] 按 gateway_type 聚合。
# SQL 交叉校验:
$PSQL -c "SELECT status,count(*) FROM orders GROUP BY status"
$PSQL -c "SELECT count(*) FROM card_keys WHERE status='AVAILABLE'"
$PSQL -c "SELECT count(*) FROM fraud_events WHERE resolved=false"
```

**通过判据**: 以上 8 步每条 SQL 断言与期望一致 → §5.6 的 7 个 ⏳ 项全部转 ✅。任一发货/风控步骤的 DB 回滚语义(耗尽不发货、内容类不分配)与 SQL 终态不符 → 记入 CODE_REVIEW §11 C4-01 复测。

## 5.8 会员等级 + 内容交付 + 文件下载 (2026-09-12, rust-core membership/deliveries + web 会员中心/我的文件)

### 纯逻辑单测 (已 PASS, 无 DB/网络)

| 用例 | 覆盖点 | 测试名 | 结论 |
|---|---|---|---|
| 等级状态机: 升级仅允许更高级 | Free→Plus/Pro ok, Plus→Pro ok | `domain::membership::tests::transition_rejects_downgrade_and_flat` | ✅ PASS |
| 等级状态机: 降级/平级拒绝 | Plus→Free / Pro→Free / Free→Free err | `domain::membership::tests::transition_rejects_downgrade_and_flat` | ✅ PASS |
| 等级 DB 值往返 | free/plus/pro 可序列化/反序列化; 未知值 err | `domain::membership::tests::db_value_roundtrip` | ✅ PASS |

### live 需环境项 (需真实 PG + NATS, schema 需追加 003_membership.sql)

> 前置: `psql -f schema/003_membership.sql` (membership_tiers / users 会员字段 / product_deliveries / uploaded_files + 种子等级)。

| 项 | 期望 | 状态 |
|---|---|---|
| 会员等级 /user/tier | 返回 free/plus/pro 三档 (priceMicro/discountPct/maxDownloads/perks) | ⏳ 需环境 |
| 会员升级 /user/profile POST | free→plus 成功; free→free / 降级 400 | ⏳ 需环境 |
| 交付物落库 product_deliveries | run_paid: 内容类 INSERT delivery (file/content), 卡密类绑定 card_key | ⏳ 需环境 |
| 下载额度约束 /user/deliveries/{id}/download | 超等级 max_downloads 返 403; 下载计数 +1 | ⏳ 需环境 |
| 会员自动晋升 | 结算后 total_spent_micro ≥ 1e8 且当前 free → 升 plus (webhook) | ⏳ 需环境 |
| admin 上传文件 /admin/deliveries/files | 登记交付文件元数据, 绑定 product | ⏳ 需环境 |
| admin 设等级 /admin/users/{uid}/tier | 手动调级 (续期/降级通道) | ⏳ 需环境 |

### 5.9 live 执行清单 (会员/交付, 可直接照抄)

```bash
export BASE=http://127.0.0.1:8080
export ADMIN="Bearer $(grep -oP '(?<=FORGE_ADMIN_TOKEN=).+' apps/rust-core/.env | tr -d '"')"
export PSQL="psql -h 127.0.0.1 -U forge -d forge -tA"

# schema 追加会员层
psql -h 127.0.0.1 -U forge -d forge -f schema/003_membership.sql

# 造用户 (users.email NOT NULL, uid UNIQUE)
UID=$($PSQL -c "INSERT INTO users (uid,email) VALUES ('u-member','u-member@live.test')
                 ON CONFLICT (uid) DO NOTHING RETURNING id;" \
     -c "SELECT id FROM users WHERE uid='u-member'")

# 1) 等级列表
curl -s "$BASE/user/tier"   # 期望 3 档 free/plus/pro, 含 priceMicro/discountPct/maxDownloads/perks

# 2) 升级 free→plus
curl -s -X POST -H 'Content-Type: application/json' \
  -d "{\"uid\":\"u-member\",\"tier_code\":\"plus\"}" "$BASE/user/profile"   # {uid,tierCode:"plus"}
$PSQL -c "SELECT tier_code FROM users WHERE uid='u-member'"                # plus

# 3) 降级拒绝 (plus→free 非法)
curl -s -o /dev/null -w '%{http_code}\n' -X POST -H 'Content-Type: application/json' \
  -d '{"uid":"u-member","tier_code":"free"}' "$BASE/user/profile"          # 期望 400

# 4) 交付物列表 (空) + 上传登记
curl -s "$BASE/user/deliveries?uid=u-member"                              # []
curl -s -H "$ADMIN" -H 'Content-Type: application/json' \
  -d '{"product_id":1,"file_name":"src.zip","storage_key":"s3://b/1/src.zip","size_bytes":1024}' \
  "$BASE/admin/deliveries/files"                                            # 201 {id:N}
$PSQL -c "SELECT id,product_id,file_name,active FROM uploaded_files ORDER BY id DESC LIMIT 1"

# 5) 下载额度 (plus 档 max_downloads, 超限 403)
DID=$($PSQL -c "SELECT max(id) FROM product_deliveries WHERE user_id=$UID")
curl -s -o /dev/null -w '%{http_code}\n' -X POST "$BASE/user/deliveries/$DID/download?uid=u-member"
$PSQL -c "SELECT download_count FROM product_deliveries WHERE id=$DID"      # 1

# 6) 自动晋升: 造一笔结算使 total_spent_micro ≥ 1e8
$PSQL -c "UPDATE users SET total_spent_micro=100000000, tier_code='free' WHERE id=$UID"
# (经 e2e-webhook 触发一次 PAID 订单结算; webhook 累加并升级)
$PSQL -c "SELECT tier_code FROM users WHERE id=$UID"                       # plus (跨阈值自动晋升)
```

**通过判据**: 上述每步与期望一致 → §5.8 全部 ⏳ 转 ✅。等级方向 (升级 ok / 降级 400)、下载额度 (超限 403)、自动晋升 (跨阈值 free→plus) 与 SQL 终态不符 → 记入 CODE_REVIEW §11 C5-01。

## 6. 全系统无死角核对表

| 层 | 已审文件 | 死角结论 |
|---|---|---|
| rust-core | 19/19 src 文件 | 事件白名单缺口 2 处 (C2-01/C2-02); 确认数旁路 1 处 (C2-03); CHECK 违例 1 处 (B2-01) |
| go-plugins | 6/6 | 公开 preview 无边界 (H2-04); 上传通道 fail-closed PASS |
| web | 16/19 (+3 低价值文件: tailwind.config.ts / tsconfig.json / package-lock.json, 无逻辑) | 浮点展示层逃逸门禁 (M2-05); ko 死路由 (L2-02); mock 回退 (M2-06); 无浏览器→后端直连 (fetch 全在服务端 action/组件内, 无需反代) |
| schema | 1/1 | CHECK 词表与代码漂移 2 处 (B2-01, L2-01) |
| k8s | 4/4 | env 缺 6 组 (H2-02); envoy 路径泄漏 (H2-03); matchPolicy 弃用 (L2-04) |
| compose | 1/1 | Redlock 注释无实现 (H2-05) |
| 文档 | architecture.md | 与代码偏差 2 处: "Redlock 防超卖" 未实现; "锁定汇率" 无写路径 |

**结论**: 47/50 文件实读, 3 个死角文件均为无逻辑文件。资金安全类最高风险 3 项: **H2-03 (/internal 外泄) > C2-01/C2-02 (未扣款/退款入账) > B2-01 (币种分支事务回滚 + 告警盲区)**。
