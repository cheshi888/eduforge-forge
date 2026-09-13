"use client";

import { useCallback, useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import {
  adminListFiles,
  adminUploadFileMeta,
  listProducts,
  AdminApiError,
  type UploadedFile
} from "@/lib/admin-api";
import { Card, Skeleton, ErrorBox, EmptyState, GHOST, FIELD } from "./ui";

export function AdminFilesView() {
  const t = useTranslations("admin");
  const [files, setFiles] = useState<UploadedFile[] | null>(null);
  const [products, setProducts] = useState<Awaited<ReturnType<typeof listProducts>>>([]);
  const [error, setError] = useState<AdminApiError | Error | null>(null);
  const [loading, setLoading] = useState(true);

  // 上传表单
  const [productId, setProductId] = useState("");
  const [fileName, setFileName] = useState("");
  const [storageKey, setStorageKey] = useState("");
  const [sizeBytes, setSizeBytes] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [fs, ps] = await Promise.all([adminListFiles(), listProducts()]);
      setFiles(fs);
      setProducts(ps);
    } catch (e) {
      setError(e as AdminApiError | Error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const upload = async () => {
    const pid = Number(productId);
    if (!pid || !fileName.trim() || !storageKey.trim()) return;
    setBusy(true);
    setMsg(null);
    try {
      await adminUploadFileMeta({
        productId: pid,
        fileName: fileName.trim(),
        storageKey: storageKey.trim(),
        sizeBytes: sizeBytes ? Number(sizeBytes) : undefined
      });
      setMsg(t("filesUploaded"));
      setFileName("");
      setStorageKey("");
      setSizeBytes("");
      await load();
    } catch (e) {
      setMsg((e as AdminApiError | Error).message);
    } finally {
      setBusy(false);
    }
  };

  const productTitle = (pid: number) => {
    const p = products.find((x) => x.id === pid);
    return p ? `${p.sku} · ${p.title}` : `#${pid}`;
  };

  return (
    <div>
      {error ? (
        <ErrorBox error={error} onRetry={() => void load()} />
      ) : (
        <>
          <Card title={t("uploadTitle")}>
            <div className="grid grid-cols-1 gap-3 p-5 sm:grid-cols-2">
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">{t("productId")}</label>
                <select
                  value={productId}
                  onChange={(e) => setProductId(e.target.value)}
                  className={FIELD}
                >
                  <option value="">{t("selectProduct")}</option>
                  {products.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.sku} · {p.title}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">{t("fileName")}</label>
                <input
                  value={fileName}
                  onChange={(e) => setFileName(e.target.value)}
                  placeholder="tutorial-react.zip"
                  className={FIELD}
                />
              </div>
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">{t("storageKey")}</label>
                <input
                  value={storageKey}
                  onChange={(e) => setStorageKey(e.target.value)}
                  placeholder="s3://bucket/products/12/tutorial-react.zip"
                  className={FIELD}
                />
              </div>
              <div>
                <label className="mb-1 block text-[11px] text-zinc-500">{t("sizeBytes")}</label>
                <input
                  type="number"
                  value={sizeBytes}
                  onChange={(e) => setSizeBytes(e.target.value)}
                  placeholder="可选"
                  className={FIELD}
                />
              </div>
              <div className="flex items-center gap-3 sm:col-span-2">
                <button
                  type="button"
                  disabled={busy || !productId || !fileName.trim() || !storageKey.trim()}
                  onClick={() => void upload()}
                  className="rounded-lg border border-indigo-500/50 bg-gradient-to-r from-indigo-500/25 to-purple-500/25 px-4 py-2 text-xs font-semibold text-indigo-200 transition hover:from-indigo-500/40 hover:to-purple-500/40 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {busy ? t("uploading") : t("upload")}
                </button>
                {msg && (
                  <span className={`text-[11px] ${msg === t("filesUploaded") ? "text-emerald-400" : "text-rose-400"}`}>
                    {msg}
                  </span>
                )}
              </div>
            </div>
          </Card>

          <Card className="mt-4" title={t("filesListTitle")}>
            {loading ? (
              <Skeleton rows={6} />
            ) : files && files.length > 0 ? (
              <div className="overflow-x-auto">
                <table className="w-full text-left text-xs">
                  <thead>
                    <tr className="text-[11px] uppercase tracking-wider text-zinc-600">
                      <th className="px-5 py-2.5 font-medium">{t("th.file")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.product")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.storage")}</th>
                      <th className="px-3 py-2.5 font-medium">{t("th.size")}</th>
                      <th className="px-5 py-2.5 text-right font-medium">{t("th.status")}</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-zinc-800/60">
                    {files.map((f) => (
                      <tr key={f.id} className="transition hover:bg-zinc-800/30">
                        <td className="px-5 py-3 font-mono text-[11px] text-zinc-300">{f.fileName}</td>
                        <td className="px-3 py-3 text-[11px] text-zinc-400">{productTitle(f.productId)}</td>
                        <td className="px-3 py-3 font-mono text-[11px] text-zinc-600">{f.storageKey}</td>
                        <td className="px-3 py-3 font-mono text-[11px] text-zinc-500">
                          {f.sizeBytes != null ? `${(f.sizeBytes / 1024 / 1024).toFixed(2)} MB` : "—"}
                        </td>
                        <td className="px-5 py-3 text-right">
                          <span
                            className={`rounded-md border px-2 py-0.5 text-[11px] ${
                              f.active
                                ? "border-emerald-500/40 bg-emerald-500/12 text-emerald-400"
                                : "border-zinc-600/50 bg-zinc-600/12 text-zinc-500"
                            }`}
                          >
                            {f.active ? t("th.active") : t("th.inactive")}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <EmptyState>{t("noFiles")}</EmptyState>
            )}
          </Card>
        </>
      )}
    </div>
  );
}
