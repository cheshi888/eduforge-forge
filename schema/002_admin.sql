-- 002_admin: 管理后台扩展 — 卡密库 / 风控事件持久化 / 商品分类
-- 前置: 001_init.sql 已应用. 本文件只加新表与 product_category 列, 不动既有约束.

-- 商品分类 (知识付费: source / tutorial / card / resource)
ALTER TABLE products ADD COLUMN category VARCHAR(16) NOT NULL DEFAULT 'resource'
    CHECK (category IN ('source','tutorial','card','resource'));
ALTER TABLE products ADD COLUMN cover      VARCHAR(64);
ALTER TABLE products ADD COLUMN description TEXT;
ALTER TABLE products ADD COLUMN active     BOOLEAN NOT NULL DEFAULT TRUE;

-- 卡密库: 发货用凭证池. 结算成功后从池分配, 标记 used + 关联订单.
CREATE TABLE card_keys (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    product_id   BIGINT NOT NULL REFERENCES products(id),
    key_code     VARCHAR(256) NOT NULL,            -- 卡密本体 (展示用, 哈希另存)
    key_hash     VARCHAR(128) NOT NULL,            -- SHA-256, 防明文外泄
    status       VARCHAR(12) NOT NULL DEFAULT 'AVAILABLE'
        CHECK (status IN ('AVAILABLE','LOCKED','USED','REVERSED')),
    order_id     BIGINT REFERENCES orders(id),
    allocated_at TIMESTAMPTZ,
    used_at      TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uk_card_key_hash UNIQUE (key_hash, product_id)
);
CREATE INDEX idx_card_keys_product ON card_keys(product_id, status);
CREATE INDEX idx_card_keys_order   ON card_keys(order_id);

-- 风控事件持久化: fraud/poll 告警落库, 运营侧可查历史明细
CREATE TABLE fraud_events (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    out_trade_no VARCHAR(64)  NOT NULL,
    order_id     BIGINT       REFERENCES orders(id),
    reason       VARCHAR(32)  NOT NULL
        CHECK (reason IN ('currency_mismatch','amount_mismatch','orphan_callback','confirmation_insufficient','other')),
    received_amount NUMERIC(20,6),
    expected_amount NUMERIC(20,6),
    currency     VARCHAR(8),
    gateway_type VARCHAR(24),
    evidence     JSONB        NOT NULL DEFAULT '{}'::jsonb,
    resolved     BOOLEAN      NOT NULL DEFAULT FALSE,
    resolved_by  VARCHAR(32),
    resolved_at  TIMESTAMPTZ,
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT now()
);
CREATE INDEX idx_fraud_events_status ON fraud_events(created_at DESC);
CREATE INDEX idx_fraud_events_unresolved ON fraud_events(resolved, created_at DESC) WHERE resolved = FALSE;
