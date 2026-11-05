-- 004_auth: 用户认证 — 密码哈希 + 服务端会话令牌
-- 前置: 001_init.sql 已应用 (users 表已存在). 本文件只加列与新建会话表, 不动既有约束.

-- 用户认证字段: Argon2id 密码哈希 (PHC 字符串格式 $argon2id$v=19$...)
ALTER TABLE users ADD COLUMN password_hash VARCHAR(255);
ALTER TABLE users ADD COLUMN email_verified BOOLEAN NOT NULL DEFAULT FALSE;

-- 服务端会话 (opaque bearer token): 撤销即失效, 无 JWT 密钥泄露风险.
-- token_hash 存 SHA-256, 防 DB 拖库后 token 明文外泄 (同 card_keys.key_hash 思路).
CREATE TABLE auth_sessions (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id       BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash    VARCHAR(64) NOT NULL UNIQUE,          -- hex(SHA-256(token))
    expires_at    TIMESTAMPTZ NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_auth_sessions_user ON auth_sessions(user_id);
CREATE INDEX idx_auth_sessions_expires ON auth_sessions(expires_at);
