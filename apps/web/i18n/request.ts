import { getRequestConfig } from "next-intl/server";
import { routing } from "./routing";

export default getRequestConfig(async (requestLocale) => {
  let locale = requestLocale.locale;
  if (!routing.locales.includes(locale as (typeof routing.locales)[number])) {
    locale = routing.defaultLocale;
  }
  const messages = await import(`../locales/${locale}.json`);
  return { locale, messages: messages.default };
});
