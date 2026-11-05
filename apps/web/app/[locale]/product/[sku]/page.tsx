import { notFound } from "next/navigation";
import { fetchCatalogProduct } from "@/lib/catalog";
import { ProductDetailView } from "@/components/shop/product-detail-view";

export default async function ProductDetailPage({
  params
}: {
  params: Promise<{ sku: string }>;
}) {
  const { sku } = await params;
  const product = await fetchCatalogProduct(sku);
  if (!product) {
    notFound();
  }
  return (
    <main className="mx-auto max-w-4xl px-6 py-10">
      <ProductDetailView product={product} />
    </main>
  );
}
