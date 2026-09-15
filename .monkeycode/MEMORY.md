# User Instruction Memory

This file records user instructions, preferences, and teachings for reference in future interactions.

## Format

### User Instruction Entry
User instruction entries should follow this format:

[User Instruction Summary]
- Date: [YYYY-MM-DD]
- Context: [Mentioned scenario or time]
- Instructions:
  - [Content of user teaching or instruction, described line by line]

### Project Knowledge Entry
Entries discovered by the Agent during task execution should follow this format:

[Project Knowledge Summary]
- Date: [YYYY-MM-DD]
- Context: Discovered by Agent while performing [specific task description]
- Category: [Operations & Deployment|Build Methods|Testing Methods|Troubleshooting & Debugging|Workflow & Collaboration|Environment Configuration]
- Instructions:
  - [Specific knowledge points, described line by line]

## Deduplication Strategy
- Before adding a new entry, check for similar or identical instructions.
- If a duplicate is found, skip the new entry or merge it with the existing one.
- When merging, update the context or date information.
- This helps avoid redundant entries and keeps the memory file tidy.

## Entries

[Project Knowledge Summary]
- Date: 2026-09-10
- Context: Discovered by Agent while building apps/go-plugins (go-plugin v1.6.0 + otel stack)
- Category: Troubleshooting & Debugging
- Instructions:
  - `go mod tidy` 在 go-plugins 模块会因 genproto 歧义导入失败: 先执行 `go get google.golang.org/genproto@latest` 再 tidy
  - go-plugin v1.6.0 netrpc 约定: `ClientConfig` 用 `Plugins map[string]plugin.Plugin` (无 PluginName 字段); `ClientProtocol` 只有 `Dispense/Ping/Close` (无 Conn); `plugin.Plugin` 接口为 `Server(*MuxBroker)(interface{}, error)` + `Client(*MuxBroker, *rpc.Client)(interface{}, error)`; RPC 方法名固定为 `Plugin.<Method>`
  - Go 1.25 的 net/rpc 没有 `rpc.Error` 结构体, 服务端方法失败直接 `return err`
  - otelgin v0.53.0 用 `otelgin.Middleware(name, otelgin.WithTracerProvider(tp))`, 无 Meter 选项; otlptracegrpc v1.28 `New` 第一个参数是 context

[Project Knowledge Summary]
- Date: 2026-09-10
- Context: Discovered by Agent while verifying the EduForge monorepo toolchains
- Category: Environment Configuration
- Instructions:
  - Rust 工具链已通过 rustup 安装: rustc/cargo 1.98.1 位于 `~/.cargo/bin/` (默认 PATH 不含, 须用全路径); 依赖已全量编译缓存于 apps/rust-core/target (约 430M); 本机 7.8Gi 无 swap, Rust 构建必须 `--jobs 1` 且走 background_terminal
  - Go 工具链 go1.25.6 可用; 热插拔冒烟测试产物在 /tmp/eduforge-plugins/ (second-half 插件 + host-demo), 复跑: `go build -o /tmp/eduforge-plugins/second-half ./cmd/plugin-second-half && go build -o /tmp/eduforge-plugins/host-demo ./cmd/host-demo && /tmp/eduforge-plugins/host-demo /tmp/eduforge-plugins/second-half`

[Project Knowledge Summary]
- Date: 2026-09-10
- Context: Discovered by Agent while resolving apps/web dependency conflicts (next + next-intl)
- Category: Build Methods
- Instructions:
  - apps/web 版本约束: next-intl 必须 >=3.24 (3.20 的 peer range 不含 next 15, 会 ERESOLVE), 当前用 3.26.0; next 用 15.5.25 (15.1.6 有 CVE-2025-66478)
  - Next 15 中 allowedHosts 配置在 `server.allowedHosts`, 不再在 `experimental.allowedHosts` (放错位置 tsc 报 TS2353)
  - 前端类型验证: `cd /workspace/apps/web && npx tsc --noEmit` (需先 npm install, 安装约 1-2 分钟, 建议后台执行)

