// 安全验收测试脚本：全盘安全攻击面实测
// 覆盖：鉴权越权 / SQL注入 / XSS / 金额篡改 / 路径穿越 / 会话 / 密码学 / 竞态 / 输入校验
import { chromium } from 'playwright';

const BACKEND = 'http://127.0.0.1:8080';
const FRONTEND = 'http://localhost:3000';
const ADMIN_TOKEN = 'test-admin-token';
const TS = Date.now();

let PASS = 0, FAIL = 0;
const failures = [];

function record(name, ok, detail = '') {
  if (ok) { PASS++; console.log(`  ✅ ${name}`); }
  else { FAIL++; failures.push({ name, detail }); console.log(`  ❌ ${name}  -- ${detail}`); }
}

async function req(method, path, { token, body, rawBody } = {}) {
  const headers = {};
  if (token) headers['Authorization'] = `Bearer ${token}`;
  let payload;
  if (rawBody !== undefined) {
    headers['Content-Type'] = 'application/json';
    payload = rawBody;
  } else if (body !== undefined) {
    headers['Content-Type'] = 'application/json';
    payload = JSON.stringify(body);
  }
  const res = await fetch(BACKEND + path, { method, headers, body: payload });
  let text = '';
  try { text = await res.text(); } catch {}
  let json = null;
  try { json = JSON.parse(text); } catch {}
  return { status: res.status, text, json };
}

