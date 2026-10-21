// Formatage des tailles, nombres et dates, selon la langue de l'interface.
// Tailles en base 1024, comme l'Explorateur Windows : 1 Go = 1024³ octets.

export type Locale = "fr" | "en";

const UNITS: Record<Locale, readonly string[]> = {
  fr: ["o", "Ko", "Mo", "Go", "To", "Po"],
  en: ["B", "KB", "MB", "GB", "TB", "PB"],
};

const INTL: Record<Locale, string> = { fr: "fr-FR", en: "en-US" };

export const KIB = 1024;
export const MIB = KIB ** 2;
export const GIB = KIB ** 3;

/** Taille lisible : 13,4 Go / 13.4 GB. Une décimale sous 100, aucune au-delà (comme la maquette). */
export function formatBytes(bytes: number, locale: Locale): string {
  const units = UNITS[locale];
  let value = Math.abs(bytes);
  let unit = 0;
  while (value >= KIB && unit < units.length - 1) {
    value /= KIB;
    unit++;
  }
  // Évite « 1024,0 Mo » après arrondi : on passe à l'unité suivante.
  if (unit < units.length - 1 && Number(value.toFixed(1)) >= KIB) {
    value /= KIB;
    unit++;
  }
  const decimals = unit === 0 || value >= 100 ? 0 : 1;
  const text = new Intl.NumberFormat(INTL[locale], {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  }).format(value);
  return `${text} ${units[unit]}`;
}

/** Gain ou perte : « −12,3 Go », « +44,2 Go » (vrai signe moins U+2212). */
export function formatDelta(bytes: number, locale: Locale): string {
  const sign = bytes < 0 ? "−" : "+";
  return `${sign}${formatBytes(bytes, locale)}`;
}

/** Nombre entier groupé : « 412 380 » / « 412,380 ». */
export function formatCount(n: number, locale: Locale): string {
  return new Intl.NumberFormat(INTL[locale], { maximumFractionDigits: 0 }).format(n);
}

export function formatPercent(ratio: number, locale: Locale): string {
  return new Intl.NumberFormat(INTL[locale], { style: "percent", maximumFractionDigits: 0 }).format(ratio);
}

const DAY = 86_400_000;

function startOfDay(t: number): number {
  const d = new Date(t);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/**
 * Date relative comme dans la maquette : « aujourd'hui, 11:42 », « hier, 22:10 »,
 * « il y a 3 jours », « il y a 2 semaines », « il y a 8 mois ».
 */
export function formatRelative(time: number, locale: Locale, now: number = Date.now()): string {
  const intl = INTL[locale];
  const hm = new Intl.DateTimeFormat(intl, { hour: "2-digit", minute: "2-digit" }).format(time);
  const days = Math.round((startOfDay(now) - startOfDay(time)) / DAY);
  const words = locale === "fr" ? { today: "aujourd'hui", yesterday: "hier" } : { today: "today", yesterday: "yesterday" };
  if (days <= 0) {
    const minutes = Math.round((now - time) / 60_000);
    if (minutes >= 0 && minutes < 120) {
      const rtf = new Intl.RelativeTimeFormat(intl, { numeric: "always" });
      return minutes < 60 ? rtf.format(-Math.max(1, minutes), "minute") : rtf.format(-Math.round(minutes / 60), "hour");
    }
    return `${words.today}, ${hm}`;
  }
  if (days === 1) return `${words.yesterday}, ${hm}`;
  const rtf = new Intl.RelativeTimeFormat(intl, { numeric: "always" });
  if (days < 7) return rtf.format(-days, "day");
  if (days < 30) return rtf.format(-Math.round(days / 7), "week");
  if (days < 365) return rtf.format(-Math.max(1, Math.round(days / 30.44)), "month");
  return rtf.format(-Math.round(days / 365.25), "year");
}

/** Date courte : « 12/06/2026 » / « 06/12/2026 ». */
export function formatDate(time: number, locale: Locale): string {
  return new Intl.DateTimeFormat(INTL[locale], { day: "2-digit", month: "2-digit", year: "numeric" }).format(time);
}

/** Jour et mois abrégé, pour les axes : « 21 oct. » / « Oct 21 ». */
export function formatDay(time: number, locale: Locale): string {
  return new Intl.DateTimeFormat(INTL[locale], { day: "numeric", month: "short" }).format(time);
}

/** Date et heure, pour choisir un snapshot : « 21/10/2026 14:05 ». */
export function formatDateTime(time: number, locale: Locale): string {
  return new Intl.DateTimeFormat(INTL[locale], { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit" }).format(time);
}

/**
 * Langue de l'interface d'après les langues du système, dans leur ordre de préférence.
 * Le français est la langue de référence : c'est aussi le repli si aucune langue n'est reconnue.
 */
export function detectLocale(languages: readonly string[]): Locale {
  for (const lang of languages) {
    const l = lang.toLowerCase();
    if (l.startsWith("fr")) return "fr";
    if (l.startsWith("en")) return "en";
  }
  return "fr";
}
