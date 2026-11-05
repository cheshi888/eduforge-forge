"use client";

import { useEffect, useState } from "react";
import { useLocale, useTranslations } from "next-intl";
import Link from "next/link";
import type { CatalogProduct } from "@/lib/catalog";
import {
  fetchReviews,
  fetchQuestions,
  submitReview,
  submitQuestion,
  toggleFavorite,
  type Review,
  type Question
} from "@/lib/community";

export function ProductDetailView({ product }: { product: CatalogProduct }) {
  const locale = useLocale();
  const t = useTranslations("shop");
  const [reviews, setReviews] = useState<Review[]>([]);
  const [questions, setQuestions] = useState<Question[]>([]);
  const [favorited, setFavorited] = useState(false);
  const [rating, setRating] = useState(5);
  const [reviewText, setReviewText] = useState("");
  const [questionText, setQuestionText] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const load = () => {
    Promise.all([fetchReviews(product.sku), fetchQuestions(product.sku)])
      .then(([r, q]) => {
        setReviews(r);
        setQuestions(q);
      })
      .catch(() => {});
  };

  useEffect(load, [product.sku]);

  const doFavorite = async () => {
    setBusy("fav");
    try {
      const f = await toggleFavorite(product.sku);
      setFavorited(f);
      setErr(null);
    } catch (e) {
      setErr(String((e as { message?: string })?.message ?? e));
    } finally {
      setBusy(null);
    }
  };

  const doReview = async () => {
    setBusy("review");
    try {
      await submitReview(product.sku, rating, reviewText);
      setReviewText("");
      setErr(null);
      load();
    } catch (e) {
      setErr(String((e as { message?: string })?.message ?? e));
    } finally {
      setBusy(null);
    }
  };

  const doQuestion = async () => {
    setBusy("q");
    try {
      await submitQuestion(product.sku, questionText);
      setQuestionText("");
      setErr(null);
      load();
    } catch (e) {
      setErr(String((e as { message?: string })?.message ?? e));
    } finally {
      setBusy(null);
    }
  };

  const price = Number(BigInt(product.priceMicro) / BigInt(10 ** 6)).toLocaleString("zh-CN", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2
  });

  return (
    <div>
      {/* 商品头部 */}
      <div className="rounded-2xl border border-zinc-800/80 bg-zinc-900/50 p-6 backdrop-blur-sm">
        <div className="flex items-start justify-between">
          <div>
            <span className="text-[11px] uppercase tracking-wider text-zinc-500">
              {product.category}
            </span>
            <h1 className="mt-1 text-2xl font-bold tracking-tight text-zinc-100">
              {product.name}
            </h1>
            <div className="mt-2 flex items-center gap-3 text-sm text-zinc-500">
              <span className="text-amber-400">★ {Number(product.rating).toFixed(1)}</span>
              <span>{t("sold", { n: product.sold })}</span>
            </div>
          </div>
          <div className="text-right">
            <div className="text-2xl font-bold text-zinc-100">¥{price}</div>
          </div>
        </div>
        <p className="mt-4 text-sm leading-relaxed text-zinc-400">{product.desc}</p>
        <div className="mt-5 flex items-center gap-3">
          <Link
            href={`/${locale}/checkout?item=${product.sku}`}
            className="rounded-xl bg-gradient-to-r from-indigo-600 via-purple-600 to-indigo-600 px-5 py-2.5 text-sm font-semibold text-white shadow-[0_0_20px_rgba(99,102,241,0.35)] transition hover:shadow-[0_0_28px_rgba(99,102,241,0.5)]"
          >
            {t("buyNow")}
          </Link>
          <button
            onClick={doFavorite}
            disabled={busy === "fav"}
            className={
              "rounded-xl border px-4 py-2.5 text-sm font-medium transition disabled:opacity-50 " +
              (favorited
                ? "border-amber-500/50 bg-amber-500/15 text-amber-300"
                : "border-zinc-700 bg-zinc-900/50 text-zinc-300 hover:border-amber-500/40 hover:text-amber-300")
            }
          >
            {favorited ? t("favorited") : t("favorite")}
          </button>
        </div>
      </div>

      {err ? (
        <p className="mt-4 rounded-lg border border-rose-500/30 bg-rose-500/15 px-4 py-3 text-sm text-rose-400">
          {err}
        </p>
      ) : null}

      {/* 评论区 */}
      <section className="mt-8">
        <h2 className="text-lg font-semibold text-zinc-100">{t("reviews")} ({reviews.length})</h2>
        <div className="mt-4 rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-4">
          <div className="flex flex-wrap items-center gap-3">
            <div className="flex gap-1">
              {[1, 2, 3, 4, 5].map((n) => (
                <button
                  key={n}
                  onClick={() => setRating(n)}
                  className={"text-lg " + (n <= rating ? "text-amber-400" : "text-zinc-600")}
                >
                  ★
                </button>
              ))}
            </div>
            <input
              value={reviewText}
              onChange={(e) => setReviewText(e.target.value)}
              placeholder={t("reviewPlaceholder")}
              className="min-w-0 flex-1 rounded-lg border border-zinc-800 bg-zinc-900/50 px-3 py-2 text-sm text-zinc-200 outline-none placeholder:text-zinc-600 focus:border-indigo-500/60"
            />
            <button
              onClick={doReview}
              disabled={busy === "review" || reviewText.trim() === ""}
              className="rounded-lg bg-indigo-600 px-4 py-2 text-xs font-semibold text-white transition hover:bg-indigo-500 disabled:opacity-50"
            >
              {t("publish")}
            </button>
          </div>
          <div className="mt-4 space-y-3">
            {reviews.length === 0 ? (
              <p className="text-sm text-zinc-600">{t("noReviews")}</p>
            ) : (
              reviews.map((r) => (
                <div key={r.id} className="rounded-lg bg-zinc-900/60 p-3">
                  <div className="flex items-center justify-between text-xs">
                    <span className="text-zinc-400">
                      {"★".repeat(r.rating)}
                      <span className="text-zinc-600">{"★".repeat(5 - r.rating)}</span>
                    </span>
                    <span className="text-zinc-600">{r.userUid}</span>
                  </div>
                  {r.content ? (
                    <p className="mt-1.5 text-sm text-zinc-300">{r.content}</p>
                  ) : null}
                </div>
              ))
            )}
          </div>
        </div>
      </section>

      {/* 问答区 */}
      <section className="mt-8">
        <h2 className="text-lg font-semibold text-zinc-100">{t("questions")} ({questions.length})</h2>
        <div className="mt-4 rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-4">
          <div className="flex flex-wrap gap-3">
            <input
              value={questionText}
              onChange={(e) => setQuestionText(e.target.value)}
              placeholder={t("questionPlaceholder")}
              className="min-w-0 flex-1 rounded-lg border border-zinc-800 bg-zinc-900/50 px-3 py-2 text-sm text-zinc-200 outline-none placeholder:text-zinc-600 focus:border-indigo-500/60"
            />
            <button
              onClick={doQuestion}
              disabled={busy === "q" || questionText.trim() === ""}
              className="rounded-lg bg-indigo-600 px-4 py-2 text-xs font-semibold text-white transition hover:bg-indigo-500 disabled:opacity-50"
            >
              {t("ask")}
            </button>
          </div>
          <div className="mt-4 space-y-3">
            {questions.length === 0 ? (
              <p className="text-sm text-zinc-600">{t("noQuestions")}</p>
            ) : (
              questions.map((q) => (
                <div key={q.id} className="rounded-lg bg-zinc-900/60 p-3">
                  <div className="text-sm font-medium text-zinc-200">Q: {q.question}</div>
                  {q.answer ? (
                    <div className="mt-1.5 text-sm text-indigo-300">A: {q.answer}</div>
                  ) : (
                    <div className="mt-1.5 text-xs text-zinc-600">{t("pendingAnswer")}</div>
                  )}
                  <div className="mt-1 text-[11px] text-zinc-600">{q.userUid}</div>
                </div>
              ))
            )}
          </div>
        </div>
      </section>
    </div>
  );
}
