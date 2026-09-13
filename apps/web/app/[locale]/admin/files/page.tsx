import { AdminShell } from "@/components/admin/admin-shell";
import { AdminFilesView } from "@/components/admin/files-view";

export default function AdminFilesPage() {
  return (
    <AdminShell module="files">
      <AdminFilesView />
    </AdminShell>
  );
}
