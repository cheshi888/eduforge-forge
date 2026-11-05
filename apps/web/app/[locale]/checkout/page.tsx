import { getLocale } from "next-intl/server";
import { CheckoutForm } from "@/components/checkout/checkout-form";
import { fetchOrderSummary, createOrderSummary } from "@/lib/api";
import { getSessionToken } from "@/lib/auth";
import { fetchCatalogProduct, type CatalogProduct } from "@/lib/catalog";

export default async function CheckoutPage({
  searchParams
}: {
  searchParams: Promise<{ item?: string; qty?: string; coupon?: string }>;
}) {
  const locale = await getLocale();
  const sp = await searchParams;
  const quantity = Math.max(1, parseInt(sp.qty ?? "1", 10) || 1);
  const token = await getSessionToken();

  // 无会话时无法下单 (后端用户侧端点一律 Bearer 鉴权)
  if (!token) {
    return (
      <main className="mx-auto max-w-xl px-6 py-12">
        <p className="rounded-lg border border-rose-500/30 bg-rose-500/15 px-4 py-3 text-sm text-rose-400">
          Please sign in to checkout.
        </p>
      </main>
    );
  }

  // 带 item (SKU) 时: 拉取商品详情用于展示 + 真实下单 (后端按会员等级折扣 + 优惠券锁定折后金额).
  // 无 item 时回退到读取最新 PENDING 订单 (兼容旧入口 / 直接访问结算页).
  const item: CatalogProduct | undefined = sp.item
    ? (await fetchCatalogProduct(sp.item)) ?? undefined
    : undefined;
  const order = item
    ? await createOrderSummary([{ sku: item.sku, quantity }], token, sp.coupon)
    : await fetchOrderSummary(token);

  return (
    <main className="mx-auto max-w-xl px-6 py-12">
      {order ? (
        <CheckoutForm order={order} locale={locale} item={item} />
      ) : (
        // M2-06: 后端不可用时显式提示, 不渲染演示订单
        <p className="rounded-lg border border-rose-500/30 bg-rose-500/15 px-4 py-3 text-sm text-rose-400">
          Payment service unavailable. Please retry later.
        </p>
      )}
    </main>
  );
}

