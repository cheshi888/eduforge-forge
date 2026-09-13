#!/usr/bin/env bash
# H-10 安全策略门禁: 金额路径禁用 f32/f64 (docs/architecture.md 金额节: 全程 Decimal/整数微单位)。
# CI 用法: 仓库 CI 门禁步骤调用本脚本; 本地直接执行即可。
set -euo pipefail
cd "$(dirname "$0")/.."

CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"

# 1) rust-core: float_policy 单测断言 (读取全量源码扫描, 注释行已剥离, 防策略字样误报)
(cd apps/rust-core && "$CARGO" test float_policy --jobs 1)

# 2) go-plugins: 计价路径禁用 float64/float32;
#    白名单: OTel 指标埋点 attribute.Float64 (插件切换耗时观测, 非金额运算)
if grep -rnE "float64|float32" apps/go-plugins --include="*.go" 2>/dev/null \
    | grep -vE "attribute\.Float64|SetAttributes"; then
    echo "FAIL: floating point detected in go-plugins pricing path"
    exit 1
fi

# 3) M2-05: web 展示层金额换算禁用 f64 — 微单位/汇率只做整数运算 (BigInt) 或字符串,
#    违例模式: 微单位裸除法 (micro / 1_000_000; 无 n 后缀) 或 金额*汇率 浮点乘
if grep -nE "/ *1_?000_?000[^nN]| *\* *rate\b|rate *\*| *1e6\b" apps/web/lib/amount.ts; then
    echo "FAIL: floating point detected in web amount display path"
    exit 1
fi

echo "security policy gate: PASS"
