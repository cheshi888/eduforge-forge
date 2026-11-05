-- 005_catalog_cart: 购物车多商品下单明细 + 商品目录扩展字段
-- 前置: 001_init.sql / 002_admin.sql / 003_membership.sql 已应用.

-- 订单明细表: 支持多商品购物车合并下单.
-- 订单主表 orders.quantity 存合计数量, 明细按 SKU 拆行记录数量与下单时单价快照.
CREATE TABLE order_items (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    order_id     BIGINT NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    product_id   BIGINT NOT NULL REFERENCES products(id),
    quantity     INT    NOT NULL CHECK (quantity > 0),
    unit_price   NUMERIC(20,6) NOT NULL CHECK (unit_price > 0),  -- 下单时单价快照 (防改价追溯)
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_order_items_order ON order_items(order_id);
CREATE INDEX idx_order_items_product ON order_items(product_id);

-- 商品目录补充字段: 评分/销量/徽标/封面色, 供商品橱窗展示 (替代前端硬编码).
ALTER TABLE products ADD COLUMN rating     NUMERIC(3,2) NOT NULL DEFAULT 0;
ALTER TABLE products ADD COLUMN sold_count BIGINT       NOT NULL DEFAULT 0;
ALTER TABLE products ADD COLUMN badge      VARCHAR(32);
