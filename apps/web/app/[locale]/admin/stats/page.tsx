import { AdminShell } from "@/components/admin/admin-shell";
import { StatsView } from "@/components/admin/stats-view";

export default function AdminStatsPage() {
  return (
    <AdminShell module="stats">
      <StatsView />
    </AdminShell>
  );
}
