"use client";

import { useEffect, useState } from "react";
import { useTranslations, useLocale } from "next-intl";
import Link from "next/link";
import {
  fetchTiers,
  fetchProfile,
  upgradeTier,
  type Tier,
  type UserTier
} from "@/lib/membership-api";
import { formatMicro } from "@/lib/amount";

export function MembershipView() {
  const locale = useLocale();
  const t = useTranslations("membership");
  const [tiers, setTiers] = useState<Tier[]>([]);
  const [profile, setProfile] = useState<UserTier | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([fetchTiers(), fetchProfile()])
      .then(([tiers, profile]) => {
        setTiers(tiers);
        setProfile(profile);
      })
      .catch((e) => setErr(String(e?.message ?? e)));
  }, []);

  const doUpgrade = async (code: string) => {
    setBusy(code);
    try {
      await upgradeTier(code);
      setErr(null);
      const p = await fetchProfile();
      setProfile(p);
    } catch (e) {
      setErr(String((e as { message?: string })?.message ?? e));
    } finally {
      setBusy(null);
    }
  };

  const activeTier = tiers.find((x) => x.code === profile?.tierCode);

  return (
    <div>
      <header className="mb-6">
        <h1 className="text-2xl font-bold tracking-tight text-zinc-100">
          会员中心
          <span className="ml-3 bg-gradient-to-r from-indigo-400 to-purple-400 bg-clip-text text-xs font-medium text-transparent">
            Membership
          </span>
        </h1>
        <p className="mt-1 text-sm text-zinc-500">{t("subtitle")}</p>
      </header>

      {err ? (
        <p className="mb-4 rounded-lg border border-rose-500/30 bg-rose-500/15 px-4 py-3 text-sm text-rose-400">
          {err}
        </p>
      ) : null}

      {profile ? (
        <div className="mb-6 rounded-2xl border border-indigo-500/40 bg-gradient-to-br from-indigo-500/10 to-purple-500/10 p-5 backdrop-blur-md">
          <div className="flex items-center justify-between">
            <div>
              <div className="text-[11px] uppercase tracking-wider text-zinc-500">
                {t("currentTier")}
              </div>
              <div className="text-xl font-bold text-zinc-100">
                {activeTier?.name ?? profile.tierCode}
                <span className="ml-2 text-xs font-normal text-zinc-500">
                  {profile.tierExpiresAt ?? t("freeForever")}
                </span>
              </div>
            </div>
            <div className="text-right">
              <div className="text-[11px] text-zinc-500">{t("totalSpent")}</div>
              <div className="font-mono text-lg text-zinc-200">
                {formatMicro(profile.totalSpentMicro, locale, "CNY")}
              </div>
            </div>
          </div>
          {profile.perkLabels.length > 0 ? (
            <div className="mt-4 flex flex-wrap gap-1.5">
              {profile.perkLabels.map((k) => (
                <span
                  key={k}
                  className="rounded-full border border-indigo-500/30 bg-indigo-500/12 px-2.5 py-0.5 text-[11px] text-indigo-300"
                >
                  {k}
                </span>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        {tiers.map((tier) => {
          const isActive = profile?.tierCode === tier.code;
          const isUpgradable =
            !isActive &&
            (tier.code === "plus" || tier.code === "pro") &&
            (profile?.tierCode === "free" ||
              (tier.code === "pro" && profile?.tierCode === "plus"));
          return (
            <div
              key={tier.code}
              className={
                "relative overflow-hidden rounded-2xl border p-5 transition " +
                (isActive
                  ? "border-emerald-500/50 bg-emerald-500/8 shadow-[0_0_24px_rgba(52,211,153,0.2)]"
                  : "border-zinc-800/80 bg-zinc-900/50 backdrop-blur-sm")
              }
            >
              <div className="flex items-center justify-between">
                <span className="text-lg font-semibold text-zinc-100">{tier.name}</span>
                {isActive ? (
                  <span className="rounded-full bg-emerald-500/20 px-2.5 py-0.5 text-[11px] font-medium text-emerald-400">
                    {t("active")}
                  </span>
                ) : null}
              </div>
              <div className="mt-2 flex items-baseline gap-0.5">
                <span className="text-xs text-zinc-500">{t("price")}</span>
                <span className="text-2xl font-bold text-zinc-100">
                  {formatMicro(tier.priceMicro, locale, "CNY")}
                </span>
                {tier.durationDays > 0 ? (
                  <span className="ml-1 text-xs text-zinc-500">
                    / {t("days")}{tier.durationDays}
                  </span>
                ) : null}
              </div>
              <ul className="mt-4 space-y-1.5 text-xs text-zinc-400">
                <li>{t("discount")}: {tier.discountPct}%</li>
                <li>
                  {t("downloads")}: {tier.maxDownloads >= 99 ? t("unlimited") : tier.maxDownloads}
                </li>
                {tier.perks.map((p) => (
                  <li key={p} className="text-indigo-300/80">
                    · {p}
                  </li>
                ))}
              </ul>
              {isUpgradable ? (
                <button
                  onClick={() => doUpgrade(tier.code)}
                  disabled={busy === tier.code}
                  className="mt-5 w-full rounded-xl bg-gradient-to-r from-indigo-600 via-purple-600 to-indigo-600 px-4 py-2.5 text-sm font-semibold text-white shadow-[0_0_18px_rgba(99,102,241,0.35)] transition hover:shadow-[0_0_26px_rgba(99,102,241,0.5)] disabled:opacity-50 disabled:shadow-none"
                >
                  {busy === tier.code ? t("upgrading") : t("upgrade")}
                </button>
              ) : null}
            </div>
          );
        })}
      </div>

      <p className="mt-6 text-center text-xs text-zinc-600">
        <Link href={`/${locale}/my-files`} className="text-indigo-400 hover:text-indigo-300">
          {t("goMyFiles")} →
        </Link>
      </p>
    </div>
  );
}