[Project Knowledge Summary]
- Date: 2026-09-10
- Context: Discovered by Agent while fixing all compile errors in apps/rust-core (cargo check 从 30 errors 到 0)
- Category: Build Methods
- Instructions:
  - sqlx 0.8 中 `Transaction` 本身不实现 `Executor`, `sqlx::query(...).execute()` 必须传解引用后的 `&mut PgConnection`: 本地 `tx: Transaction` 传 `&mut *tx`, 参数 `&mut Transaction` 传 `&mut **tx`
  - async-nats 0.50: JetStream 上下文类型是 `async_nats::jetstream::Context` (无 JetStream 类型), `jetstream::new(client)` 同步返回; `publish` 的 payload 必须 `bytes::Bytes`; 主题 `ToSubject` 仅实现 `&'static str`/`String`/`Subject`, 局部 `&str` 先 `.to_string()`
  - rsa 0.9 验签: `RsaPublicKey::verify(scheme, hashed, sig)` 需**预哈希** digest (`Sha256::digest(msg)`), sig 传原始 `&[u8]`; `Pkcs1v15Sign::new::<sha2::Sha256>()` 零参泛型, 需 sha2 开 `oid` feature; PEM 解析导入 trait: PKCS#1 `rsa::pkcs1::DecodeRsaPublicKey::from_pkcs1_pem(&str)`, SPKI `rsa::pkcs8::DecodePublicKey::from_public_key_pem(&str)`
  - rust_decimal: 解析用 `Decimal::from_str` (需 `use std::str::FromStr`, 无 `try_from_str`); `Decimal::new` 参数是 `i64` 不是 i128
  - 验证命令: `cd /workspace/apps/rust-core && ~/.cargo/bin/cargo check --jobs 1` (依赖已缓存, 单 crate 约 5s)

[Project Knowledge Summary]
- Date: 2026-09-10
- Context: Discovered by Agent while adding alipay signature regression tests in apps/rust-core
- Category: Troubleshooting & Debugging
- Instructions:
  - 本依赖图 (spki 0.7 / der 0.7 / rsa 0.9) 中 SPKI PEM 解析方法是 `from_public_key_pem` (非 0.8+ 的 `from_pkcs8_pem`), 需 `use rsa::pkcs8::DecodePublicKey`; `to_pkcs1_der()`/`to_public_key_der()` 返回 `der::Document`, 取字节用 `.as_bytes()`
  - 读公钥 n/e 需 `use rsa::traits::PublicKeyParts`; 生成 PKCS#1 测试 PEM 用 `EncodeRsaPublicKey::to_pkcs1_der` (openssl 3.0.19 没有 `-RSAPUBOUT` 选项, 报 Unknown cipher)
  - base64 STANDARD 陷阱: 任何长度%4==0 且字符在 [A-Za-z0-9+/] 的串 (含全 hex 串) 都能静默 decode 成功, 编码格式探测必须按规范主格式优先 (支付宝 sign=hex, 先 hex 后 base64), 否则合法回调被拒
  - 验签类单测的签名基必须与校验侧重建逻辑逐字节一致 (alipay verify 用 BTreeMap ASCII 排序重建), 测试里手写未排序 base 串会导致 SignatureInvalid 假故障
  - cargo test: test profile 冷编译约 5min, 增量 10-30s; 输出日志在 /tmp/terminal_term_*.log, `echo EXIT=$?` 会掩盖真实退出码, 需读日志尾部

[Project Knowledge Summary]
- Date: 2026-09-10
- Context: Discovered by Agent while closing B-01 internal routes and updating the review report
- Category: Workflow & Collaboration
- Instructions:
  - EduForge 现行代码审查/测试报告的**唯一**位置是 `/workspace/docs/CODE_REVIEW_20260910.md` (仓库根 `docs/`, 不在 `apps/rust-core/docs/`); 该文件含 §0 风险速览 + §3 缺陷表 + §8 用例矩阵/JSON 摘要/`fixed_in_review` + 下一步建议, 修复后按既有范式更新这些位置
  - 修复项更新范式: §0 速览行尾加 `【已修复: ...】` 标记; §3 状态列改 `已修复(证据)`; TC 行结论改 `部分 PASS(...)`; JSON issues 条目 status 改 fixed 并补 `fix`+`regression` 字段; 同步更新 `testcases` 计数与 `fixed_in_review` 数组
  - 本机无 psql/docker/postgres: 涉及实库的 live 验证一律标 "需环境" 并在报告 `not_executed` 中列出复现命令

