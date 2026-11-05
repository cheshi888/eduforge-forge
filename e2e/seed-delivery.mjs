import pg from 'pg';

// 测试数据准备: 为 e2e 用户模拟"已购买已发货"状态 (插入 product_deliveries),
// 以便浏览器测试评论功能 (评论需已购校验). 这是标准测试 seed, 不绕过业务逻辑本身.
// 用法: node seed-delivery.mjs <email> <sku>
const email = process.argv[2];
const sku = process.argv[3] || 'tut-rust';
if (!email) { console.error('usage: node seed-delivery.mjs <email> <sku>'); process.exit(1); }

const client = new pg.Client({ host: '127.0.0.1', port: 5432, user: 'postgres', password: 'postgres', database: 'postgres' });
await client.connect();

const user = await client.query('SELECT id FROM users WHERE email = $1', [email]);
if (user.rows.length === 0) { console.error('user not found:', email); process.exit(1); }
const userId = user.rows[0].id;

const prod = await client.query('SELECT id, category FROM products WHERE sku = $1', [sku]);
if (prod.rows.length === 0) { console.error('product not found:', sku); process.exit(1); }
const productId = prod.rows[0].id;
const cat = prod.rows[0].category;
const deliveryType = cat === 'tutorial' ? 'content' : (cat === 'card' ? 'card_key' : 'file');

// 幂等: 已存在则跳过
const exists = await client.query('SELECT 1 FROM product_deliveries WHERE user_id=$1 AND product_id=$2', [userId, productId]);
if (exists.rows.length > 0) {
  console.log(`[seed] delivery 已存在: ${email} -> ${sku}`);
} else {
  const r = await client.query(
    'INSERT INTO product_deliveries (user_id, product_id, delivery_type, file_name) VALUES ($1,$2,$3,$4) RETURNING id',
    [userId, productId, deliveryType, `${sku}-delivery`]
  );
  console.log(`[seed] 插入 delivery id=${r.rows[0].id}: ${email} -> ${sku} (type=${deliveryType})`);
}

// 同时为测试用户领取优惠券 (WELCOME20 / NINEOFF)
for (const code of ['WELCOME20', 'NINEOFF']) {
  const c = await client.query('SELECT id FROM coupons WHERE code=$1', [code]);
  if (c.rows.length > 0) {
    const couponId = c.rows[0].id;
    const has = await client.query('SELECT 1 FROM user_coupons WHERE user_id=$1 AND coupon_id=$2 AND status=\'UNUSED\'', [userId, couponId]);
    if (has.rows.length === 0) {
      await client.query('INSERT INTO user_coupons (user_id, coupon_id, status) VALUES ($1,$2,\'UNUSED\')', [userId, couponId]);
      console.log(`[seed] 领取优惠券 ${code}`);
    }
  }
}

await client.end();
console.log('[seed] done');
