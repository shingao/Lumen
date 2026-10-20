import type { Locale } from "../core/format";
import { useStore } from "../app/store";
import { en } from "./en";
import { fr, type MessageKey } from "./fr";

export type { MessageKey };

const MESSAGES: Record<Locale, Record<MessageKey, string>> = { fr, en };

export type Params = Record<string, string | number>;

export function translate(locale: Locale, key: MessageKey, params?: Params): string {
  const text = MESSAGES[locale][key];
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (match, name: string) => (name in params ? String(params[name]) : match));
}

/** Clé de pluriel : français, 0 et 1 au singulier ; anglais, 1 seulement. */
export function pluralKey(locale: Locale, base: string, n: number): MessageKey {
  const one = locale === "fr" ? n < 2 : n === 1;
  return `${base}.${one ? "one" : "other"}` as MessageKey;
}

export interface Translator {
  (key: MessageKey, params?: Params): string;
  plural(base: string, n: number, params?: Params): string;
  locale: Locale;
}

export function makeTranslator(locale: Locale): Translator {
  const t = ((key: MessageKey, params?: Params) => translate(locale, key, params)) as Translator;
  t.plural = (base, n, params) => translate(locale, pluralKey(locale, base, n), { n, ...params });
  t.locale = locale;
  return t;
}

const translators: Record<Locale, Translator> = { fr: makeTranslator("fr"), en: makeTranslator("en") };

export function useT(): Translator {
  const locale = useStore((s) => s.locale);
  return translators[locale];
}

/** Hors composants (commandes, toasts). */
export function currentT(): Translator {
  return translators[useStore.getState().locale];
}
