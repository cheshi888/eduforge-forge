-- 003_membership: 会员等级制度 + 内容交付 + 上传文件管理
-- 前置: 001_init.sql / 002_admin.sql 已应用.

-- ============================================================
-- 会员等级: free / plus / pro (免费 / 付费会员 / 高级会员)
-- ============================================================
CREATE TABLE membership_tiers (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code         VARCHAR(16)  NOT NULL UNIQUE
        CHECK (code IN ('free','plus','pro')),
    name         VARCHAR(32)  NOT NULL,
    price_micro  NUMERIC(20,6) NOT NULL CHECK (price_micro >= 0),  -- 0 = free
    currency     VARCHAR(8)   NOT NULL DEFAULT 'CNY',
    duration_days INT         NOT NULL DEFAULT 365,
    discount_pct  SMALLINT    NOT NULL DEFAULT 0
        CHECK (discount_pct BETWEEN 0 AND 100),                    -- 会员购买非会员专享商品的折扣
    perk_json   JSONB        NOT NULL DEFAULT '{}'::jsonb,        -- 权益列表 [{key,label}]
    max_downloads SMALLINT    NOT NULL DEFAULT 3,
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT now()
);

INSERT INTO membership_tiers (code,name,price_micro,duration_days,discount_pct,max_downloads,perk_json) VALUES
    ('free','Free',0,0,0,1,'["browse_store","download_1_per_item"]'),
    ('plus','Plus',99000000,365,10,5,'["all_downloads_5","discount_10pct","priority_support"]'),
    ('pro','Pro',299000000,365,20,99,'["unlimited_downloads","discount_20pct","exclusive_content","community_access"]');

-- ============================================================
-- 用户会员档案: 当前等级 + 到期时间 + 累计消费 (驱动等级晋升)
-- ============================================================
ALTER TABLE users ADD COLUMN tier_code      VARCHAR(16) NOT NULL DEFAULT 'free'
    REFERENCES membership_tiers(code);
ALTER TABLE users ADD COLUMN tier_expires_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN total_spent_micro NUMERIC(20,6) NOT NULL DEFAULT 0;

CREATE INDEX idx_users_tier ON users(tier_code, tier_expires_at);

-- ============================================================
-- 已购内容交付: 用户购买某产品后获得的可下载交付物
-- 发货接线 run_paid: 订单 SHIPPED 时按 product.category 落库
-- ============================================================
CREATE TABLE product_deliveries (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id       BIGINT NOT NULL REFERENCES users(id),
    product_id    BIGINT NOT NULL REFERENCES products(id),
    order_id      BIGINT REFERENCES orders(id),
    delivery_type VARCHAR(16) NOT NULL
        CHECK (delivery_type IN ('file','card_key','content','license')),
    file_name     VARCHAR(256),
    file_url      TEXT,                -- 对象存储 key / 预签名 URL
    card_key_id   BIGINT REFERENCES card_keys(id),
    meta_json     JSONB NOT NULL DEFAULT '{}'::jsonb,
    downloaded_at TIMESTAMPTZ,
    download_count INT NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_deliveries_user ON product_deliveries(user_id, created_at DESC);
CREATE INDEX idx_deliveries_order ON product_deliveries(order_id);

-- ============================================================
-- 上传文件库: admin 上传的交付文件, 绑定到某产品
-- 发货时按产品分配给对应 delivery_type
-- ============================================================
CREATE TABLE uploaded_files (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    product_id   BIGINT NOT NULL REFERENCES products(id),
    file_name    VARCHAR(256) NOT NULL,
    storage_key  TEXT NOT NULL,          -- 对象存储 key
    size_bytes   BIGINT NOT NULL DEFAULT 0,
    mime_type    VARCHAR(128),
    checksum_sha256 CHAR(64),
    active       BOOLEAN NOT NULL DEFAULT TRUE,
    uploaded_by  VARCHAR(32),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_uploaded_files_product ON uploaded_files(product_id, active);
