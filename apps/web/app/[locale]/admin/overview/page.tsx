import { AdminShell } from "@/components/admin/admin-shell";
import { OverviewView } from "@/components/admin/overview-view";

export default function AdminOverviewPage() {
  return (
    <AdminShell module="overview">
      <OverviewView />
    </AdminShell>
  );
}
