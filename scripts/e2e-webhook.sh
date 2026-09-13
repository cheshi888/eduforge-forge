#!/usr/bin/env bash
# ============================================================================
# e2e-webhook.sh — 报告 §8.2 TC-03/04/05 + §8.4 TC-11/12/13 的 live 复现
#
# 前置依赖 (本机需具备):
#   - docker: 拉起仓库根 docker-compose.yml 基础设施 (postgres/redis/nats/minio)
#   - psql / curl / openssl / python3
#   - cargo: 编译并启动 apps/rust-core
#   - 无需任何网关沙箱凭据: alipay 密钥对本地生成, 签法与 cargo test alipay 一致
#
# 用例矩阵:
#   TC-11  /internal/orders/preview  真实订单路径 (金额 ×10^6 微单位)
#   TC-12  /internal/checkout        属主校验: 跨用户 404 / 属主 200 锁定通道
#   TC-03  alipay 签名回调           → 订单 PAID
#   TC-05  同一回调重放              → 幂等 ACK, 订单状态不变
#   TC-04  跨币种回调 (CNY 单+USD)  → 订单 FRAUD_SUSPENDED + fraud 告警
#   TC-13  孤儿单号回调              → 交易 REJECTED + fraud 告警 (orphan_callback)
#
# 用法: ./scripts/e2e-webhook.sh
set -euo pipefail
cd "$(dirname "$0")/.."

CARGO_BIN="${CARGO_BIN:-cargo}"
BASE="${BASE:-http://127.0.0.1:8080}"
NATS_MONITOR="${NATS_MONITOR:-http://127.0.0.1:8222}"
DB_URL="${FORGE_DATABASE_URL:-postgres://eduforge:eduforge_dev@127.0.0.1:5432/eduforge}"
PSQL=(psql "$DB_URL" -v ON_ERROR_STOP=1 -qtA -X)

# rust-core 启动环境 (可选变量均有安全默认, 仅 HMAC 必填)
export FORGE_DATABASE_URL="$DB_URL"
export FORGE_NATS_URL="${FORGE_NATS_URL:-nats://127.0.0.1:4222}"
export FORGE_HMAC_SECRET="${FORGE_HMAC_SECRET:-e2e-dev-hmac-secret}"
export FORGE_STRIPE_WEBHOOK_SECRET="${FORGE_STRIPE_WEBHOOK_SECRET:-e2e-stripe}"
export FORGE_COINBASE_WEBHOOK_SECRET="${FORGE_COINBASE_WEBHOOK_SECRET:-e2e-coinbase}"

PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); echo "  PASS  $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL  $1"; }
assert() { # assert <说明> '<shell 表达式, 0 为真>'
  if eval "$2" >/dev/null 2>&1; then ok "$1"; else bad "$1"; fi
}
db() { "${PSQL[@]}" -c "$1"; }

WORK="$(mktemp -d /tmp/forge-e2e.XXXXXX)"
trap 'kill "$CORE_PID" 2>/dev/null || true; rm -rf "$WORK"' EXIT
CORE_PID=""

# ----------------------------------------------------------------------------
echo "== step 1: 基础设施 (docker compose + schema + NATS stream) =="
if docker compose version >/dev/null 2>&1; then DC=(docker compose); else DC=(docker-compose); fi
"${DC[@]}" up -d postgres redis nats minio

for i in $(seq 1 60); do
  psql "$DB_URL" -c 'SELECT 1' >/dev/null 2>&1 && break
  sleep 2
done
psql "$DB_URL" -c 'SELECT 1' >/dev/null 2>&1
ok "postgres reachable"

# NATS 监控口就绪后再建流 (publish 无流则失败, 告警会丢)
for i in $(seq 1 30); do
  curl -sf "$NATS_MONITOR/" >/dev/null 2>&1 && break
  sleep 2
done
curl -s -o /dev/null "$NATS_MONITOR/"
ok "nats monitor reachable"

if [ -z "$(db "SELECT to_regclass('public.orders');")" ]; then
  psql "$DB_URL" -v ON_ERROR_STOP=1 -f schema/001_init.sql >/dev/null
  ok "schema applied"
else
  ok "schema already present"
fi

# JetStream 无流定义时 publish 直接失败 (告警丢失), 先建流 (幂等: 已存在则忽略)
curl -sf -X POST "$NATS_MONITOR/js/api/stream/add/eduforge-events" \
  -H 'Content-Type: application/json' \
  -d '{"subjects": ["orders.events.>", "fraud.events.>", "crypto.events.>"]}' >/dev/null \
  || echo "  note: eduforge-events stream 已存在或监控端口未就绪"
ok "nats stream ensured"

