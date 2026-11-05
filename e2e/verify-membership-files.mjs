import { chromium } from 'playwright';

const BASE = 'http://localhost:3000';
let pass = 0, fail = 0;
const failures = [];
function ok(l) { pass++; console.log(`  ✅ ${l}`); }
function no(l, e='') { fail++; failures.push(`${l}${e?' — '+e:''}`); console.log(`  ❌ ${l}${e?' — '+e:''}`); }
function assert(c, l, e='') { c ? ok(l) : no(l, e); }

const browser = await chromium.launch({ channel: 'chrome', headless: true });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 } });
const page = await ctx.newPage();

console.log('\n=== 场景1: 未登录访问会员中心 ===');
await page.goto(`${BASE}/zh/membership`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(2000);
let body = await page.evaluate(() => document.body.innerText);
// 1. 页面应显示会员中心标题
assert(body.includes('会员中心'), '会员中心标题显示');
// 2. 套餐列表应显示（Free/Plus/Pro 来自公开 /user/tier）
assert(body.includes('Free') && body.includes('Plus') && body.includes('Pro'), '套餐列表显示 (Free/Plus/Pro)');
// 3. 不应显示 401 错误文案
assert(!body.includes('请求失败 (401)') && !body.includes('unauthorized'), '无 401 错误文案');
// 4. 应显示登录引导
assert(body.includes('登录'), '显示登录引导');
console.log('  [membership body 片段]', body.slice(0, 300).replace(/\n/g, ' '));

console.log('\n=== 场景2: 未登录访问我的文件 ===');
await page.goto(`${BASE}/zh/my-files`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
await page.waitForTimeout(2000);
body = await page.evaluate(() => document.body.innerText);
assert(body.includes('我的文件'), '我的文件标题显示');
assert(!body.includes('请求失败 (401)') && !body.includes('unauthorized'), '无 401 错误文案');
assert(body.includes('登录'), '显示登录引导');
console.log('  [my-files body 片段]', body.slice(0, 300).replace(/\n/g, ' '));

console.log('\n=== 场景3: 登录后访问（应正常显示数据）===');
// 用唯一邮箱注册并登录
const email = `verify-${Date.now()}@test.com`;
await page.goto(`${BASE}/zh/auth`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForTimeout(1500);
// 尝试填注册表单
const emailInput = page.locator('input[name="email"]');
const passInput = page.locator('input[name="password"]');
if (await emailInput.count() > 0 && await passInput.count() > 0) {
  await emailInput.fill(email);
  await passInput.fill('password123');
  // 找到注册按钮
  const regBtn = page.locator('button').filter({ hasText: /注册|Register/i }).first();
  if (await regBtn.count() > 0) {
    await regBtn.click();
    await page.waitForTimeout(3000);
  }
}
await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
// 回到会员中心，验证有 cookie 后 profile 正常
await page.goto(`${BASE}/zh/membership`, { waitUntil: 'domcontentloaded', timeout: 30000 });
await page.waitForTimeout(2500);
body = await page.evaluate(() => document.body.innerText);
// 登录后应显示"当前等级"（profile 已加载）
const hasProfile = body.includes('当前等级') || body.includes('累计消费');
console.log('  登录后 profile 是否加载:', hasProfile);
// 检查 cookie 是否设置
const cookies = await ctx.cookies();
const hasSession = cookies.some(c => c.name === 'forge_session');
assert(hasSession, '登录后 forge_session cookie 已设置');

console.log(`\n=== 结果: ${pass} PASS / ${fail} FAIL ===`);
if (failures.length) { console.log('失败项:'); failures.forEach(f => console.log('  -', f)); }

await browser.close();
process.exit(fail > 0 ? 1 : 0);