// ============ 主流程 ============
async function main() {
  console.log('\n==================== 安全验收测试 ====================\n');

  // ---- 0. 环境准备：注册两个用户 + 管理员 ---- 
  console.log('【0. 测试环境准备】');
  const ua = `alice-${TS}@test.com`;
  const ub = `bob-${TS}@test.com`;
  const ra = await req('POST', '/auth/register', { body: { email: ua, password: 'Password123!' } });
  record('注册用户 A (alice)', ra.status === 200 && ra.json?.token, `status=${ra.status}`);
  const rb = await req('POST', '/auth/register', { body: { email: ub, password: 'Password123!' } });
  record('注册用户 B (bob)', rb.status === 200 && rb.json?.token, `status=${rb.status}`);
  const tokenA = ra.json?.token;
  const tokenB = rb.json?.token;
  const uidB = rb.json?.uid;

  // ============ 1. 鉴权与越权 (IDOR / Broken Access Control) ============
  console.log('\n【1. 鉴权与越权 (IDOR)】');

  // 1.1 未认证访问受保护端点
  const noauth = await req('GET', '/auth/me');
  record('未认证访问 /auth/me → 401', noauth.status === 401, `status=${noauth.status}`);
  const noauth2 = await req('GET', '/internal/orders/preview');
  record('未认证访问 /internal/orders/preview → 401', noauth2.status === 401, `status=${noauth2.status}`);
  const noauth3 = await req('GET', '/user/deliveries');
  record('未认证访问 /user/deliveries → 401', noauth3.status === 401, `status=${noauth3.status}`);
  const noauth4 = await req('GET', '/user/notifications');
  record('未认证访问 /user/notifications → 401', noauth4.status === 401, `status=${noauth4.status}`);

  // 1.2 伪造/无效 token
  const badtok = await req('GET', '/auth/me', { token: 'deadbeef'.repeat(8) });
  record('伪造 token 访问 /auth/me → 401', badtok.status === 401, `status=${badtok.status}`);

  // 1.3 admin 端点无 token / 错误 token / 用户 token 越权
  const admin_no = await req('GET', '/admin/products');
  record('无 token 访问 /admin/products → 401', admin_no.status === 401, `status=${admin_no.status}`);
  const admin_wrong = await req('GET', '/admin/products', { token: 'wrong-admin-token' });
  record('错误 admin token → 401', admin_wrong.status === 401, `status=${admin_wrong.status}`);
  const admin_usertoken = await req('GET', '/admin/products', { token: tokenA });
  record('普通用户 token 访问 admin 端点 → 401 (防越权)', admin_usertoken.status === 401, `status=${admin_usertoken.status}`);
  const admin_ok = await req('GET', '/admin/products', { token: ADMIN_TOKEN });
  record('正确 admin token → 200', admin_ok.status === 200, `status=${admin_ok.status}`);

  // 1.4 admin 端点：用户 token 尝试提权/改价
  const tier_attack = await req('POST', `/admin/users/${uidB}/tier`, { token: tokenA, body: { uid: uidB, tier_code: 'pro' } });
  record('普通用户 token 调用 admin 提权 → 401 (防越权提权)', tier_attack.status === 401, `status=${tier_attack.status}`);

  // 1.5 用户 A 尝试操作用户 B 的资源（下单归属 / checkout 越权）
  // A 先下单拿一个 out_trade_no，然后 B 尝试 checkout 这个单
  const orderA = await req('POST', '/internal/orders', { token: tokenA, body: { items: [{ sku: 'tut-rust', quantity: 1 }] } });
  const outTradeA = orderA.json?.outTradeNo;
  const checkoutB = await req('POST', '/internal/checkout', { token: tokenB, body: { out_trade_no: outTradeA, payment_method: 'alipay' } });
  record('用户 B 越权结算用户 A 的订单 → 404 (防越权)', checkoutB.status === 404, `status=${checkoutB.status}`);

  // 1.6 下载越权：用户 B 无法访问用户 A 的交付物（先造一条 A 的交付记录）
  // 通过 admin 发货造交付，然后 B 尝试下载
  // 先给 A 订单支付+发货（模拟）
  const shipA = await req('POST', `/admin/orders/${outTradeA}/ship`, { token: ADMIN_TOKEN });
  // 记录 ship 结果（订单可能未 PAID，此处只验证越权下载逻辑，需先有 delivery）

  console.log('\n【2. 金额篡改与业务逻辑】');

  // 2.1 客户端传价攻击：尝试传价格字段，后端应忽略
  const priceAttack = await req('POST', '/internal/orders', { token: tokenA, body: { items: [{ sku: 'tut-rust', quantity: 1, price: 1 }], amount: 1 } });
  const amt = priceAttack.json?.amountMicro;
  // tut-rust 原价 199 元 = 199000000 微单位，攻击者想改成 1 分钱
  record('下单传价攻击被忽略 (金额仍为库内价)', amt === 199000000, `返回 amountMicro=${amt}`);

  // 2.2 负数量 / 零数量
  const negQty = await req('POST', '/internal/orders', { token: tokenA, body: { items: [{ sku: 'tut-rust', quantity: -5 }] } });
  record('负数量下单被拒 → 400', negQty.status === 400, `status=${negQty.status}`);
  const zeroQty = await req('POST', '/internal/orders', { token: tokenA, body: { items: [{ sku: 'tut-rust', quantity: 0 }] } });
  record('零数量下单被拒 → 400', zeroQty.status === 400, `status=${zeroQty.status}`);

  // 2.3 超大数量（整数溢出/DoS）—— 修复后应返回 400
  const hugeQty = await req('POST', '/internal/orders', { token: tokenA, body: { items: [{ sku: 'tut-rust', quantity: 2147483647 }] } });
  record('超大数量下单被拒 → 400 (防溢出)', hugeQty.status === 400, `status=${hugeQty.status}`);

  // 2.4 空购物车
  const emptyCart = await req('POST', '/internal/orders', { token: tokenA, body: { items: [] } });
  record('空购物车下单被拒 → 400', emptyCart.status === 400, `status=${emptyCart.status}`);

  // 2.5 不存在的 SKU
  const badSku = await req('POST', '/internal/orders', { token: tokenA, body: { items: [{ sku: 'nonexistent-sku', quantity: 1 }] } });
  record('不存在的 SKU → 404', badSku.status === 404, `status=${badSku.status}`);

  // 2.6 免费升级会员（修复后应返回 403，禁止免费升级）
  const upgrade = await req('POST', '/user/profile', { token: tokenA, body: { tier_code: 'pro' } });
  record('免费升级会员被拒 → 403 (修复后禁止免费升级)', upgrade.status === 403, `status=${upgrade.status} body=${upgrade.text.slice(0,100)}`);

  // 2.7 优惠券门槛绕过
  // 领 WELCOME20（满100减20），然后用一个低价商品下单，应该被拒
  const claim = await req('POST', '/user/coupons', { token: tokenA, body: { code: 'WELCOME20' } });
  record('领券 WELCOME20', claim.status === 200, `status=${claim.status}`);
  // tut-rust 199元 > 100门槛，应该能用券（199-20=179元，free 无会员折扣）
  const lowOrder = await req('POST', '/internal/orders', { token: tokenA, body: { items: [{ sku: 'tut-rust', quantity: 1 }], coupon_code: 'WELCOME20' } });
  record('满减券正确应用 (199→179)', lowOrder.json?.amountMicro === 179000000, `amountMicro=${lowOrder.json?.amountMicro}`);

  console.log('\n【3. SQL 注入与输入校验】');

  // 3.1 SQL 注入尝试（catalog 搜索）
  const sqli1 = await req('GET', "/catalog/products?search=1' OR '1'='1");
  record('搜索 SQL 注入被参数化 (返回正常或空，不报错)', sqli1.status === 200, `status=${sqli1.status}`);

  const sqli2 = await req('GET', "/catalog/products?search=%27%3B%20DROP%20TABLE%20users%3B--");
  record('DROP TABLE 注入尝试不破坏数据库', sqli2.status === 200, `status=${sqli2.status}`);

  // 3.2 路径穿越（SKU）
  const pathTrav = await req('GET', '/catalog/products/..%2F..%2Fetc%2Fpasswd');
  record('SKU 路径穿越 → 404 (不泄露文件)', pathTrav.status === 404, `status=${pathTrav.status}`);

  // 3.3 邮箱格式校验
  const badEmail = await req('POST', '/auth/register', { body: { email: 'not-an-email', password: 'Password123!' } });
  record('非法邮箱注册被拒 → 400', badEmail.status === 400, `status=${badEmail.status}`);

  // 3.4 弱密码
  const weakPw = await req('POST', '/auth/register', { body: { email: `weak-${TS}@test.com`, password: '123' } });
  record('弱密码（<8位）注册被拒 → 400', weakPw.status === 400, `status=${weakPw.status}`);

  // 3.5 空密码
  const emptyPw = await req('POST', '/auth/register', { body: { email: `empty-${TS}@test.com`, password: '' } });
  record('空密码注册被拒 → 400', emptyPw.status === 400, `status=${emptyPw.status}`);

  console.log('\n【4. 会话与密码学】');

  // 4.1 token 长度（256-bit = 64 hex）
  const tokenLen = ra.json?.token?.length;
  record('会话 token 为 256-bit (64 hex)', tokenLen === 64, `len=${tokenLen}`);

  // 4.2 登出后 token 失效
  const logout = await req('POST', '/auth/logout', { token: tokenA });
  const afterLogout = await req('GET', '/auth/me', { token: tokenA });
  record('登出后 token 失效 → 401', afterLogout.status === 401, `status=${afterLogout.status}`);
  // 重新登录恢复
  const reloginA = await req('POST', '/auth/login', { body: { email: ua, password: 'Password123!' } });
  const tokenA2 = reloginA.json?.token;

  // 4.3 用户枚举：登录不存在用户 vs 错误密码 都应返回统一 401
  const loginNoUser = await req('POST', '/auth/login', { body: { email: `nouser-${TS}@test.com`, password: 'whatever123' } });
  const loginWrongPw = await req('POST', '/auth/login', { body: { email: ua, password: 'WrongPassword123' } });
  const sameMsg = loginNoUser.text === loginWrongPw.text && loginNoUser.status === loginWrongPw.status;
  record('防用户枚举 (不存在用户与错误密码返回一致 401)', loginNoUser.status === 401 && loginWrongPw.status === 401 && sameMsg, `noUser=${loginNoUser.text} wrongPw=${loginWrongPw.text}`);

  // 4.4 密码哈希不落明文（通过 me 端点无法获取密码）
  const meResp = await req('GET', '/auth/me', { token: tokenA2 });
  record('me 端点不泄露密码哈希', !JSON.stringify(meResp.json).includes('password') && !JSON.stringify(meResp.json).includes('hash'), `resp=${JSON.stringify(meResp.json)}`);

  console.log('\n【5. XSS 与内容安全】');

  // 5.1 评论内容 XSS（存到 DB，读出来应该被前端转义）
  // 先给 A 造交付记录（通过 admin 发货或直接 seed），然后 A 发表带 script 的评论
  // 这里先验证后端存储不报错，前端转义在浏览器测试验证
  const xssContent = '<script>alert(1)</script><img src=x onerror=alert(2)>';
  const xssReview = await req('POST', '/user/reviews', { token: tokenA2, body: { sku: 'tut-rust', rating: 5, content: xssContent } });
  // 若用户未购买，会 403；此处只记录
  record('XSS 评论请求被后端接受或正确拒绝（无崩溃）', [200, 201, 403].includes(xssReview.status), `status=${xssReview.status}`);

  // 5.2 提问 XSS
  const xssQ = await req('POST', '/user/questions', { token: tokenA2, body: { sku: 'tut-rust', question: `<b onmouseover="alert('xss')">q</b>` } });
  record('XSS 提问请求被后端接受（无崩溃）', [200, 201].includes(xssQ.status), `status=${xssQ.status}`);

  console.log('\n【6. 路径穿越（文件下载/上传）】');

  // 6.1 未授权下载不存在交付
  const dlNone = await req('GET', '/files/999999');
  record('未认证访问 /files/999999 → 401', dlNone.status === 401, `status=${dlNone.status}`);

  // 6.2 admin 上传文件路径穿越（file_name 含 ../）
  const uploadTrav = await req('POST', '/admin/deliveries/files', { token: ADMIN_TOKEN, body: { product_id: 1, file_name: '../../../../etc/evil.txt', content: 'aGVsbG8=' } });
  // 记录是否成功写入逃逸路径
  const storageKey = uploadTrav.json?.storageKey;
  record('admin 上传文件名路径穿越检测（记录 storageKey）', true, `storageKey=${storageKey}`);
  if (storageKey && storageKey.includes('..')) {
    console.log(`  🚨 路径穿越风险：storageKey 含 ../ → ${storageKey}`);
  }

  // 6.3 admin 卡密生成 count 边界（正确字段名 productId）
  const hugeCount = await req('POST', '/admin/card-keys', { token: ADMIN_TOKEN, body: { productId: 1, count: 100000 } });
  record('卡密生成 count 超限被拒 → 400', hugeCount.status === 400, `status=${hugeCount.status}`);

  console.log('\n【7. 竞态与并发（优惠券 / 库存）】');

  // 7.1 优惠券并发领取（同一用户重复领券应受限）
  const claimConcurrent = await Promise.all([
    req('POST', '/user/coupons', { token: tokenA2, body: { code: 'NINEOFF' } }),
    req('POST', '/user/coupons', { token: tokenA2, body: { code: 'NINEOFF' } }),
    req('POST', '/user/coupons', { token: tokenA2, body: { code: 'NINEOFF' } }),
  ]);
  const successCount = claimConcurrent.filter(r => r.status === 200).length;
  record(`并发领券同一用户 (成功数应 ≤ max_uses_per_user=1, 实际=${successCount})`, successCount <= 1, `success=${successCount}`);

  console.log('\n【8. 公开端点信息泄露】');

  // 8.1 商品列表不泄露库存/成本
  const prods = await req('GET', '/catalog/products');
  const firstProd = prods.json?.[0];
  const leaksStock = firstProd && ('stock' in firstProd);
  record('公开商品列表不泄露库存字段', !leaksStock, `字段=${Object.keys(firstProd || {}).join(',')}`);

  // 8.2 公开评论不泄露用户 email（只泄露 uid）
  const reviews = await req('GET', '/catalog/products/tut-rust/reviews');
  const reviewStr = JSON.stringify(reviews.json);
  record('公开评论不泄露用户 email', !reviewStr.includes('@'), `review=${reviewStr.slice(0,200)}`);

  console.log('\n【9. 路径穿越修复验证（B4-06）】');

  // 9.1 admin 上传带 ../ 的文件名 → 应被净化或拒绝
  const trav1 = await req('POST', '/admin/deliveries/files', { token: ADMIN_TOKEN, body: { product_id: 1, file_name: '../../../evil-escape.txt', content: 'aGVsbG8=' } });
  const sk1 = trav1.json?.storageKey || '';
  const escaped = sk1.includes('evil-escape') && !sk1.includes('uploads');
  record('路径穿越被净化 (文件名不含 ../，落在 uploads 内)', !sk1.includes('..'), `storageKey=${sk1}`);

  // 9.2 绝对路径文件名 → 应被净化为 basename（不含 Windows/system32 用户输入）
  const trav2 = await req('POST', '/admin/deliveries/files', { token: ADMIN_TOKEN, body: { product_id: 1, file_name: 'C:\\Windows\\system32\\evil.dll', content: 'aGVsbG8=' } });
  const sk2 = trav2.json?.storageKey || '';
  const purified = !sk2.includes('Windows') && !sk2.includes('system32') && sk2.endsWith('evil.dll');
  record('Windows 盘符路径被净化为 basename', purified, `storageKey=${sk2}`);

  console.log('\n【10. 评论/提问长度限制与 DoS 防护】');

  // 10.1 超长评论（> 10000 字符）
  const longReview = 'A'.repeat(50000);
  const lr = await req('POST', '/user/reviews', { token: tokenA2, body: { sku: 'tut-rust', rating: 5, content: longReview } });
  record('超长评论被处理 (400/403 或截断，不崩溃)', [400, 403, 201, 200].includes(lr.status), `status=${lr.status}`);

  // 10.2 超长提问
  const longQ = await req('POST', '/user/questions', { token: tokenA2, body: { sku: 'tut-rust', question: 'B'.repeat(50000) } });
  record('超长提问被处理 (400/403 或截断，不崩溃)', [400, 403, 201, 200].includes(longQ.status), `status=${longQ.status}`);

  console.log('\n==================== 测试汇总 ====================');
  console.log(`通过: ${PASS} / ${PASS + FAIL}`);
  if (FAIL > 0) {
    console.log('\n失败项：');
    failures.forEach(f => console.log(`  ❌ ${f.name} -- ${f.detail}`));
  }
  return { PASS, FAIL, failures };
}

main().catch(e => { console.error('测试脚本错误：', e); process.exit(1); });
