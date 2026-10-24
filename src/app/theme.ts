// Thème et langue effectifs : réglage de l'utilisateur, ou ceux de Windows (« Système »).

import { detectLocale } from "../core/format";
import { backend } from "../ipc/client";
import type { Settings, ThemePref } from "../ipc/types";
import { useStore } from "./store";

/** Clé lue par public/theme-boot.js avant le premier rendu (évite un flash). */
export const THEME_CACHE_KEY = "lumen-theme";

const darkQuery = typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null;

function resolveDark(pref: ThemePref): boolean {
  if (pref === "system") return darkQuery?.matches ?? false;
  return pref === "dark";
}

let switchTimer: number | undefined;

function applyTheme(dark: boolean, animate: boolean): void {
  const root = document.documentElement;
  const next = dark ? "dark" : "light";
  // theme-boot.js a pu poser le thème avant le premier rendu : l'état suit quand même.
  useStore.setState({ dark });
  if (root.dataset.theme === next) return;
  if (animate) {
    root.classList.add("theme-switching");
    window.clearTimeout(switchTimer);
    // Un peu plus que --dur-theme : la classe ne doit pas couper la transition en cours.
    switchTimer = window.setTimeout(() => root.classList.remove("theme-switching"), 300);
  }
  root.dataset.theme = next;
}

function applyLocale(settings: Settings): void {
  const locale = settings.language === "system" ? detectLocale(navigator.languages) : settings.language;
  document.documentElement.lang = locale;
  backend.setLocale(locale);
  useStore.setState({ locale });
}

function apply(settings: Settings, animate: boolean): void {
  applyTheme(resolveDark(settings.theme), animate);
  applyLocale(settings);
  try {
    localStorage.setItem(THEME_CACHE_KEY, settings.theme);
  } catch {
    // Sans cache, theme-boot.js retombe sur le thème du système.
  }
}

export async function initAppearance(): Promise<void> {
  const settings = await backend.settings();
  useStore.setState({ settings });
  apply(settings, false);
  darkQuery?.addEventListener("change", () => {
    const s = useStore.getState().settings;
    if (s?.theme === "system") applyTheme(resolveDark("system"), true);
  });
  window.addEventListener("languagechange", () => {
    const s = useStore.getState().settings;
    if (s) applyLocale(s);
  });
}

export async function updateSettings(patch: Partial<Settings>): Promise<void> {
  const settings = await backend.updateSettings(patch);
  useStore.setState({ settings });
  apply(settings, true);
}

/** Bascule rapide de la barre de titre : passe à l'opposé du thème affiché. */
export function toggleTheme(): Promise<void> {
  return updateSettings({ theme: useStore.getState().dark ? "light" : "dark" });
}
