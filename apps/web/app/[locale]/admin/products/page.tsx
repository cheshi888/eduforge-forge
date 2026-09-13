import { AdminShell } from "@/components/admin/admin-shell";
import { ProductsView } from "@/components/admin/products-view";

export default function AdminProductsPage() {
  return (
    <AdminShell module="products">
      <ProductsView />
    </AdminShell>
  );
}