[Project Knowledge Summary]
- Date: 2026-09-10
- Context: Discovered by Agent while implementing the H-10 security policy gate (禁 f32/f64 金额策略)
- Category: Build & Compilation
- Instructions:
  - 安全策略门禁: 仓库根 `scripts/check-security-policy.sh` (可直接执行, 亦可接 CI) — ① `cargo test float_policy` (src/gateway/mod.rs 单测, include_str! 扫描全 19 个 .rs 源码, 剥离注释行 + 令牌运行时拼接避免自命中) ② go-plugins float64 grep (OTel 埋点 `attribute.Float64` 白名单, 非金额范围)
  - 金额相关代码改动后, 提交前跑一次该门禁脚本; 新增 .rs 文件需登记进 `SOURCES` 数组 (gateway/mod.rs float_policy 模块)

[Project Knowledge Summary]
- Date: 2026-09-10
- Context: Discovered by Agent while writing the e2e webhook script (报告 §8.4 live 验证缺口)
- Category: Testing Methods
- Instructions:
  - `scripts/e2e-webhook.sh` 一键复现 live TC-03/04/05/11/12/13: docker compose 起 pg/redis/nats/minio → psql 建 schema + seed (时间戳唯一行, 可重跑) → 本地生成 alipay RSA 密钥对 (签法与 cargo test alipay 一致, 无需网关沙箱凭据) → 每个 /webhooks 请求带入口 HMAC 头 x-forge-signature
  - 运行前置: docker + psql + curl + openssl + python3 + cargo; JetStream 无流定义时 publish 失败, 脚本会先经 NATS 监控口 8222 创建 eduforge-events 流 (subjects: orders.events.>/fraud.events.>/crypto.events.>), fraud 告警断言按消息数增量 (≥3: 1 paid + 2 fraud)
  - 本机无 docker, 该脚本仅静态校验 (bash -n), 实跑留给有 docker 的机器; 报告 §8.3/§8.4 与下一步建议第 5 条已指向该脚本

[Project Knowledge Summary]
- Date: 2026-09-13
- Context: Discovered by Agent while running full e2e on the EduForge deliverables-file upload/download feature (Docker deps + Next.js proxy)
- Category: Troubleshooting & Debugging
- Instructions:
  - axum 0.7.x 路由模板路径参数用 `:param` 旧语法 (底层 matchit 0.7.3), 写 `{param}` 会静默不匹配落入 404 fallback; 全仓统一 `:param`
  - sqlx 严格类型映射: DB `smallint`/`int2` 列 (membership_tiers.discount_pct/max_downloads) 对应 Rust `i16`, 写成 `i32` 会 decode 报 "mismatched types" 500; 派生表 JOIN 外层引用子查询未 SELECT 的列 (如 `u.id`) 会 "column does not exist"
  - 单端口预览下浏览器无法直连后端 8080: 前端 `next.config.ts` 用 `rewrites` 把 `/api/:path*` 反代到后端 (server-side, 目标读 `FORGE_API_PROXY_TARGET` 缺省 `http://127.0.0.1:8080`), 4 处 `API_BASE` 一律同源 `"/api"`, 免 CORS
  - 客户端 bundle 只内联 `NEXT_PUBLIC_*` 前缀环境变量: admin token 用 `NEXT_PUBLIC_FORGE_ADMIN_TOKEN`, 无前缀变量在客户端为 undefined → 空 Bearer → 401
  - 本机 docker compose 已起 postgres/nats 两容器 (镜像走 DaoCloud 加速), rust-core `.env` 配 FORGE_ADMIN_TOKEN=dev-admin-token-2026; 交付文件落盘 `uploads/{pid}/`, 登记 `local://` 前缀 storage_key, 下载 handler 按 `local://` 剥离前缀读本地文件
