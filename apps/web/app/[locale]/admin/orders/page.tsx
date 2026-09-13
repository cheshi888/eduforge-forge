import { AdminShell } from "@/components/admin/admin-shell";
import { OrdersView } from "@/components/admin/orders-view";

export default function AdminOrdersPage() {
  return (
    <AdminShell module="orders">
      <OrdersView />
    </AdminShell>
  );
}
