#!/usr/bin/env bash
# run-live-tests.sh — 在真实 PG + NATS + 运行中的 rust-core 环境跑 §5.7 全部 7 项 live.
# 每步断言 PASS/FAIL, 末尾汇总; 任一断言失败即 FAIL. 需前置 schema 001+002 已应用.
set -uo pipefail   # 不用 -e: 断言要用条件分支, 失败累计而非立即中断

# --- 配置 (可用环境变量覆盖) ---
export HOST=${FORGE_HOST:-127.0.0.1}
export PORT=${FORGE_PORT:-8080}
export PGUSER=${FORGE_PGUSER:-forge}
export PGPASS=${FORGE_PGPASS:-forge}
export DB=${FORGE_DB:-forge}
BASE="http://$HOST:$PORT"

# admin token: 优先 env, 否则从 rust .env 读
ADMIN_TOKEN=${FORGE_ADMIN_TOKEN:-$(grep -oP '(?<=FORGE_ADMIN_TOKEN=).+' /workspace/apps/rust-core/.env 2>/dev/null | tr -d '"')}
if [ -z "$ADMIN_TOKEN" ]; then
  echo "[setup] FORGE_ADMIN_TOKEN 缺失 (env 或 apps/rust-core/.env); fail-closed 下 401, 测试无法继续"
  exit 2
fi
PSQL="psql -h $HOST -U $PGUSER -d $DB -tA"

# core 在跑? (打一个 401 探针)
CODE=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/admin/products" || echo 000)
if [ "$CODE" != "401" ]; then
  echo "[setup] core 未就绪: $BASE/admin/products 期望 401, 实际 $CODE. 先跑 scripts/start-rust-core.sh"
  exit 2
fi

PASS=0; FAIL=0
ok(){ PASS=$((PASS+1)); echo "  ✅ PASS: $1"; }
no(){ FAIL=$((FAIL+1)); echo "  ❌ FAIL: $1"; }
# assert_eq <actual> <expected> <label>
assert_eq(){ if [ "$1" = "$2" ]; then ok "$3"; else no "$3 (期望 [$2], 实际 [$1])"; fi; }

echo "=== §5.7 live 执行: $PASS 基线 ==="

# ---------- 0. 前置数据 ----------
echo "--- 0. 前置 (schema + 测试用户) ---"
UIDLIVE=$($PSQL -c "INSERT INTO users (uid,email) VALUES ('u-live','u-live@live.test')
              ON CONFLICT (uid) DO NOTHING RETURNING id;" \
           -c "SELECT id FROM users WHERE uid='u-live'" | tail -1)
if [ -z "$UIDLIVE" ]; then no "造测试用户失败"; else ok "测试用户 id=$UIDLIVE"; fi

# ---------- 1. 鉴权 fail-closed ----------
echo "--- 1. 鉴权 fail-closed ---"
assert_eq "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/admin/products")" "401" "无 token → 401"
assert_eq "$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer bad" "$BASE/admin/products")" "401" "错误 token → 401"
assert_eq "$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $ADMIN_TOKEN" "$BASE/admin/products")" "200" "正确 token → 200"

# ---------- 2. 商品 CRUD ----------
echo "--- 2. 商品 CRUD ---"
CID=$(curl -s -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' \
  -d '{"sku":"KCARD-01","title":"AI 卡密","category":"card","unitPriceMicro":99000000,"currency":"CNY","stock":10,"active":true}' \
  "$BASE/admin/products" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id",""))')
if [ -z "$CID" ]; then no "create_product 无返回 id"; else ok "create_product id=$CID"; fi
assert_eq "$($PSQL -c "SELECT category||'/'||unit_price FROM products WHERE id=$CID")" "card/99.000000" "SQL 落库 category/unit_price"
assert_eq "$(curl -s -H "Authorization: Bearer $ADMIN_TOKEN" "$BASE/admin/products?category=card" | python3 -c 'import sys,json;print(any(p["sku"]=="KCARD-01" for p in json.load(sys.stdin)))')" "True" "list?category=card 含 KCARD-01"
assert_eq "$(curl -s -o /dev/null -w '%{http_code}' -X PUT -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' -d '{"sku":"KCARD-01","title":"v2","category":"card","unitPriceMicro":99000000,"currency":"CNY","stock":10,"active":true}' "$BASE/admin/products/$CID")" "200" "update_product 200"
assert_eq "$($PSQL -c "SELECT stock FROM products WHERE id=$CID")" "7" "adjust_stock -3"
assert_eq "$(curl -s -o /dev/null -w '%{http_code}' -X DELETE -H "Authorization: Bearer $ADMIN_TOKEN" "$BASE/admin/products/$CID")" "204" "delete_product 204"

