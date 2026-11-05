import { chromium } from 'playwright';
import pg from 'pg';

const BASE = 'http://localhost:3000';
const TS = Date.now();
const EMAIL = `e2e-${TS}@test.com`;
const PASSWORD = 'password123';

// 测试数据准备: 评论需"已购"状态, 注册后插入 product_deliveries 模拟已完成购买.
async function seedDelivery(email, sku = 'tut-rust') {
  const client = new pg.Client({ host: '127.0.0.1', port: 5432, user: 'postgres', password: 'postgres', database: 'postgres' });
  await client.connect();
  const u = await client.query('SELECT id FROM users WHERE email=$1', [email]);
  if (u.rows.length === 0) { await client.end(); return; }
  const userId = u.rows[0].id;
  const p = await client.query('SELECT id, category FROM products WHERE sku=$1', [sku]);
  if (p.rows.length === 0) { await client.end(); return; }
  const productId = p.rows[0].id;
  const cat = p.rows[0].category;
  const dt = cat === 'tutorial' ? 'content' : (cat === 'card' ? 'card_key' : 'file');
  const exists = await client.query('SELECT 1 FROM product_deliveries WHERE user_id=$1 AND product_id=$2', [userId, productId]);
  if (exists.rows.length === 0) {
    await client.query('INSERT INTO product_deliveries (user_id, product_id, delivery_type, file_name) VALUES ($1,$2,$3,$4)', [userId, productId, dt, `${sku}-delivery`]);
  }
  await client.end();
}

let pass = 0;
let fail = 0;
const failures = [];

function ok(label) { pass++; console.log(`  ✅ ${label}`); }
function no(label, extra = '') { fail++; failures.push(`${label}${extra ? ' — ' + extra : ''}`); console.log(`  ❌ ${label}${extra ? ' — ' + extra : ''}`); }
function assert(cond, label, extra = '') { cond ? ok(label) : no(label, extra); }

async function attach(page, name) {
  page.on('console', (m) => {
    if (m.type() !== 'error') return;
    const t = m.text();
    if (/401|favicon|Failed to load resource.*40[13]|_rsc=/.test(t)) return;
    console.log(`  [console:${name}] ${t.slice(0, 160)}`);
  });
  page.on('pageerror', (e) => console.log(`  [pageerror:${name}] ${e.message.split('\n')[0]}`));
}

const browser = await chromium.launch({ channel: 'chrome', headless: true });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 } });
const page = await ctx.newPage();
await attach(page, 'main');

