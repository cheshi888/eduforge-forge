import { AdminShell } from "@/components/admin/admin-shell";
import { FraudView } from "@/components/admin/fraud-view";

export default function AdminFraudPage() {
  return (
    <AdminShell module="fraud">
      <FraudView />
    </AdminShell>
  );
}
