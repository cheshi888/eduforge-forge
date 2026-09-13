import { getLocale } from "next-intl/server";
import { CheckoutForm } from "@/components/checkout/checkout-form";
import { fetchOrderSummary } from "@/lib/api";
import { getProduct, type Product } from "@/lib/products";

export default async function CheckoutPage({
  searchParams
}: {
  searchParams: Promise<{ item?: string }>;
}) {
  const locale = await getLocale();
  const order = await fetchOrderSummary();
  const sp = await searchParams;
  const item: Product | undefined = sp.item ? getProduct(sp.item) : undefined;
  return (
    <main className="mx-auto max-w-xl px-6 py-12">
      {order ? (
        <CheckoutForm order={order} locale={locale} item={item} />
      ) : (
        // M2-06: 后端不可用时显式提示, 不渲染演示订单 (mock 需 FORGE_MOCK_FALLBACK=1 才回退)
        <p className="rounded-lg border border-rose-500/30 bg-rose-500/15 px-4 py-3 text-sm text-rose-400">
          Payment service unavailable. Please retry later.
        </p>
      )}
    </main>
  );
}