# 建一个 card 产品 (保留, 供发货步骤用)
CID_CARD=$($PSQL -c "SELECT id FROM products WHERE category='card' LIMIT 1")
if [ -z "$CID_CARD" ]; then
  CID_CARD=$(curl -s -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' \
    -d '{"sku":"KCARD-02","title":"卡密","category":"card","unitPriceMicro":99000000,"currency":"CNY","stock":99,"active":true}' \
    "$BASE/admin/products" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id",""))')
fi

# ---------- 3. 订单管理 (复用 PAID 单) ----------
echo "--- 3. 订单管理 ---"
# 造一张 PAID 单 (发货会由 run_paid 自动推进, 故先查当前 PAID; 没有就插一张)
OUT=$($PSQL -c "SELECT out_trade_no FROM orders WHERE status='PAID' LIMIT 1")
if [ -z "$OUT" ]; then
  OUT="LIVE-ORD-01"
  $PSQL -c "INSERT INTO orders (out_trade_no,user_id,product_id,quantity,expected_amount,currency,gateway_type,status)
        VALUES ('$OUT',$UIDLIVE,$CID_CARD,1,99.000000,'CNY','alipay','PAID')" >/dev/null
fi
assert_eq "$(curl -s -H "Authorization: Bearer $ADMIN_TOKEN" "$BASE/admin/orders" | python3 -c 'import sys,json;print(any(o["outTradeNo"]=="'$OUT'" for o in json.load(sys.stdin)))')" "True" "list_orders 含 $OUT"
assert_eq "$(curl -s -H "Authorization: Bearer $ADMIN_TOKEN" "$BASE/admin/orders/$OUT" | python3 -c 'import sys,json;d=json.load(sys.stdin);print("order" in d and "transactions" in d and "cardKeys" in d)')" "True" "get_order 返回 {order,transactions,cardKeys}"

