-- EduForge-Neo 核心 Schema
-- 金额统一 NUMERIC(20,6): 内部 6 位精度, 展示层四舍五入到 2 位

CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE users (
    id                   BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uid                  VARCHAR(32)  NOT NULL UNIQUE,
    email                VARCHAR(255) NOT NULL,
    preferred_language   VARCHAR(5)   NOT NULL DEFAULT 'zh'
        CHECK (preferred_language IN ('zh','en','ja','ko')),
    created_at           TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE TABLE products (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sku          VARCHAR(64) NOT NULL UNIQUE,
    title        TEXT      NOT NULL,
    unit_price   NUMERIC(20,6) NOT NULL CHECK (unit_price > 0),
    currency     CHAR(3)   NOT NULL DEFAULT 'CNY',
    stock        BIGINT    NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE orders (
    id                         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    out_trade_no               VARCHAR(64)  NOT NULL UNIQUE,      -- 传给网关的商户单号
    user_id                    BIGINT       NOT NULL REFERENCES users(id),
    product_id                 BIGINT       NOT NULL REFERENCES products(id),
    quantity                     INT          NOT NULL CHECK (quantity > 0),
    expected_amount            NUMERIC(20,6) NOT NULL CHECK (expected_amount > 0),
    currency                   CHAR(3)      NOT NULL DEFAULT 'CNY',
    paid_amount                NUMERIC(20,6),
    status                     VARCHAR(24)  NOT NULL DEFAULT 'PENDING'
        CHECK (status IN ('PENDING','PAID','PENDING_BALANCE','SHIPPED','FRAUD_SUSPENDED','REFUNDED','CANCELLED')),
    -- Crypto 字段: 未达确认数时金额记入 PENDING_BALANCE, 确认达标后才发货
    crypto_network             VARCHAR(12),                       -- TRC-20 / ERC-20
    blockchain_confirmations   INT          NOT NULL DEFAULT 0,
    locked_exchange_rate       NUMERIC(20,6),                     -- 下单时刻美元锚定汇率, 防汇率波动少付
    gateway_type               VARCHAR(24),                       -- 下单时锁定的支付通道
    state_version              INT          NOT NULL DEFAULT 0,   -- 乐观并发版本号
    created_at                 TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at                 TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX idx_orders_status      ON orders(status);
CREATE INDEX idx_orders_user        ON orders(user_id, created_at);
CREATE INDEX idx_orders_blockchain  ON orders(status, crypto_network) WHERE status = 'PENDING_BALANCE';

CREATE TABLE payment_transactions (
    id                 BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    gateway_txn_id     VARCHAR(128) NOT NULL,      -- 网关侧流水号
    gateway_type       VARCHAR(24)  NOT NULL,      -- alipay / wechat / stripe / paypal / coinbase
    out_trade_no       VARCHAR(64)  NOT NULL,
    amount             NUMERIC(20,6) NOT NULL,
    currency           CHAR(3)      NOT NULL,
    confirmations      INT,                        -- Crypto 链上确认数
    status             VARCHAR(24)  NOT NULL DEFAULT 'RECEIVED'
        CHECK (status IN ('RECEIVED','MISMATCH','CURRENCY_MISMATCH','DUPLICATE','SETTLED','REJECTED')),
    raw_payload        JSONB        NOT NULL,
    received_at        TIMESTAMPTZ  NOT NULL DEFAULT now()
    -- !!! CRITICAL !!! 联合唯一索引: 防重放攻击的终极防线
    ,CONSTRAINT uk_txn UNIQUE (gateway_txn_id, gateway_type)
);

CREATE INDEX idx_txn_out_trade_no ON payment_transactions(out_trade_no);

CREATE TABLE plugin_registry (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name         VARCHAR(64) NOT NULL,             -- promotion / catalog / community
    version      VARCHAR(32) NOT NULL,
    artifact     TEXT        NOT NULL,             -- 二进制哈希或对象存储 key
    active       BOOLEAN     NOT NULL DEFAULT FALSE,
    checksum_sha256 CHAR(64) NOT NULL,
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uk_plugin_active UNIQUE (name, active)
);

-- 未来课程推荐: pgvector 向量索引
CREATE TABLE course_embeddings (
    course_id  BIGINT NOT NULL REFERENCES products(id),
    embedding  VECTOR(1536),
    PRIMARY KEY (course_id)
);

-- M2-02: 事务级事件 outbox — NATS 发布失败不再静默丢失 fraud/paid/confirm 事件.
-- 事件在 webhook 的同一 commit 事务内落库 (与订单状态变更原子),
-- 恢复后由 flusher 扫描 pending 行补发 JetStream, 成功后标 sent.
CREATE TABLE event_outbox (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    subject     VARCHAR(128) NOT NULL,
    payload     TEXT         NOT NULL,
    status      VARCHAR(16)  NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','sent')),
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    sent_at     TIMESTAMPTZ
);

CREATE INDEX idx_outbox_pending ON event_outbox(status, created_at) WHERE status = 'pending';