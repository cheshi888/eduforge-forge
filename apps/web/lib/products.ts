// 虚拟商品清单 (知识付费项目: 源码 / 教程 / 卡密 / 资源包)
// 金额以 6 位小数微单位 (int64) 存储, 与后端契约对齐; 汇率以十进制字符串
export interface Product {
  id: string;
  name: string;
  category: "source" | "tutorial" | "card" | "resource";
  categoryLabel: string;
  priceMicro: number;
  currency: string;
  cover: string;
  desc: string;
  badge?: string;
  rating: number;
  sold: number;
}

const MICRO = 10 ** 6;

export const PRODUCTS: Product[] = [
  {
    id: "src-spring-cloud",
    name: "Spring Cloud 微服务实战源码 (含网关/注册中心)",
    category: "source",
    categoryLabel: "源码",
    priceMicro: 299 * MICRO,
    currency: "CNY",
    cover: "src",
    desc: "5 个模块 + Docker Compose 一键部署, 含 README 与架构图",
    badge: "HOT",
    rating: 4.9,
    sold: 1284
  },
  {
    id: "tut-rust",
    name: "Rust 系统级编程 30 讲视频 + 课件",
    category: "tutorial",
    categoryLabel: "教程",
    priceMicro: 199 * MICRO,
    currency: "CNY",
    cover: "vid",
    desc: "30 节高清视频, 附每讲 exercises 与答案",
    rating: 4.8,
    sold: 876
  },
  {
    id: "card-figma",
    name: "Figma 专业版年卡密 (自动发货)",
    category: "card",
    categoryLabel: "卡密",
    priceMicro: 120 * MICRO,
    currency: "CNY",
    cover: "key",
    desc: "365 天有效期, 支付成功后邮箱自动发卡",
    badge: "自动发货",
    rating: 5.0,
    sold: 3421
  },
  {
    id: "res-prompt",
    name: "AI 提示词工程资源包 (500 条 + 模板)",
    category: "resource",
    categoryLabel: "资源包",
    priceMicro: 59 * MICRO,
    currency: "CNY",
    cover: "pack",
    desc: "按场景分类的提示词集合, 附使用说明与对比案例",
    rating: 4.7,
    sold: 2109
  },
  {
    id: "src-llm",
    name: "LLM 应用开发模板仓库 (RAG + Agent)",
    category: "source",
    categoryLabel: "源码",
    priceMicro: 459 * MICRO,
    currency: "CNY",
    cover: "src",
    desc: "RAG 检索 / 多 Agent 编排 / 评估管线, 可跑 demo",
    badge: "NEW",
    rating: 4.9,
    sold: 567
  },
  {
    id: "tut-k8s",
    name: "Kubernetes 生产落地实战 (含故障演练)",
    category: "tutorial",
    categoryLabel: "教程",
    priceMicro: 329 * MICRO,
    currency: "CNY",
    cover: "vid",
    desc: "Helm / GitOps / 可观测性 / 混沌工程, 含实验室手册",
    rating: 4.8,
    sold: 643
  },
  {
    id: "card-midjourney",
    name: "Midjourney 月卡密 (海外直连)",
    category: "card",
    categoryLabel: "卡密",
    priceMicro: 89 * MICRO,
    currency: "CNY",
    cover: "key",
    desc: "30 天 Pro 档, 支付后 10 分钟内发卡",
    rating: 4.9,
    sold: 1892
  },
  {
    id: "res-design",
    name: "UI 设计系统素材包 (Figma + 组件库)",
    category: "resource",
    categoryLabel: "资源包",
    priceMicro: 149 * MICRO,
    currency: "CNY",
    cover: "pack",
    desc: "80+ 组件 + 3 套主题, 附使用规范文档",
    rating: 4.6,
    sold: 934
  }
];

export function getProduct(id: string): Product | undefined {
  return PRODUCTS.find((p) => p.id === id);
}
