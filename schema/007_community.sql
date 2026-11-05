-- 007_community: 社区功能 — 评论 / 评分 / 问答 / 收藏 + 站内通知
-- 前置: 001~006 schema 已应用.

-- 商品评论 (含评分): 仅已购用户可评 (由应用层校验交付归属), 评分 1~5.
CREATE TABLE product_reviews (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    product_id  BIGINT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    user_id     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rating      SMALLINT NOT NULL CHECK (rating BETWEEN 1 AND 5),
    content     TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_reviews_product ON product_reviews(product_id, created_at DESC);
CREATE INDEX idx_reviews_user ON product_reviews(user_id);

-- 问答 (Q&A): 用户提问 + 回答 (可回复楼层)
CREATE TABLE product_questions (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    product_id  BIGINT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    user_id     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    question    TEXT NOT NULL,
    answer      TEXT,                 -- 商家/作者回复
    answered_by BIGINT REFERENCES users(id),
    answered_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_questions_product ON product_questions(product_id, created_at DESC);

-- 收藏 (用户收藏商品)
CREATE TABLE favorites (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    product_id  BIGINT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uk_favorite UNIQUE (user_id, product_id)
);
CREATE INDEX idx_favorites_user ON favorites(user_id, created_at DESC);

-- 站内通知: 支付成功 / 发货 / 会员升级 / 评论回复等履约与系统消息
CREATE TABLE notifications (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind        VARCHAR(24) NOT NULL
        CHECK (kind IN ('payment_success','shipped','membership_upgraded','reply','system')),
    title       VARCHAR(128) NOT NULL,
    body        TEXT,
    read        BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_notifications_user ON notifications(user_id, read, created_at DESC);