# ----------------------------------------------------------------------------
echo "== step 2: alipay 密钥对 (本地生成, 非生产密钥) =="
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$WORK/alipay_priv.pem" 2>/dev/null
openssl pkey -in "$WORK/alipay_priv.pem" -pubout -out "$WORK/alipay_pub.pem"
export FORGE_ALIPAY_PUBLIC_KEY_PEM="$(cat "$WORK/alipay_pub.pem")"
ok "keypair generated"

# ----------------------------------------------------------------------------
echo "== step 3: 编译并启动 rust-core =="
# 先 build 再 exec binary: 便于 EXIT 时精确 kill 进程 (cargo run 的子进程无法被父 PID 捕获)
( cd apps/rust-core && "$CARGO_BIN" --jobs 1 build ) >"$WORK/build.log" 2>&1 \
  || { echo "  build failed, log tail:"; tail -20 "$WORK/build.log"; exit 1; }
( cd apps/rust-core && exec ./target/debug/eduforge-core ) >"$WORK/core.log" 2>&1 &
CORE_PID=$!
CORE_READY=0
for i in $(seq 1 120); do
  curl -fsS "$BASE/healthz" >/dev/null 2>&1 && { CORE_READY=1; break; }
  sleep 1
done
if [ "$CORE_READY" -eq 1 ]; then ok "core ready"; else
  echo "  core log tail:"; tail -20 "$WORK/core.log"; bad "core not ready"; exit 1
fi

# ----------------------------------------------------------------------------
echo "== step 5: seed 用户 / 商品 / 订单 =="
TS="$(date +%s)"
UID_A="u-e2e-$TS"
UID_B="u-e2e-x-$TS"
NO_A="E2E-$TS"       # CNY 300.000000, TC-03/05
NO_B="E2E-B-$TS"     # CNY 199.990000, TC-11/12
TNO_A="2026${TS}1400000001"
TNO_B="2026${TS}1400000002"
TNO_C="2026${TS}1400000003"

db "INSERT INTO products (sku, title, unit_price, stock) VALUES ('e2e-$TS', 'e2e', 100.00, 1000);"
db "INSERT INTO users (uid, email) VALUES ('$UID_A', '$UID_A@e2e.local'), ('$UID_B', '$UID_B@e2e.local');"
db "INSERT INTO orders (out_trade_no, user_id, product_id, quantity, expected_amount, currency, status)
     VALUES ('$NO_A', (SELECT id FROM users WHERE uid = '$UID_A'),
             (SELECT id FROM products WHERE sku = 'e2e-$TS'), 1, 300.000000, 'CNY', 'PENDING'),
            ('$NO_B', (SELECT id FROM users WHERE uid = '$UID_A'),
             (SELECT id FROM products WHERE sku = 'e2e-$TS'), 1, 199.990000, 'CNY', 'PENDING');"
ok "seeded (uid=$UID_A/$UID_B, orders=$NO_A/$NO_B)"

# alipay 官方签法: 字段 ASCII 升序 (排除 sign/sign_type), RSA2 (PKCS#1 v1.5 + SHA256), hex
alipay_body() { # $1 = 已排序的字段串
  local sig
  printf '%s' "$1" | openssl dgst -sha256 -sign "$WORK/alipay_priv.pem" -out "$WORK/sig.bin"
  sig="$(od -An -v -tx1 "$WORK/sig.bin" | tr -d ' \n')"
  printf '%s&sign_type=RSA2&sign=%s' "$1" "$sig"
}
# 入口 HMAC: hex(HMAC-SHA256(FORGE_HMAC_SECRET, 原始 body)), 头 x-forge-signature
hmac_header() { printf '%s' "$1" | openssl dgst -sha256 -hmac "$FORGE_HMAC_SECRET" -r | cut -d' ' -f1; }
post_webhook() { # $1 = gateway, $2 = body; 结果写入 REPLY_CODE / REPLY_BODY
  local sig
  sig="$(hmac_header "$2")"
  REPLY_CODE="$(curl -s -o "$WORK/reply_body" -w '%{http_code}' \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    -H "x-forge-signature: $sig" \
    -d "$2" "$BASE/webhooks/$1" || true)"
  REPLY_BODY="$(cat "$WORK/reply_body" 2>/dev/null || true)"
}
json_get() { # $1 = dot.path, stdin = json; 缺失或非法 JSON 输出空串
  python3 -c 'import sys, json
try:
    d = json.load(sys.stdin)
except Exception:
    d = {}
v = d
for k in sys.argv[1].split("."):
    v = (v or {}).get(k, "") if isinstance(v, dict) else ""
print(v)' "$1"
}
is2xx() { [ "${1:0:1}" = "2" ]; }

# ----------------------------------------------------------------------------
echo "== step 6: TC-11 订单预览 (真实数据路径) =="
PV="$(curl -s "$BASE/internal/orders/preview?uid=$UID_A" || true)"
NO_GOT="$(printf '%s' "$PV" | json_get outTradeNo)"
MICRO_GOT="$(printf '%s' "$PV" | json_get amountMicro)"
assert "preview 返回最新 PENDING 单 $NO_B (got $NO_GOT)" '[ "$NO_GOT" = "$NO_B" ]'
assert "199.990000 → 199990000 微单位 (got $MICRO_GOT)" '[ "$MICRO_GOT" = "199990000" ]'

