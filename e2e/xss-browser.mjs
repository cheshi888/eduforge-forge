// 浏览器 XSS 转义验证：注入恶意评论/提问，验证 React 是否正确转义（不执行脚本）
import { chromium } from 'playwright';

const BASE = 'http://localhost:3000';
const BACKEND = 'http://127.0.0.1:8080';
const TS = Date.now();
const EMAIL = `xss-${TS}@test.com`;
const PASSWORD = 'Password123!';

let PASS = 0, FAIL = 0;
const failures = [];

function record(name, ok, detail = '') {
  if (ok) { PASS++; console.log(`  ✅ ${name}`); }
  else { FAIL++; failures.push({ name, detail }); console.log(`  ❌ ${name}  -- ${detail}`); }
}

async function main() {
  console.log('\n==================== 前端 XSS 转义验证 ====================\n');

  // 1. 注册用户并通过后端插入 XSS 评论/提问
  const reg = await fetch(`${BACKEND}/auth/register`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email: EMAIL, password: PASSWORD })
  });
  const { token } = await reg.json();

  // 造一条 XSS 提问（提问无需已购）
  const xssQ = `<img src=x onerror="window.__xss_q=1">恶意提问<script>alert('q')</script>`;
  await fetch(`${BACKEND}/user/questions`, {
    method: 'POST', headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
    body: JSON.stringify({ sku: 'tut-rust', question: xssQ })
  });

  // 造一条 XSS 评论（需要已购，先 seed delivery）
  // 通过 pg 直接插入 delivery（复用 e2e 的 pg 连接方式）
  const { Client } = await import('pg');
  const client = new Client({ connectionString: 'postgres://postgres:postgres@127.0.0.1:5432/postgres' });
  await client.connect();
  const uidRes = await client.query('SELECT id FROM users WHERE email = $1', [EMAIL]);
  const userId = uidRes.rows[0].id;
  const prodRes = await client.query('SELECT id FROM products WHERE sku = $1', ['tut-rust']);
  const prodId = prodRes.rows[0].id;
  // 插入交付记录（模拟已购）
  await client.query(
    'INSERT INTO product_deliveries (user_id, product_id, delivery_type) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING',
    [userId, prodId, 'content']
  );
  await client.end();

  const xssR = `<img src=x onerror="window.__xss_r=1">恶意评论<script>alert('r')</script>`;
  await fetch(`${BACKEND}/user/reviews`, {
    method: 'POST', headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
    body: JSON.stringify({ sku: 'tut-rust', rating: 5, content: xssR })
  });

  // 2. 浏览器加载商品详情页，检查 XSS 是否被转义
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage();

  // 收集弹窗和全局变量
  let alertCount = 0;
  page.on('dialog', async (dialog) => { alertCount++; await dialog.dismiss(); });

  await page.goto(`${BASE}/zh/product/tut-rust`, { waitUntil: 'domcontentloaded', timeout: 30000 });
  await page.waitForTimeout(3000);

  // 检查是否执行了 XSS（onerror 设置了全局变量）
  const executed = await page.evaluate(() => ({
    q: (window).__xss_q || 0,
    r: (window).__xss_r || 0,
  }));

  record('XSS 提问脚本未执行 (onerror 未触发)', executed.q === 0, `__xss_q=${executed.q}`);
  record('XSS 评论脚本未执行 (onerror 未触发)', executed.r === 0, `__xss_r=${executed.r}`);
  record('无弹窗 (alert 未被触发)', alertCount === 0, `alertCount=${alertCount}`);

  // 3. 检查页面是否真的渲染了这些内容（说明是转义显示而非执行）
  const bodyText = await page.evaluate(() => document.body.innerText);
  const hasEscaped = bodyText.includes('恶意提问') || bodyText.includes('恶意评论');
  record('XSS 内容以文本形式渲染 (被转义)', hasEscaped, `body含恶意文本=${hasEscaped}`);

  // 4. 检查 DOM 中是否有未转义的 img/script 标签
  const dangerousElements = await page.evaluate(() => {
    const imgs = Array.from(document.querySelectorAll('img[onerror]'));
    const scripts = Array.from(document.querySelectorAll('script')).filter(s => s.textContent.includes('alert'));
    return { imgOnError: imgs.length, inlineScripts: scripts.length };
  });
  record('DOM 无注入的 onerror img', dangerousElements.imgOnError === 0, `img[onerror]=${dangerousElements.imgOnError}`);
  record('DOM 无注入的内联 script', dangerousElements.inlineScripts === 0, `inlineScripts=${dangerousElements.inlineScripts}`);

  await browser.close();

  console.log('\n==================== 汇总 ====================');
  console.log(`通过: ${PASS} / ${PASS + FAIL}`);
  if (FAIL > 0) {
    failures.forEach(f => console.log(`  ❌ ${f.name} -- ${f.detail}`));
  }
}

main().catch(e => { console.error('脚本错误：', e); process.exit(1); });
