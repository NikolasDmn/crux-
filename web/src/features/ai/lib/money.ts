// AI spending in the user's own currency (Settings > AI > Spending). Providers bill in USD and the
// ai service keeps the cap in micro-USD; we only show and read amounts in another currency, at
// fixed approximate rates (the cap is a budget guard, not an invoice). USD shows exact.

/** USD per 1 unit of the currency, and how many decimals amounts get. Approximate. */
export const CURRENCIES = {
  USD: { usd: 1, decimals: 2 },
  EUR: { usd: 1.08, decimals: 2 },
  GBP: { usd: 1.27, decimals: 2 },
  CHF: { usd: 1.13, decimals: 2 },
  CAD: { usd: 0.73, decimals: 2 },
  AUD: { usd: 0.66, decimals: 2 },
  NZD: { usd: 0.6, decimals: 2 },
  SEK: { usd: 0.095, decimals: 2 },
  NOK: { usd: 0.093, decimals: 2 },
  DKK: { usd: 0.145, decimals: 2 },
  PLN: { usd: 0.25, decimals: 2 },
  CZK: { usd: 0.043, decimals: 2 },
  JPY: { usd: 0.0067, decimals: 0 },
  CNY: { usd: 0.14, decimals: 2 },
  INR: { usd: 0.012, decimals: 0 },
  BRL: { usd: 0.19, decimals: 2 },
} as const;

export type Currency = keyof typeof CURRENCIES;

const BY_REGION: Record<string, Currency> = {
  US: "USD",
  GB: "GBP",
  CH: "CHF",
  CA: "CAD",
  AU: "AUD",
  NZ: "NZD",
  SE: "SEK",
  NO: "NOK",
  DK: "DKK",
  PL: "PLN",
  CZ: "CZK",
  JP: "JPY",
  CN: "CNY",
  IN: "INR",
  BR: "BRL",
  ...Object.fromEntries(
    [
      "AT",
      "BE",
      "CY",
      "DE",
      "EE",
      "ES",
      "FI",
      "FR",
      "GR",
      "HR",
      "IE",
      "IT",
      "LT",
      "LU",
      "LV",
      "MT",
      "NL",
      "PT",
      "SI",
      "SK",
    ].map((r) => [r, "EUR" as Currency]),
  ),
};

export function isCurrency(v: unknown): v is Currency {
  return typeof v === "string" && v in CURRENCIES;
}

/** The currency of a locale's region ("nl-NL" → EUR); USD when it can't tell. */
export function currencyFor(locale: string | undefined): Currency {
  const region = locale?.split(/[-_]/)[1]?.toUpperCase();
  return (region && BY_REGION[region]) || "USD";
}

/** micro-USD as an amount in `cur` ("≈ €4.63"; USD exact). */
export function money(microUsd: number, cur: Currency, locale = "en-GB"): string {
  const { usd, decimals } = CURRENCIES[cur];
  const amount = microUsd / 1_000_000 / usd;
  const small = amount > 0 && amount < 10 ** -decimals;
  const text = new Intl.NumberFormat(locale, {
    style: "currency",
    currency: cur,
    minimumFractionDigits: small ? undefined : decimals,
    maximumFractionDigits: small ? 4 : decimals,
  }).format(amount);
  return cur === "USD" || amount === 0 ? text : `≈ ${text}`;
}

/** A typed cap ("5", "2,50", "€ 3") in `cur` → micro-USD; "none" when empty; null when not an amount. */
export function capInput(text: string, cur: Currency): number | null | "none" {
  const s = text
    .trim()
    .replace(/^[^\p{L}\d]+/u, "") // a leading symbol ("€ 3"), not letters
    .replace(",", ".");
  if (!s) return "none";
  if (!/^\d+(\.\d{1,2})?$/.test(s)) return null;
  return Math.round(Number(s) * CURRENCIES[cur].usd * 1_000_000);
}

/** The stored cap shown for editing in `cur` ("4.63"); "" for none. */
export function capText(microUsd: number | null | undefined, cur: Currency): string {
  if (!microUsd) return "";
  const { usd, decimals } = CURRENCIES[cur];
  return String(Number((microUsd / 1_000_000 / usd).toFixed(decimals)));
}