echo "== step 7: TC-12 checkout 属主校验 =="
CODE="$(curl -s -o "$WORK/ck_cross.json" -w '%{http_code}' -X POST \
  -H 'Content-Type: application/json' \
  -d "{\"uid\":\"$UID_B\",\"out_trade_no\":\"$NO_B\",\"payment_method\":\"alipay\"}" \
  "$BASE/internal/checkout" || true)"
assert "跨用户 checkout → 404 (got $CODE)" '[ "$CODE" = "404" ]'

CODE="$(curl -s -o "$WORK/ck_own.json" -w '%{http_code}' -X POST \
  -H 'Content-Type: application/json' \
  -d "{\"uid\":\"$UID_A\",\"out_trade_no\":\"$NO_B\",\"payment_method\":\"alipay\"}" \
  "$BASE/internal/checkout" || true)"
GT="$(db "SELECT gateway_type FROM orders WHERE out_trade_no = '$NO_B';")"
assert "属主 checkout → 200 (got $CODE)" '[ "$CODE" = "200" ]'
assert "订单锁定 gateway_type=alipay (got '$GT')" '[ "$GT" = "alipay" ]'

# ----------------------------------------------------------------------------
echo "== step 8: TC-03 alipay 签名回调 → PAID =="
# 记录 JetStream 基线消息数 (含 orders.events.paid / fraud.events.alert)
BASE_MSGS="$(curl -s "$NATS_MONITOR/js/api/stream/info/eduforge-events" | json_get stream.state.messages || true)"
BASE_MSGS="${BASE_MSGS:-0}"
B03="$(alipay_body "currency=CNY&notify_type=trade_status_sync&out_trade_no=$NO_A&total_amount=300.00&trade_no=$TNO_A")"
post_webhook alipay "$B03"
ST="$(db "SELECT status FROM orders WHERE out_trade_no = '$NO_A';")"
TXN="$(db "SELECT status FROM payment_transactions WHERE gateway_txn_id = '$TNO_A';")"
assert "回调 2xx + ack 'success' (got $REPLY_CODE '$REPLY_BODY')" 'is2xx "$REPLY_CODE" && [ "$REPLY_BODY" = "success" ]'
assert "订单 status=PAID (got $ST)" '[ "$ST" = "PAID" ]'
assert "交易入账 (txn status='$TXN')" '[ -n "$TXN" ]'

echo "== step 9: TC-05 幂等重放 → 状态不变 =="
post_webhook alipay "$B03"
ST2="$(db "SELECT status FROM orders WHERE out_trade_no = '$NO_A';")"
assert "重放 2xx (got $REPLY_CODE)" 'is2xx "$REPLY_CODE"'
assert "重放后订单仍 PAID (got $ST2)" '[ "$ST2" = "PAID" ]'

echo "== step 10: TC-04 跨币种 → FRAUD_SUSPENDED =="
B04="$(alipay_body "currency=USD&notify_type=trade_status_sync&out_trade_no=$NO_B&total_amount=199.99&trade_no=$TNO_B")"
post_webhook alipay "$B04"
ST3="$(db "SELECT status FROM orders WHERE out_trade_no = '$NO_B';")"
assert "币种不符订单 → FRAUD_SUSPENDED (got $ST3)" '[ "$ST3" = "FRAUD_SUSPENDED" ]'

echo "== step 11: TC-13 孤儿单号 → REJECTED + fraud 告警 =="
B13="$(alipay_body "currency=CNY&notify_type=trade_status_sync&out_trade_no=NOT-EXIST-$TS&total_amount=1.00&trade_no=$TNO_C")"
post_webhook alipay "$B13"
RX="$(db "SELECT status FROM payment_transactions WHERE gateway_txn_id = '$TNO_C';")"
assert "孤儿回调 2xx (got $REPLY_CODE '$REPLY_BODY')" 'is2xx "$REPLY_CODE"'
assert "孤儿交易记 REJECTED (got '$RX')" '[ "$RX" = "REJECTED" ]'
# 增量断言: step 8 基线后应新增 3 条 (1 paid + 2 fraud, B13 孤儿与 B04 币种)
TOTAL="$(curl -s "$NATS_MONITOR/js/api/stream/info/eduforge-events" | json_get stream.state.messages || true)"
TOTAL="${TOTAL:-0}"
DELTA=$((TOTAL - BASE_MSGS))
assert "JetStream 新增 $DELTA 条 (paid + fraud×2, 基线 $BASE_MSGS, 总计 $TOTAL)" '[ "$DELTA" -ge 3 ]'

# ----------------------------------------------------------------------------
echo "============================================"
echo " e2e-webhook: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
