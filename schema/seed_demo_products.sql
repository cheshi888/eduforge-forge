-- seed_demo_products.sql: 演示商品目录种子数据 (与前端演示商品对齐)
-- 前置: 001~005 schema 已应用. 幂等: 用 ON CONFLICT (sku) 跳过已存在.
-- 金额 NUMERIC(20,6) 即元, 6 位小数; 前端 priceMicro = 元 * 10^6.

INSERT INTO products (sku, title, category, cover, description, unit_price, currency, stock, rating, sold_count, badge) VALUES
    ('src-spring-cloud', 'Spring Cloud 微服务实战源码 (含网关/注册中心)', 'source', 'src',
     '5 个模块 + Docker Compose 一键部署, 含 README 与架构图', 299, 'CNY', 9999, 4.9, 1284, 'HOT'),
    ('tut-rust', 'Rust 系统级编程 30 讲视频 + 课件', 'tutorial', 'vid',
     '30 节高清视频, 附每讲 exercises 与答案', 199, 'CNY', 9999, 4.8, 876, NULL),
    ('card-figma', 'Figma 专业版年卡密 (自动发货)', 'card', 'key',
     '365 天有效期, 支付成功后邮箱自动发卡', 120, 'CNY', 9999, 5.0, 3421, '自动发货'),
    ('res-prompt', 'AI 提示词工程资源包 (500 条 + 模板)', 'resource', 'pack',
     '按场景分类的提示词集合, 附使用说明与对比案例', 59, 'CNY', 9999, 4.7, 2109, NULL),
    ('src-llm', 'LLM 应用开发模板仓库 (RAG + Agent)', 'source', 'src',
     'RAG 检索 / 多 Agent 编排 / 评估管线, 可跑 demo', 459, 'CNY', 9999, 4.9, 567, 'NEW'),
    ('tut-k8s', 'Kubernetes 生产落地实战 (含故障演练)', 'tutorial', 'vid',
     'Helm / GitOps / 可观测性 / 混沌工程, 含实验室手册', 329, 'CNY', 9999, 4.8, 643, NULL),
    ('card-midjourney', 'Midjourney 月卡密 (海外直连)', 'card', 'key',
     '30 天 Pro 档, 支付后 10 分钟内发卡', 89, 'CNY', 9999, 4.9, 1892, NULL),
    ('res-design', 'UI 设计系统素材包 (Figma + 组件库)', 'resource', 'pack',
     '80+ 组件 + 3 套主题, 附使用规范文档', 149, 'CNY', 9999, 4.6, 934, NULL)
ON CONFLICT (sku) DO NOTHING;

-- 演示用户 (无密码, 仅用于 e2e seed 场景; 真实用户走 /auth/register)
INSERT INTO users (uid, email, preferred_language) VALUES
    ('u-demo', 'demo@eduforge.local', 'zh')
ON CONFLICT (uid) DO NOTHING;
