import { getLocale } from "next-intl/server";
import { AuthForm } from "@/components/auth/auth-form";

export default async function AuthPage() {
  const locale = await getLocale();
  return (
    <main className="mx-auto max-w-md px-6 py-16">
      <AuthForm locale={locale} />
    </main>
  );
}
