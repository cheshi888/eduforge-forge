-- 006_promotion: 优惠券营销体系 — 券模板 + 用户领券 + 使用记录
-- 前置: 001~005 schema 已应用.
-- 金额一律 NUMERIC(20,6), 折扣率 SMALLINT 百分比整数 (禁浮点).

-- 优惠券模板 (运营侧配置)
CREATE TABLE coupons (
    id              BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code            VARCHAR(64)  NOT NULL UNIQUE,       -- 券码 (对外可见, 用于领用/核销)
    title           VARCHAR(128) NOT NULL,              -- 展示名 (如 "新客立减 20")
    coupon_type     VARCHAR(16)  NOT NULL
        CHECK (coupon_type IN ('fixed','percent')),      -- fixed=满减 / percent=折扣
    discount_value  NUMERIC(20,6) NOT NULL,              -- fixed: 减免金额(元); percent: 折扣率(1~99)
    threshold_micro NUMERIC(20,6) NOT NULL DEFAULT 0,    -- 使用门槛: 订单折后金额 ≥ 此值才可用
    max_uses_total  INT          NOT NULL DEFAULT 0,     -- 全平台发放总量 (0=不限)
    max_uses_per_user INT        NOT NULL DEFAULT 1,     -- 每用户可用次数
    valid_from      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    valid_until     TIMESTAMPTZ,                         -- 过期时间 (NULL=不过期)
    active          BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    CONSTRAINT ck_percent_range CHECK (coupon_type != 'percent' OR (discount_value >= 1 AND discount_value <= 99))
);

-- 用户领券记录
CREATE TABLE user_coupons (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    coupon_id    BIGINT NOT NULL REFERENCES coupons(id),
    status       VARCHAR(12) NOT NULL DEFAULT 'UNUSED'
        CHECK (status IN ('UNUSED','LOCKED','USED','EXPIRED')),
    used_at      TIMESTAMPTZ,
    order_id     BIGINT REFERENCES orders(id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uk_user_coupon UNIQUE (user_id, coupon_id, id)  -- 允许同券多次领取, 由 max_uses_per_user 限次
);
CREATE INDEX idx_user_coupons_user ON user_coupons(user_id, status);

-- 领券活动 (可选: 公开可领的券池)
CREATE TABLE coupon_campaigns (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    coupon_id    BIGINT NOT NULL REFERENCES coupons(id) ON DELETE CASCADE,
    campaign_name VARCHAR(128) NOT NULL,
    active       BOOLEAN NOT NULL DEFAULT TRUE,
    started_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at     TIMESTAMPTZ
);

-- 演示券: 新客满 100 减 20 + 全店 9 折券
INSERT INTO coupons (code, title, coupon_type, discount_value, threshold_micro, max_uses_total, max_uses_per_user, valid_until) VALUES
    ('WELCOME20', '新客满 100 减 20', 'fixed', 20, 100, 10000, 1, now() + interval '365 days'),
    ('NINEOFF', '全店 9 折券', 'percent', 10, 0, 10000, 1, now() + interval '365 days')
ON CONFLICT (code) DO NOTHING;