# ---------- 4. card 类卡密分配 (NATS run_paid) ----------
echo "--- 4. card 卡密分配 (NATS) ---"
# 生成 3 张
GEN=$(curl -s -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' \
  -d "{\"productId\":$CID_CARD,\"count\":3}" "$BASE/admin/card-keys")
assert_eq "$(echo "$GEN" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("count"))')" "3" "generate_card_keys count=3"
assert_eq "$($PSQL -c "SELECT count(*) FROM card_keys WHERE product_id=$CID_CARD AND status='AVAILABLE'")" "3" "3 张 AVAILABLE"
# 造 PAID card 单 + 发 NATS
OID=$($PSQL -c "INSERT INTO orders (out_trade_no,user_id,product_id,quantity,expected_amount,currency,gateway_type,status)
      VALUES ('LIVE-CARD-01',$UIDLIVE,$CID_CARD,2,198.000000,'CNY','alipay','PAID') RETURNING id")
nats pub orders.events.paid "{\"order_id\":$OID,\"out_trade_no\":\"LIVE-CARD-01\"}" >/dev/null
sleep 2
assert_eq "$($PSQL -c "SELECT status FROM orders WHERE out_trade_no='LIVE-CARD-01'")" "SHIPPED" "PAID→SHIPPED"
assert_eq "$($PSQL -c "SELECT count(*) FROM card_keys WHERE order_id=$OID AND status='USED'")" "2" "分配 2 张 USED 并绑订单"

# ---------- 5. 卡密耗尽保持 PAID ----------
echo "--- 5. 卡密耗尽保持 PAID ---"
$PSQL -c "DELETE FROM card_keys WHERE product_id=$CID_CARD" >/dev/null
OID2=$($PSQL -c "INSERT INTO orders (out_trade_no,user_id,product_id,quantity,expected_amount,currency,gateway_type,status)
      VALUES ('LIVE-CARD-EX',$UIDLIVE,$CID_CARD,1,99.000000,'CNY','alipay','PAID') RETURNING id")
nats pub orders.events.paid "{\"order_id\":$OID2,\"out_trade_no\":\"LIVE-CARD-EX\"}" >/dev/null
sleep 2
assert_eq "$($PSQL -c "SELECT status FROM orders WHERE out_trade_no='LIVE-CARD-EX'")" "PAID" "耗尽仍 PAID (run_paid 回滚)"
assert_eq "$($PSQL -c "SELECT count(*) FROM card_keys WHERE product_id=$CID_CARD AND status='USED'")" "0" "无卡密被消耗"
# 反证: admin ship 端点不查池, 直接推进
SH=$(curl -s -X POST -H "Authorization: Bearer $ADMIN_TOKEN" "$BASE/admin/orders/LIVE-CARD-EX/ship")
assert_eq "$(echo "$SH" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("shipped"))')" "LIVE-CARD-EX" "admin ship 端点 {shipped} (200 非 204)"
assert_eq "$($PSQL -c "SELECT status FROM orders WHERE out_trade_no='LIVE-CARD-EX'")" "SHIPPED" "端点路径已 SHIPPED"

# ---------- 6. 非 card 内容类直接 SHIPPED ----------
echo "--- 6. 内容类直接 SHIPPED (无卡密) ---"
RPID=$($PSQL -c "SELECT id FROM products WHERE category IN ('resource','source') LIMIT 1")
if [ -z "$RPID" ]; then
  RPID=$(curl -s -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' \
    -d '{"sku":"RES-01","title":"资源包","category":"resource","unitPriceMicro":49000000,"currency":"CNY","stock":99,"active":true}' \
    "$BASE/admin/products" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id",""))')
fi
OID3=$($PSQL -c "INSERT INTO orders (out_trade_no,user_id,product_id,quantity,expected_amount,currency,gateway_type,status)
      VALUES ('LIVE-RES-01',$UIDLIVE,$RPID,1,49.000000,'CNY','alipay','PAID') RETURNING id")
nats pub orders.events.paid "{\"order_id\":$OID3,\"out_trade_no\":\"LIVE-RES-01\"}" >/dev/null
sleep 2
assert_eq "$($PSQL -c "SELECT status FROM orders WHERE out_trade_no='LIVE-RES-01'")" "SHIPPED" "内容类 PAID→SHIPPED"
assert_eq "$($PSQL -c "SELECT count(*) FROM card_keys WHERE order_id=$OID3")" "0" "未分配任何卡密"

# ---------- 7. 风控落库 fraud_events ----------
echo "--- 7. 风控落库 ---"
FID=$($PSQL -c "INSERT INTO fraud_events (out_trade_no,order_id,reason,received_amount,expected_amount,currency,gateway_type,evidence)
      VALUES ('$OUT',NULL,'amount_mismatch',99.000000,100.000000,'CNY','alipay','{}'::jsonb) RETURNING id")
assert_eq "$($PSQL -c "SELECT resolved FROM fraud_events WHERE id=$FID")" "f" "INSERT fraud_event resolved=false"
assert_eq "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $ADMIN_TOKEN" "$BASE/admin/fraud/$FID")" "204" "resolve_fraud 204"
assert_eq "$($PSQL -c "SELECT resolved||'/'||resolved_by FROM fraud_events WHERE id=$FID")" "t/admin" "resolved=true,resolved_by=admin"

# ---------- 8. 统计 ----------
echo "--- 8. /admin/stats ---"
STATS=$(curl -s -H "Authorization: Bearer $ADMIN_TOKEN" "$BASE/admin/stats")
assert_eq "$(echo "$STATS" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(all(k in d for k in ["todayOrders","todayRevenueMicro","pendingShip","availableCardKeys","unresolvedFraud"]))')" "True" "stats 含 5 个 KPI 字段"
assert_eq "$(echo "$STATS" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(isinstance(d.get("ordersByStatus"),list) and isinstance(d.get("revenueByGateway"),list))')" "True" "ordersByStatus/revenueByGateway 为数组"
# 交叉校验 availableCardKeys
assert_eq "$($PSQL -c "SELECT count(*) FROM card_keys WHERE status='AVAILABLE'")" "$(echo "$STATS" | python3 -c 'import sys,json;print(json.load(sys.stdin)["availableCardKeys"])')" "availableCardKeys 与 SQL 一致"

# ---------- §5.9 会员等级 + 内容交付 + 文件下载 ----------
# 前置: schema 003_membership.sql 已应用
if [ "$($PSQL -c "SELECT count(*) FROM membership_tiers")" -eq 0 ] 2>/dev/null; then
  echo "--- 9. [SKIP] membership_tiers 空 — 未应用 schema/003_membership.sql, 跳过 §5.9 ---"
else
  # ---------- 9. 等级列表 ----------
  echo "--- 9. 等级列表 /user/tier ---"
  assert_eq "$(curl -s "$BASE/user/tier" | python3 -c 'import sys,json;print(len(json.load(sys.stdin)))')" "3" "tiers 3 档 (free/plus/pro)"
  assert_eq "$(curl -s "$BASE/user/tier" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(any(t.get("maxDownloads") is not None for t in d))')" "True" "tiers 含 maxDownloads 字段"

  # ---------- 10. 升级 + 降级拒绝 ----------
  echo "--- 10. 升级/降级 (等级状态机) ---"
  MUID=$($PSQL -c "SELECT uid FROM users LIMIT 1")
  if [ -z "$MUID" ]; then MUID="u-member-live"; $PSQL -c "INSERT INTO users (uid,email) VALUES ('$MUID','$MUID@live.test') ON CONFLICT (uid) DO NOTHING" >/dev/null; fi
  # 先重置为 free 保证可升级
  $PSQL -c "UPDATE users SET tier_code='free' WHERE uid='$MUID'" >/dev/null
  assert_eq "$(curl -s -X POST -H 'Content-Type: application/json' -d "{\"uid\":\"$MUID\",\"tier_code\":\"plus\"}" "$BASE/user/profile" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("tierCode"))')" "plus" "free→plus 升级成功"
  assert_eq "$($PSQL -c "SELECT tier_code FROM users WHERE uid='$MUID'")" "plus" "SQL tier_code=plus"
  assert_eq "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d "{\"uid\":\"$MUID\",\"tier_code\":\"free\"}" "$BASE/user/profile")" "400" "降级 plus→free 返 400 (状态机拒绝)"

  # ---------- 11. 交付物 + 下载额度 ----------
  echo "--- 11. 交付物 + 下载额度 ---"
  # 造一条 content 交付物 (user 已购), 验证列表 + 下载计数
  MPID=$($PSQL -c "SELECT product_id FROM orders WHERE status='SHIPPED' AND user_id=$UIDLIVE LIMIT 1")
  if [ -z "$MPID" ]; then MPID=$CID_CARD; fi
  DID=$($PSQL -c "INSERT INTO product_deliveries (user_id,product_id,delivery_type,product_sku,product_title,product_category)
        VALUES ($UIDLIVE,$MPID,'content','LIVE-RES-01','测试内容','resource') RETURNING id")
  assert_eq "$($PSQL -c "SELECT count(*) FROM product_deliveries WHERE id=$DID")" "1" "content 交付物落库"
  assert_eq "$(curl -s "$BASE/user/deliveries?uid=$MUID" | python3 -c 'import sys,json;print(any(d["deliveryType"]=="content" for d in json.load(sys.stdin)))')" "True" "deliveries 列表含 content"
  assert_eq "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$BASE/user/deliveries/$DID/download?uid=$MUID")" "200" "download 200 (plus 档有额度)"
  assert_eq "$($PSQL -c "SELECT download_count FROM product_deliveries WHERE id=$DID")" "1" "download_count +1"

  # ---------- 12. admin 上传文件登记 + 设等级 ----------
  echo "--- 12. admin 上传登记 + 设等级 ---"
  UPID=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' \
    -d "{\"product_id\":$MPID,\"file_name\":\"src.zip\",\"storage_key\":\"local://uploads/1/src.zip\",\"size_bytes\":1024}" "$BASE/admin/deliveries/files")
  assert_eq "$UPID" "201" "admin 上传登记 201"
  assert_eq "$($PSQL -c "SELECT count(*) FROM uploaded_files WHERE product_id=$MPID AND file_name='src.zip'")" "1" "uploaded_files 落库"
  assert_eq "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' -d "{\"uid\":\"$MUID\",\"tier_code\":\"free\"}" "$BASE/admin/users/$MUID/tier")" "200" "admin 设等级 (降级通道) 200"
fi

# ---------- 汇总 ----------
echo "==================================="
echo "§5.7+§5.9 live 结果: PASS=$PASS FAIL=$FAIL"
if [ "$FAIL" -eq 0 ]; then
  echo "🎉 全部通过 — 把 §5.6/§5.8 的 ⏳ 项改为 ✅, 并回填 CODE_REVIEW §11 C4-01 / §11.2 C5-01"
else
  echo "存在失败, 对照上方 FAIL 行定位; DB 回滚/等级/额度语义不符记入 §11 复测"
fi
[ "$FAIL" -eq 0 ]
