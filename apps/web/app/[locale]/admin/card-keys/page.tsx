import { AdminShell } from "@/components/admin/admin-shell";
import { CardKeysView } from "@/components/admin/card-keys-view";

export default function AdminCardKeysPage() {
  return (
    <AdminShell module="card-keys">
      <CardKeysView />
    </AdminShell>
  );
}
