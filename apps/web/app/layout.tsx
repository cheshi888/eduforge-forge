import type { ReactNode } from "react";

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="zh">
      <body className="min-h-screen bg-[#0a0c14] text-zinc-100 antialiased">{children}</body>
    </html>
  );
}