// ===== 0. 首页 =====
console.log('\n=== 0. 首页 / 商品目录 ===');
await page.goto(`${BASE}/zh`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(1500);
let bodyText = await page.evaluate(() => document.body.innerText);
assert(bodyText.includes('商品橱窗'), '首页显示「商品橱窗」');
const cardCount = await page.locator('a[href*="/checkout"]').count();
assert(cardCount > 0, `商品卡片已渲染 (${cardCount} 个)`);

// 分类筛选
await page.getByRole('button', { name: '源码' }).click();
await page.waitForTimeout(1500);
const srcCards = await page.locator('a[href*="/checkout"]').count();
assert(srcCards > 0 && srcCards <= 8, `点击「源码」分类筛选生效 (${srcCards} 个)`);

// ===== 1. 注册 =====
console.log('\n=== 1. 注册 ===');
await page.goto(`${BASE}/zh/auth`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(1000);
await page.getByRole('button', { name: '注册' }).click();
await page.waitForTimeout(400);
await page.fill('input[name="email"]', EMAIL);
await page.fill('input[name="password"]', PASSWORD);
await page.getByRole('button', { name: '创建账号' }).click();
await page.waitForTimeout(2500);
bodyText = await page.evaluate(() => document.body.innerText);
assert(bodyText.includes('注册成功'), '注册后显示「注册成功」');
await seedDelivery(EMAIL);
console.log('  ↳ 已为测试用户准备 delivery (模拟已购买)');

// ===== 2. 登录 =====
console.log('\n=== 2. 登录 ===');
await page.goto(`${BASE}/zh/auth`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(1000);
await page.fill('input[name="email"]', EMAIL);
await page.fill('input[name="password"]', PASSWORD);
await page.getByRole('button', { name: '登录' }).last().click();
await page.waitForTimeout(2500);
bodyText = await page.evaluate(() => document.body.innerText);
assert(bodyText.includes('登录成功'), '登录后显示「登录成功」');

// ===== 3. 登录态导航 =====
console.log('\n=== 3. 登录态导航 ===');
await page.goto(`${BASE}/zh`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(1500);
bodyText = await page.evaluate(() => document.body.innerText);
assert(bodyText.includes(EMAIL), '登录后导航显示用户邮箱');
assert(bodyText.includes('通知'), '登录后导航显示「通知」入口');

// ===== 4. 商品详情页 =====
console.log('\n=== 4. 商品详情页 ===');
await page.goto(`${BASE}/zh/product/tut-rust`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(2000);
bodyText = await page.evaluate(() => document.body.innerText);
assert(bodyText.includes('Rust'), '详情页显示商品名');
assert(bodyText.includes('用户评价'), '详情页显示「用户评价」');
assert(bodyText.includes('问答'), '详情页显示「问答」');

// ===== 5. 提问（无需已购）=====
console.log('\n=== 5. 提问 ===');
const qInput = page.locator('input[placeholder*="疑问"], input[placeholder*="question"], input[placeholder*="質問"], input[placeholder*="궁금"]').first();
if (await qInput.count() > 0) {
  await qInput.fill(`测试提问 ${TS}`);
  await page.getByRole('button', { name: '提问' }).click();
  await page.waitForTimeout(2000);
  bodyText = await page.evaluate(() => document.body.innerText);
  assert(bodyText.includes(`测试提问 ${TS}`), '提问提交成功并显示');
} else {
  no('找到提问输入框');
}

// ===== 5.5 提交评论（已购用户）=====
console.log('\n=== 5.5 提交评论 ===');
const reviewInput = page.locator('input[placeholder*="体验"], input[placeholder*="experience"], input[placeholder*="体験"], input[placeholder*="후기"]').first();
if (await reviewInput.count() > 0) {
  await reviewInput.fill(`自动化测试评论 ${TS}`);
  await page.getByRole('button', { name: '发表' }).click();
  await page.waitForTimeout(2000);
  bodyText = await page.evaluate(() => document.body.innerText);
  assert(bodyText.includes(`自动化测试评论 ${TS}`), '评论提交成功并显示');
} else {
  no('找到评论输入框');
}

// ===== 6. 收藏 =====
console.log('\n=== 6. 收藏 ===');
const favBtn = page.getByRole('button', { name: /收藏|favorite/i }).first();
if (await favBtn.count() > 0) {
  await favBtn.click();
  await page.waitForTimeout(1500);
  bodyText = await page.evaluate(() => document.body.innerText);
  assert(bodyText.includes('已收藏'), '收藏成功（按钮态变化）');
} else {
  no('找到收藏按钮');
}

// ===== 7. 会员中心 =====
console.log('\n=== 7. 会员中心 ===');
await page.goto(`${BASE}/zh/membership`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(2000);
bodyText = await page.evaluate(() => document.body.innerText);
assert(bodyText.includes('Free') || bodyText.includes('Plus') || bodyText.includes('Pro') || bodyText.includes('免费'), '会员中心显示等级卡片');

// ===== 8. 结算页（无折扣 free）=====
console.log('\n=== 8. 结算页 ===');
await page.goto(`${BASE}/zh/checkout?item=tut-rust&qty=1`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(2500);
bodyText = await page.evaluate(() => document.body.innerText);
assert(/199\.00/.test(bodyText), '结算页显示商品金额 ¥199.00');
const payMethods = await page.locator('button').filter({ hasText: /支付宝|微信|Stripe|卡|PayPal|USDT/i }).count();
assert(payMethods >= 6, `支付方式按钮存在 (${payMethods})`);

// ===== 9. 优惠券结算（带 coupon 参数）=====
console.log('\n=== 9. 优惠券结算 ===');
// 先通过后端 API 领券（模拟用户已领券）
await page.evaluate(async (email) => {
  const r = await fetch('/api/auth/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email, password: 'password123' }) });
  const d = await r.json();
  if (d.token) {
    await fetch('/api-user/user/coupons', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ code: 'WELCOME20' }) });
  }
}, EMAIL);
await page.waitForTimeout(1000);
await page.goto(`${BASE}/zh/checkout?item=tut-rust&qty=1&coupon=WELCOME20`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(2500);
bodyText = await page.evaluate(() => document.body.innerText);
assert(bodyText.includes('WELCOME20'), '结算页显示优惠券码 WELCOME20', bodyText.slice(0, 300));

// ===== 10. 通知中心 =====
console.log('\n=== 10. 通知中心 ===');
await page.goto(`${BASE}/zh`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(1500);
const notifBtn = page.getByRole('button', { name: /通知|Notifications|알림/i }).first();
if (await notifBtn.count() > 0) {
  await notifBtn.click();
  await page.waitForTimeout(1500);
  bodyText = await page.evaluate(() => document.body.innerText);
  assert(bodyText.includes('通知') || bodyText.includes('暂无通知'), '通知中心下拉面板打开');
} else {
  no('找到通知按钮');
}

// ===== 11. 我的文件 =====
console.log('\n=== 11. 我的文件 ===');
await page.goto(`${BASE}/zh/my-files`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(2000);
bodyText = await page.evaluate(() => document.body.innerText);
assert(bodyText.length > 50, '我的文件页正常渲染');

// ===== 12. 多语言 =====
console.log('\n=== 12. 多语言 ===');
for (const [code, kw] of [['en', 'Storefront'], ['ja', '商品カタログ'], ['ko', '상품 쇼윈도']]) {
  await page.goto(`${BASE}/${code}`, { waitUntil: 'domcontentloaded', timeout: 30000 });
  await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
  await page.waitForTimeout(1500);
  bodyText = await page.evaluate(() => document.body.innerText);
  assert(bodyText.includes(kw), `语言 ${code} 首页正确显示`);
}

// ===== 13. 未登录拦截 =====
console.log('\n=== 13. 未登录拦截 ===');
const ctx2 = await browser.newContext({ viewport: { width: 1280, height: 900 } });
const page2 = await ctx2.newPage();
await page2.goto(`${BASE}/zh/checkout?item=tut-rust`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page2.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page2.waitForTimeout(1500);
const anonText = await page2.evaluate(() => document.body.innerText);
assert(/sign in|登录|Sign in|Please sign/.test(anonText), '未登录访问结算页被拦截');
await ctx2.close();

// ===== 14. 移动端视口 =====
console.log('\n=== 14. 移动端视口 ===');
const page3 = await ctx.newPage();
await page3.setViewportSize({ width: 375, height: 812 });
const overflow = [];
for (const r of ['/zh', '/zh/membership', '/zh/auth', '/zh/product/tut-rust']) {
  await page3.goto(`${BASE}${r}`, { waitUntil: 'domcontentloaded', timeout: 30000 });
  await page3.waitForLoadState('networkidle', { timeout: 20000 }).catch(() => {});
  await page3.waitForTimeout(1000);
  const m = await page3.evaluate(() => ({
    scrollW: document.documentElement.scrollWidth,
    clientW: document.documentElement.clientWidth
  }));
  if (m.scrollW - m.clientW > 1) overflow.push(`${r}(${m.scrollW}>${m.clientW})`);
}
assert(overflow.length === 0, '移动端无横向溢出', overflow.join(', '));
await page3.close();

// ===== 汇总 =====
console.log('\n===== 结果汇总 =====');
console.log(`✅ PASS: ${pass}`);
console.log(`❌ FAIL: ${fail}`);
if (failures.length) {
  console.log('\n失败项：');
  failures.forEach((f) => console.log('  - ' + f));
}

await browser.close();
process.exit(fail === 0 ? 0 : 1);
